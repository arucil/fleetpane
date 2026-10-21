//! A single bounded snapshot per endpoint. Never queries the provider or claims a lease.
use super::{Configuration, Endpoint, Error};
use fleetpane_runtime::{
    FaultScope, Lease, PROTOCOL_VERSION, Request, Response, Snapshot, lifecycle,
};
use serde::Serialize;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub endpoint: Endpoint,
    pub checked_at_unix_ms: u128,
    pub elapsed_ms: u128,
    pub expected_protocol: u32,
    #[serde(flatten)]
    pub result: ResultState,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ResultState {
    Reachable { observation: Box<Observation> },
    Unavailable { error: String },
}

/// Deliberately excludes provider initialization data, prompts, request payloads,
/// session IDs and operation IDs. Errors and local process/endpoint identity remain inspectable.
#[derive(Clone, Debug, Serialize)]
pub struct Observation {
    pub protocol: u32,
    pub host_id: Uuid,
    pub host_pid: u32,
    pub provider_pid: u32,
    pub owner: Option<Lease>,
    pub admission: lifecycle::Admission,
    pub sequence: u64,
    pub journal_total: u64,
    pub active_turns: usize,
    pub pending_requests: usize,
    pub in_flight_operations: usize,
    pub unknown_operations: u64,
    pub terminals: usize,
    /// None means the Host does not report authoritative Stop progress.
    pub stopping: Option<usize>,
    pub fault: Option<String>,
    pub fault_scope: Option<FaultScope>,
}

impl From<Snapshot> for Observation {
    fn from(snapshot: Snapshot) -> Self {
        Self {
            protocol: snapshot.protocol,
            host_id: snapshot.host_id,
            host_pid: snapshot.host_pid,
            provider_pid: snapshot.provider_pid,
            owner: snapshot.owner,
            admission: snapshot.admission,
            sequence: snapshot.sequence,
            journal_total: snapshot.journal_total,
            active_turns: snapshot.activity.turns.len(),
            pending_requests: snapshot.pending_requests.len(),
            in_flight_operations: snapshot.activity.in_flight_operations,
            unknown_operations: snapshot.activity.unknown_operations,
            terminals: snapshot.activity.terminal_ids.len(),
            stopping: snapshot.activity.stopping.map(|stops| stops.len()),
            fault: snapshot.fault,
            fault_scope: snapshot.fault_scope,
        }
    }
}

impl Report {
    pub fn label(&self) -> &'static str {
        match &self.result {
            ResultState::Unavailable { .. } => "Unavailable",
            ResultState::Reachable { observation } => {
                if observation.protocol != self.expected_protocol {
                    "Incompatible protocol"
                } else if observation.fault.is_some() {
                    match observation.fault_scope {
                        Some(FaultScope::Provider) => "Provider fault",
                        Some(FaultScope::Host) => "Host fault",
                        None => "Fault · scope unreported",
                    }
                } else if observation.admission.mode == lifecycle::Mode::Draining {
                    "Draining"
                } else {
                    "Reachable"
                }
            }
        }
    }
}

/// Checks configured endpoints concurrently and returns configuration order.
/// This is an observation, never evidence that a Host can be retired or updated.
pub async fn collect(configuration: Configuration) -> Result<Vec<Report>, Error> {
    configuration.validate()?;
    let mut tasks = tokio::task::JoinSet::new();
    for (index, endpoint) in configuration.hosts.into_iter().enumerate() {
        tasks.spawn(async move {
            let checked_at_unix_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| Error(format!("cannot timestamp Host check: {error}")))?
                .as_millis();
            let started = Instant::now();
            let result = match fleetpane_runtime::unix::request(
                &endpoint.socket,
                &Request::Snapshot,
            )
            .await
            {
                Ok(Response::Snapshot(snapshot)) => ResultState::Reachable {
                    observation: Box::new(snapshot.into()),
                },
                Ok(Response::Error { code, message }) => ResultState::Unavailable {
                    error: format!("{code}: {message}"),
                },
                Ok(_) => ResultState::Unavailable {
                    error: "Host did not return a snapshot".into(),
                },
                Err(error) => ResultState::Unavailable {
                    error: error.to_string(),
                },
            };
            Ok((
                index,
                Report {
                    endpoint,
                    checked_at_unix_ms,
                    elapsed_ms: started.elapsed().as_millis(),
                    expected_protocol: PROTOCOL_VERSION,
                    result,
                },
            ))
        });
    }
    let mut reports = Vec::new();
    while let Some(result) = tasks.join_next().await {
        reports.push(
            result.map_err(|error| Error(format!("Host diagnostic task failed: {error}")))??,
        );
    }
    reports.sort_by_key(|(index, _)| *index);
    Ok(reports.into_iter().map(|(_, report)| report).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn report() -> Report {
        let snapshot =
            fleetpane_runtime::State::new(42, json!({"secret":"private-init"})).snapshot();
        Report {
            endpoint: Endpoint {
                name: "开发".into(),
                socket: "/tmp/diagnostics.sock".into(),
            },
            checked_at_unix_ms: 100,
            elapsed_ms: 2,
            expected_protocol: PROTOCOL_VERSION,
            result: ResultState::Reachable {
                observation: Box::new(snapshot.into()),
            },
        }
    }

    #[test]
    fn reachability_drain_fault_and_compatibility_are_separate() {
        let mut report = report();
        assert_eq!(report.label(), "Reachable");
        let ResultState::Reachable { observation } = &mut report.result else {
            panic!()
        };
        observation.admission.mode = lifecycle::Mode::Draining;
        assert_eq!(report.label(), "Draining");
        for (scope, label) in [
            (None, "Fault · scope unreported"),
            (Some(FaultScope::Provider), "Provider fault"),
            (Some(FaultScope::Host), "Host fault"),
        ] {
            let ResultState::Reachable { observation } = &mut report.result else {
                panic!()
            };
            observation.fault = Some("explicit failure".into());
            observation.fault_scope = scope;
            assert_eq!(report.label(), label);
        }
        let ResultState::Reachable { observation } = &mut report.result else {
            panic!()
        };
        observation.protocol += 1;
        assert_eq!(report.label(), "Incompatible protocol");
        report.result = ResultState::Unavailable {
            error: "socket missing".into(),
        };
        assert_eq!(report.label(), "Unavailable");
    }

    #[test]
    fn diagnostics_keep_counts_without_provider_and_request_payloads() {
        let mut snapshot =
            fleetpane_runtime::State::new(42, json!({"secret":"private-init"})).snapshot();
        snapshot
            .pending_requests
            .push(json!({"id":"private-request","params":{"prompt":"private-prompt"}}));
        snapshot.activity.turns.push(lifecycle::ActiveTurn {
            thread_id: "private-thread".into(),
            turn_id: Some("private-turn".into()),
        });
        snapshot.activity.unknown_operations = 7;
        snapshot.activity.stopping = None;
        let observation = Observation::from(snapshot);
        assert_eq!(observation.active_turns, 1);
        assert_eq!(observation.pending_requests, 1);
        assert_eq!(observation.unknown_operations, 7);
        assert_eq!(observation.stopping, None);
        let text = serde_json::to_string(&observation).unwrap();
        assert!(!text.contains("private-"));
        assert!(!text.contains("provider_info"));
    }
}
