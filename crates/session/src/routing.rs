//! Read-only recommendations inside explicit Host groups. Never claims a lease,
//! sends a mutation, retries creation or transfers an existing session.
use crate::hosts::{Configuration, Endpoint, Probe, ProbeResult};
use fleetpane_runtime::{Snapshot, lifecycle::Mode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
#[error("Host selection: {0}")]
pub struct Error(String);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub endpoint: Endpoint,
    pub host_id: Uuid,
    pub admission_generation: u64,
}

impl Target {
    /// Check a cached view or a fresh snapshot; dispatch still needs the normal
    /// Host lease and admission gate. A recommendation is never a reservation.
    pub fn check(&self, state: &Snapshot) -> Result<(), Error> {
        if state.host_id != self.host_id {
            return Err(Error(
                "recommended Host restarted; find a ready Host again".into(),
            ));
        }
        if state.fault.is_some()
            || state.admission.mode != Mode::Ready
            || state.admission.generation != self.admission_generation
        {
            return Err(Error(
                "recommended Host availability changed; find a ready Host again".into(),
            ));
        }
        Ok(())
    }

    pub async fn confirm(&self) -> Result<(), Error> {
        let state = crate::client::snapshot(&self.endpoint.socket)
            .await
            .map_err(|error| Error(error.to_string()))?;
        self.check(&state)
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Availability {
    Ready {
        host_id: Uuid,
        admission_generation: u64,
        has_controller: bool,
        active_turns: usize,
    },
    Draining {
        active_turns: usize,
        pending_answers: usize,
    },
    Unavailable {
        error: String,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct Candidate {
    pub endpoint: Endpoint,
    #[serde(flatten)]
    pub availability: Availability,
}

impl Candidate {
    pub fn label(&self) -> String {
        let name = &self.endpoint.name;
        match &self.availability {
            Availability::Ready { active_turns, .. } => {
                format!("{name}: ready · {active_turns} active")
            }
            Availability::Draining {
                active_turns,
                pending_answers,
            } => format!("{name}: draining · {active_turns} active · {pending_answers} pending"),
            Availability::Unavailable { error } => format!("{name}: unavailable · {error}"),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Selection {
    pub route: String,
    pub target: Option<Target>,
    pub candidates: Vec<Candidate>,
}

fn select_observed(route: String, probes: Vec<Probe>) -> Result<Selection, Error> {
    let mut identities = BTreeSet::new();
    let mut target = None;
    let mut candidates = Vec::with_capacity(probes.len());
    for probe in probes {
        let availability = match probe.result {
            ProbeResult::Unavailable { error } => Availability::Unavailable { error },
            ProbeResult::Available { snapshot } => {
                if !identities.insert(snapshot.host_id) {
                    return Err(Error("multiple group endpoints resolve to the same Host; correct the configuration".into()));
                }
                if let Some(error) = snapshot.fault {
                    Availability::Unavailable { error }
                } else if snapshot.admission.mode == Mode::Draining {
                    Availability::Draining {
                        active_turns: snapshot.activity.turns.len(),
                        pending_answers: snapshot.pending_requests.len(),
                    }
                } else {
                    if target.is_none() {
                        target = Some(Target {
                            endpoint: probe.endpoint.clone(),
                            host_id: snapshot.host_id,
                            admission_generation: snapshot.admission.generation,
                        });
                    }
                    Availability::Ready {
                        host_id: snapshot.host_id,
                        admission_generation: snapshot.admission.generation,
                        has_controller: snapshot.owner.is_some(),
                        active_turns: snapshot.activity.turns.len(),
                    }
                }
            }
        };
        candidates.push(Candidate {
            endpoint: probe.endpoint,
            availability,
        });
    }
    Ok(Selection {
        route,
        target,
        candidates,
    })
}

/// Probe only explicitly named members, in parallel. Selection follows the
/// group's configured priority, independent of response timing or UI tab order.
pub async fn select(configuration: &Configuration, route: &str) -> Result<Selection, Error> {
    configuration
        .validate()
        .map_err(|error| Error(error.to_string()))?;
    let group = configuration
        .routes
        .iter()
        .find(|group| group.name == route)
        .ok_or_else(|| Error(format!("unknown route group: {route}")))?;
    let hosts = group
        .hosts
        .iter()
        .map(|name| {
            configuration
                .hosts
                .iter()
                .find(|host| host.name == *name)
                .expect("validated route membership")
                .clone()
        })
        .collect();
    let probes = crate::hosts::probe(Configuration {
        version: 1,
        hosts,
        routes: Vec::new(),
    })
    .await
    .map_err(|error| Error(error.to_string()))?;
    select_observed(route.to_owned(), probes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleetpane_runtime::{
        PROTOCOL_VERSION,
        lifecycle::{Activity, Admission},
    };
    use serde_json::json;

    fn snapshot(mode: Mode) -> Snapshot {
        Snapshot {
            session_directories: None,
            protocol: PROTOCOL_VERSION,
            host_id: Uuid::new_v4(),
            host_pid: 10,
            provider_pid: 11,
            provider_info: json!({}),
            owner: None,
            sequence: 0,
            operation_ids: Vec::new(),
            journal_total: 0,
            admission: Admission {
                generation: 3,
                mode,
            },
            activity: Activity {
                stopping: Some(Vec::new()),
                terminal_ids: Vec::new(),
                turns: Vec::new(),
                in_flight_operations: 0,
                unknown_operations: 0,
            },
            pending_requests: Vec::new(),
            attention: Some(Box::default()),
            fault: None,
            fault_scope: None,
        }
    }

    fn available(name: &str, state: Snapshot) -> Probe {
        Probe {
            endpoint: Endpoint {
                name: name.into(),
                socket: format!("/tmp/{name}/control.sock").into(),
            },
            result: ProbeResult::Available {
                snapshot: Box::new(state),
            },
        }
    }

    #[test]
    fn priority_skips_drain_fault_and_offline_without_leaving_group() {
        let mut fault = snapshot(Mode::Ready);
        fault.fault = Some("provider exited".into());
        let offline = Probe {
            endpoint: Endpoint {
                name: "Offline".into(),
                socket: "/tmp/offline/control.sock".into(),
            },
            result: ProbeResult::Unavailable {
                error: "not connected".into(),
            },
        };
        let selected = select_observed(
            "Codex".into(),
            vec![
                available("Old", snapshot(Mode::Draining)),
                offline,
                available("Fault", fault),
                available("New", snapshot(Mode::Ready)),
                available("Later", snapshot(Mode::Ready)),
            ],
        )
        .unwrap();
        assert_eq!(selected.target.unwrap().endpoint.name, "New");
        assert_eq!(
            selected
                .candidates
                .iter()
                .map(|c| c.endpoint.name.as_str())
                .collect::<Vec<_>>(),
            ["Old", "Offline", "Fault", "New", "Later"]
        );
        let none = select_observed(
            "Codex".into(),
            vec![available("Old", snapshot(Mode::Draining))],
        )
        .unwrap();
        assert!(none.target.is_none());
        assert_eq!(none.candidates.len(), 1);
    }

    #[test]
    fn stale_incarnation_admission_and_duplicate_aliases_are_rejected() {
        let ready = snapshot(Mode::Ready);
        let selected =
            select_observed("Codex".into(), vec![available("New", ready.clone())]).unwrap();
        let target = selected.target.unwrap();
        target.check(&ready).unwrap();
        let mut changed = ready.clone();
        changed.admission.generation += 1;
        assert!(target.check(&changed).is_err());
        changed = ready.clone();
        changed.admission.mode = Mode::Draining;
        assert!(target.check(&changed).is_err());
        assert!(target.check(&snapshot(Mode::Ready)).is_err());
        changed = ready.clone();
        changed.fault = Some("provider exited".into());
        assert!(target.check(&changed).is_err());
        assert!(
            select_observed(
                "Codex".into(),
                vec![available("A", ready.clone()), available("B", ready)]
            )
            .is_err()
        );
    }
}
