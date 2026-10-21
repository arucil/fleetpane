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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Approval,
    Question,
    Other,
}

#[derive(Clone, Debug, Serialize)]
pub struct Item {
    pub endpoint: Endpoint,
    pub host_id: Uuid,
    pub id: Value,
    pub method: String,
    pub kind: Kind,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub title: String,
    pub request: Value,
}

impl Item {
    fn from_request(endpoint: Endpoint, host_id: Uuid, request: Value) -> Result<Self, Error> {
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
            endpoint,
            host_id,
            id,
            method,
            kind,
            thread_id,
            turn_id,
            title,
            request,
        })
    }

    /// Recheck the incarnation and original request, including changed details.
    pub fn validate(&self, snapshot: &Snapshot) -> Result<(), Error> {
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
    pub endpoint: Endpoint,
    pub host_id: Option<Uuid>,
    pub items: Vec<Item>,
    pub error: Option<String>,
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
            endpoint,
            host_id: None,
            items: vec![],
            error: Some(error),
        },
        ProbeResult::Available { snapshot } => {
            let items = snapshot
                .pending_requests
                .iter()
                .cloned()
                .map(|request| Item::from_request(endpoint.clone(), snapshot.host_id, request))
                .collect::<Result<Vec<_>, _>>();
            match items {
                Ok(items) => Host {
                    endpoint,
                    host_id: Some(snapshot.host_id),
                    items,
                    error: snapshot.fault.clone(),
                },
                Err(error) => Host {
                    endpoint,
                    host_id: Some(snapshot.host_id),
                    items: vec![],
                    error: Some(error.to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn endpoint(name: &str) -> Endpoint {
        Endpoint {
            name: name.into(),
            socket: format!("/tmp/{name}/control.sock").into(),
        }
    }
    fn snapshot() -> Snapshot {
        fleetpane_runtime::State::new(42, json!({"synthetic":true}))
            .snapshot()
            .clone()
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
}
