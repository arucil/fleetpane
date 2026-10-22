//! On-demand pending-request inventory. Observation never claims or answers a Host.
use crate::{
    client::{self, Error},
    hosts::{self, Configuration, Endpoint, Probe, ProbeResult},
};
use fleetpane_runtime::Snapshot;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

pub mod navigation;
pub mod review;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Approval,
    Question,
    Other,
}

#[derive(Clone, Debug, Serialize)]
pub struct Item {
    pub request_sequence: Option<u64>,
    pub observed_unix_ms: Option<u64>,
    pub endpoint: Endpoint,
    pub host_id: Uuid,
    pub id: Value,
    pub method: String,
    pub kind: Kind,
    pub thread_id: Option<String>,
    pub cwd: Option<String>,
    pub turn_id: Option<String>,
    pub title: String,
    pub request: Value,
}

impl Item {
    /// Time since this Host observed the occurrence; never provider creation age.
    pub fn observation_label(&self, now_unix_ms: u64) -> String {
        match self.observed_unix_ms {
            None => "Host observation time unavailable".into(),
            Some(time) if time > now_unix_ms => {
                "Host observation time is ahead of local clock".into()
            }
            Some(time) => {
                let seconds = (now_unix_ms - time) / 1000;
                if seconds < 60 {
                    format!("Host observed {seconds}s ago")
                } else if seconds < 3600 {
                    format!("Host observed {}m ago", seconds / 60)
                } else {
                    format!("Host observed {}h ago", seconds / 3600)
                }
            }
        }
    }

    pub(crate) fn from_request(
        endpoint: Endpoint,
        host_id: Uuid,
        request: Value,
    ) -> Result<Self, Error> {
        let id = request
            .get("id")
            .filter(|id| id.is_string() || id.is_number())
            .cloned()
            .ok_or_else(|| invalid("pending request lacks its original ID"))?;
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .filter(|method| !method.is_empty())
            .ok_or_else(|| invalid("pending request lacks its method"))?
            .to_owned();
        let kind = match method.as_str() {
            "session/request_permission"
            | "item/commandExecution/requestApproval"
            | "item/fileChange/requestApproval" => Kind::Approval,
            crate::questions::METHOD => Kind::Question,
            _ => Kind::Other,
        };
        let string = |pointer| {
            request
                .pointer(pointer)
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
        };
        let thread_id = string("/params/threadId");
        let turn_id = string("/params/turnId");
        let title = string("/params/toolCall/title")
            .or_else(|| string("/params/command"))
            .or_else(|| string("/params/questions/0/question"))
            .or_else(|| string("/params/reason"))
            .unwrap_or_else(|| match kind {
                Kind::Approval => "Tool approval".into(),
                Kind::Question => "Question".into(),
                Kind::Other => method.clone(),
            });
        Ok(Self {
            request_sequence: None,
            observed_unix_ms: None,
            endpoint,
            host_id,
            id,
            method,
            kind,
            thread_id,
            cwd: None,
            turn_id,
            title,
            request,
        })
    }

    /// Recheck the incarnation and original request, including changed details.
    pub fn validate(&self, snapshot: &Snapshot) -> Result<(), Error> {
        fleetpane_runtime::pending::validate_snapshot(snapshot).map_err(invalid)?;
        if snapshot.host_id != self.host_id {
            return Err(invalid("Host was replaced; refresh the queue"));
        }
        if let Some(error) = &snapshot.fault {
            return Err(invalid(format!("Host is unavailable: {error}")));
        }
        if !snapshot
            .pending_requests
            .iter()
            .any(|request| request == &self.request)
        {
            return Err(invalid(
                "request was resolved or changed; refresh the queue",
            ));
        }
        if let Some(sequence) = self.request_sequence
            && !snapshot
                .pending_observations
                .as_ref()
                .is_some_and(|observations| {
                    observations
                        .iter()
                        .any(|o| o.id == self.id && o.sequence == sequence)
                })
        {
            return Err(invalid("request occurrence changed; refresh the queue"));
        }
        if self.thread_id.is_none() {
            return Err(invalid(
                "request has no session target; inspect its original payload",
            ));
        }
        Ok(())
    }

    pub async fn confirm(&self) -> Result<(), Error> {
        self.validate(&client::snapshot(&self.endpoint.socket).await?)
    }
}

#[derive(Debug, Serialize)]
pub struct Host {
    /// Complete pending snapshot boundary, absent on legacy/unavailable Hosts.
    pub pending_sequence: Option<u64>,
    pub endpoint: Endpoint,
    pub host_id: Option<Uuid>,
    pub items: Vec<Item>,
    pub error: Option<String>,
    pub completions: Vec<review::Item>,
    /// Missing on an older Host, not an empty supported feed.
    pub completion_evicted: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    /// Observation time, not the provider's request creation time.
    pub observed_unix_ms: u64,
    pub hosts: Vec<Host>,
}

fn invalid(message: impl std::fmt::Display) -> Error {
    Error::Input(format!("Attention: {message}"))
}

fn project(probe: Probe) -> Host {
    let endpoint = probe.endpoint;
    match probe.result {
        ProbeResult::Unavailable { error } => Host {
            pending_sequence: None,
            endpoint,
            host_id: None,
            items: vec![],
            error: Some(error),
            completions: vec![],
            completion_evicted: None,
        },
        ProbeResult::Available { snapshot } => {
            let completions = snapshot
                .attention
                .as_ref()
                .map(|feed| {
                    feed.records
                        .iter()
                        .rev()
                        .map(|record| review::Item {
                            endpoint: endpoint.clone(),
                            host_id: snapshot.host_id,
                            record: record.clone(),
                            reviewed: false,
                            cwd: snapshot
                                .session_directories
                                .as_ref()
                                .and_then(|feed| feed.directory(&record.thread_id))
                                .map(str::to_owned),
                        })
                        .collect()
                })
                .unwrap_or_default();
            let completion_evicted = snapshot.attention.as_ref().map(|feed| feed.evicted);
            let items = fleetpane_runtime::pending::validate_snapshot(&snapshot)
                .map_err(invalid)
                .and_then(|()| {
                    snapshot
                        .pending_requests
                        .iter()
                        .cloned()
                        .map(|request| {
                            let mut item =
                                Item::from_request(endpoint.clone(), snapshot.host_id, request)?;
                            if let Some(observations) = &snapshot.pending_observations {
                                let observation =
                                    observations.iter().find(|o| o.id == item.id).ok_or_else(
                                        || invalid("Host omitted a pending request occurrence"),
                                    )?;
                                if observation.sequence == 0
                                    || observation.sequence > snapshot.sequence
                                {
                                    return Err(invalid(
                                        "Host has an invalid pending request occurrence",
                                    ));
                                }
                                item.request_sequence = Some(observation.sequence);
                                item.observed_unix_ms = Some(observation.observed_unix_ms);
                            }
                            item.cwd = item
                                .thread_id
                                .as_ref()
                                .and_then(|id| {
                                    snapshot
                                        .session_directories
                                        .as_ref()
                                        .and_then(|feed| feed.directory(id))
                                })
                                .map(str::to_owned);
                            Ok::<_, Error>(item)
                        })
                        .collect::<Result<Vec<_>, _>>()
                });
            match items {
                Ok(items) => Host {
                    pending_sequence: snapshot
                        .pending_observations
                        .as_ref()
                        .map(|_| snapshot.sequence),
                    endpoint,
                    host_id: Some(snapshot.host_id),
                    items,
                    error: snapshot.fault.clone(),
                    completions,
                    completion_evicted,
                },
                Err(error) => Host {
                    pending_sequence: None,
                    endpoint,
                    host_id: Some(snapshot.host_id),
                    items: vec![],
                    error: Some(error.to_string()),
                    completions,
                    completion_evicted,
                },
            }
        }
    }
}

pub async fn collect(configuration: Configuration) -> Result<Report, Error> {
    let hosts = hosts::probe(configuration)
        .await
        .map_err(invalid)?
        .into_iter()
        .map(project)
        .collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(invalid)?;
    let observed_unix_ms = now.as_millis().try_into().map_err(invalid)?;
    Ok(Report {
        observed_unix_ms,
        hosts,
    })
}

/// Completion review receipts are local to this controller store. Reading them
/// does not claim a Host and cannot alter pending requests or provider outcomes.
pub async fn collect_reviewed(
    configuration: Configuration,
    store: &crate::creations::Store,
) -> Result<Report, Error> {
    let mut report = collect(configuration).await?;
    for host in &mut report.hosts {
        host.completions = store
            .completion_reviews(std::mem::take(&mut host.completions))
            .await?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn occurrence_projection_refuses_reused_selection_and_reports_observation_age() {
        let mut state = fleetpane_runtime::State::new(42, json!({}));
        let request =
            json!({"id":1,"method":"session/request_permission","params":{"threadId":"same"}});
        state.record(request.clone()).unwrap();
        let snapshot = state.snapshot();
        let host = project(Probe {
            endpoint: endpoint("a"),
            result: ProbeResult::Available {
                snapshot: Box::new(snapshot.clone()),
            },
        });
        let item = &host.items[0];
        assert!(item.validate(&snapshot).is_ok());
        let time = item.observed_unix_ms.unwrap();
        assert_eq!(
            item.observation_label(time + 61_000),
            "Host observed 1m ago"
        );
        assert!(item.observation_label(time - 1).contains("ahead"));
        state.answered(&json!(1));
        state.record(request).unwrap();
        assert!(item.validate(&state.snapshot()).is_err());
        let mut damaged = snapshot;
        damaged.pending_observations.as_mut().unwrap().clear();
        let host = project(Probe {
            endpoint: endpoint("a"),
            result: ProbeResult::Available {
                snapshot: Box::new(damaged),
            },
        });
        assert!(host.error.is_some() && host.pending_sequence.is_none());
    }

    fn endpoint(name: &str) -> Endpoint {
        Endpoint {
            name: name.into(),
            socket: format!("/tmp/{name}/control.sock").into(),
        }
    }
    fn snapshot() -> Snapshot {
        let mut snapshot = fleetpane_runtime::State::new(42, json!({"synthetic":true})).snapshot();
        snapshot.pending_observations = None;
        snapshot
    }

    #[test]
    fn colliding_ids_remain_separate_hosts_and_unknown_requests_remain_visible() {
        let pending = json!({"id":1,"method":"session/request_permission","params":{"threadId":"same","turnId":"t","toolCall":{"title":" exact 中文  "}}});
        let mut first = snapshot();
        first.pending_requests.push(pending.clone());
        let mut second = snapshot();
        second.pending_requests.push(pending);
        let rows: Vec<_> = [("a", first), ("b", second)]
            .into_iter()
            .map(|(name, state)| {
                project(Probe {
                    endpoint: endpoint(name),
                    result: ProbeResult::Available {
                        snapshot: Box::new(state),
                    },
                })
            })
            .collect();
        assert_ne!(rows[0].items[0].host_id, rows[1].items[0].host_id);
        assert_eq!(rows[0].items[0].id, rows[1].items[0].id);
        assert_ne!(
            rows[0].items[0].endpoint.socket,
            rows[1].items[0].endpoint.socket
        );
        assert_eq!(rows[0].items[0].title, " exact 中文  ");
        let other = Item::from_request(
            endpoint("a"),
            Uuid::new_v4(),
            json!({"id":"1","method":"future/request","params":{}}),
        )
        .unwrap();
        assert_eq!(other.kind, Kind::Other);
        assert!(other.thread_id.is_none());
        assert!(other.request["params"].is_object());
    }

    #[test]
    fn navigation_requires_original_host_request_details_and_session() {
        let mut state = snapshot();
        let raw = json!({"id":"1","method":crate::questions::METHOD,"params":{"threadId":"s","questions":[{"question":"  选择实现语言？\n"}]}});
        state.pending_requests.push(raw.clone());
        let item = Item::from_request(endpoint("a"), state.host_id, raw).unwrap();
        assert_eq!(item.kind, Kind::Question);
        assert_eq!(item.title, "  选择实现语言？\n");
        item.validate(&state).unwrap();
        let mut replaced = state.clone();
        replaced.host_id = Uuid::new_v4();
        assert!(item.validate(&replaced).is_err());
        let mut changed = state.clone();
        changed.pending_requests[0]["params"]["threadId"] = json!("other");
        assert!(item.validate(&changed).is_err());
        let mut resolved = state.clone();
        resolved.pending_requests.clear();
        assert!(item.validate(&resolved).is_err());
        state.fault = Some("provider ended".into());
        assert!(item.validate(&state).is_err());
    }

    #[test]
    fn unavailable_and_malformed_snapshots_do_not_report_all_clear() {
        let host = project(Probe {
            endpoint: endpoint("offline"),
            result: ProbeResult::Unavailable {
                error: "offline".into(),
            },
        });
        assert!(host.error.is_some());
        assert!(host.host_id.is_none());
        let mut state = snapshot();
        state.pending_requests.push(json!({"method":"missing-id"}));
        let host = project(Probe {
            endpoint: endpoint("invalid"),
            result: ProbeResult::Available {
                snapshot: Box::new(state),
            },
        });
        assert!(host.error.is_some());
        assert!(host.host_id.is_some());
    }

    #[test]
    fn workspace_is_joined_by_original_session_metadata_without_trusting_request_cwd() {
        let mut state = snapshot();
        state.session_directories.as_mut().unwrap().record(
            fleetpane_runtime::session_directories::Record {
                thread_id: "s".into(),
                cwd: "/observed".into(),
            },
        );
        state.pending_requests.push(json!({"id":1,"method":"session/request_permission","params":{"threadId":"s","cwd":"/wrong","toolCall":{"title":"Synthetic"}}}));
        state.pending_requests.push(
            json!({"id":2,"method":"unknown","params":{"threadId":"missing","cwd":"/guess"}}),
        );
        state
            .attention
            .as_mut()
            .unwrap()
            .record(fleetpane_runtime::attention::Record {
                sequence: 1,
                observed_unix_ms: 42,
                thread_id: "s".into(),
                turn_id: "t".into(),
                kind: fleetpane_runtime::attention::Kind::Failed,
                stop_reason: None,
            });
        let host = project(Probe {
            endpoint: endpoint("same"),
            result: ProbeResult::Available {
                snapshot: Box::new(state.clone()),
            },
        });
        assert_eq!(host.items[0].cwd.as_deref(), Some("/observed"));
        assert_eq!(host.items[1].cwd, None);
        assert_eq!(host.completions[0].cwd.as_deref(), Some("/observed"));
        assert_eq!(host.items[0].request, state.pending_requests[0]);
        state.session_directories = None;
        let old = project(Probe {
            endpoint: endpoint("old"),
            result: ProbeResult::Available {
                snapshot: Box::new(state),
            },
        });
        assert!(old.items.iter().all(|i| i.cwd.is_none()));
        assert!(old.completions.iter().all(|i| i.cwd.is_none()));
    }
}
