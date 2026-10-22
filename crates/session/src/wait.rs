//! Observe one exact turn without taking control, sending, answering or stopping it.
use crate::{
    Turn,
    client::{Connection, Error},
    history,
};
use fleetpane_runtime::Snapshot;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeSet, time::Duration};
use uuid::Uuid;

pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;
pub const MAX_TIMEOUT_MS: u64 = 3_600_000;
const MAX_LOOKUP_PAGES: usize = 4096;
const POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Serialize)]
pub struct Report {
    pub host_id: Uuid,
    pub thread_id: String,
    pub turn_id: String,
    pub sequence: Option<u64>,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    Finished { turn: Turn },
    Unknown { turn: Turn },
    Attention { requests: Vec<Value> },
    TimedOut,
}

fn terminal(mut turn: Turn) -> Result<Option<Outcome>, Error> {
    // Waiting returns metadata, never a transcript or a task-success assertion.
    turn.items.clear();
    Ok(match turn.status.as_str() {
        "inProgress" => None,
        "completed" | "failed" | "interrupted" => Some(Outcome::Finished { turn }),
        "unknown" => Some(Outcome::Unknown { turn }),
        status => {
            return Err(Error::Protocol(format!(
                "Unknown turn status while waiting: {status}"
            )));
        }
    })
}

fn is_active(snapshot: &Snapshot, thread: &str, turn: &str) -> bool {
    snapshot
        .activity
        .turns
        .iter()
        .any(|active| active.thread_id == thread && active.turn_id.as_deref() == Some(turn))
}

fn attention(snapshot: &Snapshot, thread: &str, turn: &str) -> Vec<Value> {
    // ACP permissions have no turnId. Bind those to the Host's active turn;
    // Codex requests that include turnId must also match it exactly.
    if !is_active(snapshot, thread, turn) {
        return Vec::new();
    }
    snapshot
        .pending_requests
        .iter()
        .filter(|request| {
            let params = &request["params"];
            params["threadId"].as_str() == Some(thread)
                && params
                    .get("turnId")
                    .is_none_or(|id| id.as_str() == Some(turn))
        })
        .cloned()
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    data: Vec<Turn>,
    next_cursor: Option<String>,
}

impl Connection {
    /// The connection's existing lease fences initial history reads. Once the
    /// target is found in Host activity or history, Host events/snapshots
    /// survive controller replacement.
    /// Dropping this future cancels observation only. A finite deadline also
    /// covers initial history I/O and backlog draining.
    pub async fn wait_turn(
        &self,
        thread: &str,
        turn: &str,
        timeout_ms: u64,
    ) -> Result<Report, Error> {
        if [thread, turn]
            .iter()
            .any(|id| id.is_empty() || id.len() > 4096)
            || !(1..=MAX_TIMEOUT_MS).contains(&timeout_ms)
        {
            return Err(Error::Input(format!(
                "Wait requires session/turn IDs up to 4 KiB and timeout 1–{MAX_TIMEOUT_MS} ms"
            )));
        }
        let observed = tokio::time::timeout(
            Duration::from_millis(timeout_ms),
            self.observe_turn(thread, turn),
        )
        .await;
        let (sequence, outcome) = match observed {
            Ok(result) => {
                let (sequence, outcome) = result?;
                (Some(sequence), outcome)
            }
            Err(_) => (None, Outcome::TimedOut),
        };
        Ok(Report {
            host_id: self.lease.host_id,
            thread_id: thread.into(),
            turn_id: turn.into(),
            sequence,
            outcome,
        })
    }

    async fn observe_turn(&self, thread: &str, turn: &str) -> Result<(u64, Outcome), Error> {
        let baseline = crate::client::snapshot(&self.socket).await?;
        if baseline.host_id != self.lease.host_id {
            return Err(Error::Protocol(
                "Host incarnation changed before wait lookup".into(),
            ));
        }
        // A query reply's sequence is its arrival boundary, not a provider
        // snapshot version. Replay from before lookup so a completion emitted
        // before a stale active query reply is still observed.
        let mut after = baseline.sequence;
        // Codex acknowledges the first turn before its paginated history is
        // materialized. Exact Host activity already identifies that turn; do
        // not require a history read to start observing it.
        if !is_active(&baseline, thread, turn) {
            let (sequence, target) = self.lookup_wait_turn(thread, turn).await?;
            if let Some(outcome) = terminal(target)? {
                return Ok((sequence, outcome));
            }
        }
        loop {
            let (state, events) = self.poll(after).await?;
            if let Some(fault) = &state.fault {
                return Err(Error::Protocol(format!(
                    "Host fault while waiting: {fault}"
                )));
            }
            for event in &events {
                let message = &event.message;
                if message["method"] == "turn/completed"
                    && message["params"]["threadId"].as_str() == Some(thread)
                {
                    let completed = &message["params"]["turn"];
                    let id = completed["id"]
                        .as_str()
                        .ok_or_else(|| Error::Protocol("Turn completion has no identity".into()))?;
                    if id == turn {
                        let target: Turn =
                            serde_json::from_value(completed.clone()).map_err(|e| {
                                Error::Protocol(format!("Invalid waited turn completion: {e}"))
                            })?;
                        let outcome = terminal(target)?.ok_or_else(|| {
                            Error::Protocol("Turn completion is still in progress".into())
                        })?;
                        return Ok((event.sequence, outcome));
                    }
                }
            }
            let pending = attention(&state, thread, turn);
            if !pending.is_empty() {
                return Ok((state.sequence, Outcome::Attention { requests: pending }));
            }
            if let Some(last) = events.last() {
                after = last.sequence;
            }
            // Only delivered event boundaries advance the cursor. A snapshot can
            // be ahead of a bounded event page; drain that backlog immediately.
            if events.is_empty() || after >= state.sequence {
                tokio::time::sleep(POLL_INTERVAL).await;
            }
        }
    }

    async fn lookup_wait_turn(&self, thread: &str, turn: &str) -> Result<(u64, Turn), Error> {
        let mut cursor = None;
        let mut cursors = BTreeSet::new();
        let mut found = None;
        for _ in 0..MAX_LOOKUP_PAGES {
            let request = history::Request::Turns {
                thread_id: thread.into(),
                cursor: cursor.clone(),
            };
            let (method, params) = request.rpc().map_err(|e| Error::Input(e.to_string()))?;
            let reply = self.execute_rpc(method, params, Uuid::new_v4()).await?;
            let page: Page = serde_json::from_value(reply.result)
                .map_err(|e| Error::Protocol(format!("Invalid wait history page: {e}")))?;
            if let Some(target) = page.data.into_iter().find(|candidate| candidate.id == turn) {
                found = Some((reply.sequence, target));
                break;
            }
            cursor = page.next_cursor;
            let Some(next) = &cursor else {
                return Err(Error::Input(
                    "Turn was not found in this session's history".into(),
                ));
            };
            if !cursors.insert(next.clone()) {
                return Err(Error::Protocol("Wait history cursor repeated".into()));
            }
        }
        found.ok_or_else(|| {
            Error::Input(format!(
                "Turn lookup exceeded {MAX_LOOKUP_PAGES} history pages"
            ))
        })
    }
}

#[cfg(test)]
mod tests;
