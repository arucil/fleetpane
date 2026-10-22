//! Admission fences and observed activity, independent of provider/desktop APIs.
use crate::HostError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Ready,
    Draining,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Admission {
    pub generation: u64,
    pub mode: Mode,
}

impl Default for Admission {
    fn default() -> Self {
        Self {
            generation: 0,
            mode: Mode::Ready,
        }
    }
}

impl Admission {
    pub fn change(&self, expected: u64, mode: Mode) -> Result<Self, HostError> {
        if self.generation != expected {
            return Err(HostError::new(
                "admission_conflict",
                "admission generation changed; read snapshot",
            ));
        }
        if self.mode == mode {
            return Ok(self.clone());
        }
        let generation = self
            .generation
            .checked_add(1)
            .filter(|next| *next <= i64::MAX as u64)
            .ok_or_else(|| {
                HostError::new("generation_exhausted", "admission generation exhausted")
            })?;
        Ok(Self { generation, mode })
    }

    /// Reads use the separate query channel. A drained Host permits only the
    /// supported cancellation RPC; new and unclassified mutations fail closed.
    pub fn permits(&self, method: &str) -> bool {
        self.mode == Mode::Ready
            || matches!(
                method,
                "turn/interrupt"
                    | "fleetpane/terminal/input"
                    | "fleetpane/terminal/resize"
                    | "fleetpane/terminal/rename"
                    | "fleetpane/terminal/close"
            )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveTurn {
    pub thread_id: String,
    /// None means the provider reported activity before a turn identity.
    pub turn_id: Option<String>,
}

/// These are observed blockers, never a proof that provider resources are idle.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Activity {
    /// None identifies an older Host without authoritative Stop progress.
    #[serde(default)]
    pub stopping: Option<Vec<crate::stopping::Stop>>,
    #[serde(default)]
    pub terminal_ids: Vec<uuid::Uuid>,
    pub turns: Vec<ActiveTurn>,
    pub in_flight_operations: usize,
    pub unknown_operations: u64,
}

#[derive(Default)]
pub struct Tracker {
    turns: BTreeMap<String, Option<String>>,
    starting: BTreeMap<Uuid, Starting>,
}

struct Starting {
    thread: String,
    // A provider can finish a turn before replying to turn/start. Retain those
    // identities only while this admitted start awaits its reply, independently
    // of the bounded event ring and completion attention feed.
    completed: BTreeSet<String>,
}

impl Tracker {
    pub fn admitted(&mut self, id: Uuid, method: &str, params: &Value) {
        if method == "turn/start"
            && let Some(thread) = params.get("threadId").and_then(Value::as_str)
        {
            // Generic RPC admission leaves parameter validation to the
            // provider. Common typed sends supply this exact thread identity.
            self.starting.insert(
                id,
                Starting {
                    thread: thread.to_owned(),
                    completed: BTreeSet::new(),
                },
            );
        }
    }

    pub fn replied(&mut self, id: Uuid, message: &Value) -> Result<(), String> {
        let Some(starting) = self.starting.remove(&id) else {
            return Ok(());
        };
        let Some(result) = message.get("result") else {
            return Ok(()); // Rejection is not running activity.
        };
        if starting.thread.is_empty() || starting.thread.len() > 512 {
            return Err("turn/start reply belongs to an invalid thread identity".into());
        }
        let turn = result
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 512)
            .ok_or("turn/start reply has invalid turn identity")?;
        match result.pointer("/turn/status").and_then(Value::as_str) {
            Some("inProgress") if !starting.completed.contains(turn) => {
                self.started(starting.thread, turn.to_owned())?;
            }
            Some("inProgress" | "completed" | "failed" | "interrupted" | "unknown") => {}
            _ => return Err("turn/start reply has unsupported turn status".into()),
        }
        Ok(())
    }

    pub fn snapshot(&self) -> Vec<ActiveTurn> {
        self.turns
            .iter()
            .map(|(thread_id, turn_id)| ActiveTurn {
                thread_id: thread_id.clone(),
                turn_id: turn_id.clone(),
            })
            .collect()
    }

    pub fn record(&mut self, message: &Value) -> Result<(), String> {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return Ok(());
        };
        if !matches!(
            method,
            "turn/started" | "turn/completed" | "thread/status/changed"
        ) {
            return Ok(());
        }
        let params = message
            .get("params")
            .ok_or("activity notification has no params")?;
        let identity = |value: Option<&Value>| -> Result<String, String> {
            let value = value
                .and_then(Value::as_str)
                .ok_or("activity identity must be a string")?;
            if value.is_empty() || value.len() > 512 {
                return Err("activity identity exceeds supported bounds".into());
            }
            Ok(value.to_owned())
        };
        let thread = identity(params.get("threadId"))?;
        if method == "thread/status/changed" {
            match params.pointer("/status/type").and_then(Value::as_str) {
                Some("active") if !self.turns.contains_key(&thread) => self.insert(thread, None)?,
                // Idle is not enough to retire a known running turn. A matching
                // completion is required; background resources are untracked.
                Some("idle" | "notLoaded") if self.turns.get(&thread) == Some(&None) => {
                    self.turns.remove(&thread);
                }
                Some(_) => {}
                None => return Err("thread activity status has no type".into()),
            }
            return Ok(());
        }
        let turn = identity(params.pointer("/turn/id"))?;
        if method == "turn/started" {
            self.started(thread, turn)?;
        } else {
            for starting in self
                .starting
                .values_mut()
                .filter(|starting| starting.thread == thread)
            {
                if !starting.completed.contains(&turn) && starting.completed.len() >= 128 {
                    return Err(
                        "turn/start reply delayed beyond completion identity capacity".into(),
                    );
                }
                starting.completed.insert(turn.clone());
            }
            if self
                .turns
                .get(&thread)
                .is_some_and(|active| active.as_ref().is_none_or(|id| id == &turn))
            {
                self.turns.remove(&thread);
            }
        }
        Ok(())
    }

    fn started(&mut self, thread: String, turn: String) -> Result<(), String> {
        if self
            .turns
            .get(&thread)
            .and_then(Option::as_ref)
            .is_some_and(|existing| existing != &turn)
        {
            return Err("provider started another turn before settling the active turn".into());
        }
        self.insert(thread, Some(turn))
    }

    fn insert(&mut self, thread: String, turn: Option<String>) -> Result<(), String> {
        if self.turns.len() >= 128 && !self.turns.contains_key(&thread) {
            return Err(
                "observed active-turn capacity exceeded; provider activity needs reconciliation"
                    .into(),
            );
        }
        self.turns.insert(thread, turn);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejected_terminal_and_invalid_start_replies_never_invent_activity() {
        for reply in [
            json!({"error":{"code":-32600,"message":"rejected"}}),
            json!({"result":{"turn":{"id":"target","status":"completed"}}}),
            json!({"result":{"turn":{"id":"target","status":"failed"}}}),
            json!({"result":{"turn":{"id":"target","status":"interrupted"}}}),
        ] {
            let mut tracker = Tracker::default();
            let id = Uuid::new_v4();
            tracker.admitted(id, "turn/start", &json!({"threadId":"s"}));
            tracker.replied(id, &reply).unwrap();
            assert!(tracker.snapshot().is_empty());
            assert!(tracker.starting.is_empty());
        }
        for (thread, result) in [
            ("s", json!({})),
            ("s", json!({"turn":{"id":"","status":"inProgress"}})),
            ("s", json!({"turn":{"id":"target","status":"future"}})),
            ("", json!({"turn":{"id":"target","status":"inProgress"}})),
        ] {
            let mut tracker = Tracker::default();
            let id = Uuid::new_v4();
            tracker.admitted(id, "turn/start", &json!({"threadId":thread}));
            assert!(tracker.replied(id, &json!({"result":result})).is_err());
            assert!(tracker.snapshot().is_empty());
        }
    }

    #[test]
    fn delayed_start_reply_tracks_only_matching_completions_with_an_explicit_bound() {
        let mut tracker = Tracker::default();
        let id = Uuid::new_v4();
        tracker.admitted(id, "turn/start", &json!({"threadId":"s"}));
        tracker.record(&json!({"method":"turn/completed","params":{"threadId":"other","turn":{"id":"target"}}})).unwrap();
        tracker
            .replied(
                id,
                &json!({"result":{"turn":{"id":"target","status":"inProgress"}}}),
            )
            .unwrap();
        assert_eq!(tracker.snapshot()[0].turn_id.as_deref(), Some("target"));
        let id = Uuid::new_v4();
        tracker.admitted(id, "turn/start", &json!({"threadId":"s"}));
        for index in 0..128 {
            tracker.record(&json!({"method":"turn/completed","params":{"threadId":"s","turn":{"id":index.to_string()}}})).unwrap();
        }
        assert!(tracker.record(&json!({"method":"turn/completed","params":{"threadId":"s","turn":{"id":"overflow"}}})).is_err());
        tracker
            .replied(
                id,
                &json!({"result":{"turn":{"id":"0","status":"inProgress"}}}),
            )
            .unwrap();
        assert_eq!(tracker.snapshot()[0].turn_id.as_deref(), Some("target"));
        assert!(tracker.starting.is_empty());
    }

    #[test]
    fn admission_cas_and_conservative_allowlist() {
        let ready = Admission::default();
        let drained = ready.change(0, Mode::Draining).unwrap();
        assert_eq!(drained.generation, 1);
        assert_eq!(drained.change(1, Mode::Draining).unwrap(), drained);
        assert!(drained.change(0, Mode::Ready).is_err());
        for method in [
            "thread/start",
            "thread/fork",
            "thread/resume",
            "turn/start",
            "thread/name/set",
            "command/exec",
            "future/read",
        ] {
            assert!(!drained.permits(method));
        }
        assert!(drained.permits("turn/interrupt"));
        assert!(
            drained
                .change(1, Mode::Ready)
                .unwrap()
                .permits("turn/start")
        );
    }

    #[test]
    fn activity_requires_exact_completion_and_retains_known_turn_on_idle() {
        let mut tracker = Tracker::default();
        tracker.record(&json!({"method":"thread/status/changed","params":{"threadId":"a","status":{"type":"active"}}})).unwrap();
        assert_eq!(tracker.snapshot()[0].turn_id, None);
        tracker
            .record(&json!({"method":"turn/started","params":{"threadId":"a","turn":{"id":"one"}}}))
            .unwrap();
        for message in [
            json!({"method":"thread/status/changed","params":{"threadId":"a","status":{"type":"idle"}}}),
            json!({"method":"turn/completed","params":{"threadId":"a","turn":{"id":"other"}}}),
            json!({"method":"turn/completed","params":{"threadId":"b","turn":{"id":"one"}}}),
        ] {
            tracker.record(&message).unwrap();
        }
        assert_eq!(tracker.snapshot()[0].turn_id.as_deref(), Some("one"));
        assert!(
            tracker
                .record(
                    &json!({"method":"turn/started","params":{"threadId":"a","turn":{"id":"two"}}})
                )
                .is_err()
        );
        tracker
            .record(
                &json!({"method":"turn/completed","params":{"threadId":"a","turn":{"id":"one"}}}),
            )
            .unwrap();
        assert!(tracker.snapshot().is_empty());
    }

    #[test]
    fn activity_bounds_and_malformed_known_notifications_are_explicit() {
        let mut tracker = Tracker::default();
        assert!(
            tracker
                .record(&json!({"method":"turn/started","params":{"threadId":"a"}}))
                .is_err()
        );
        for index in 0..128 {
            tracker.record(&json!({"method":"turn/started","params":{"threadId":index.to_string(),"turn":{"id":"one"}}})).unwrap();
        }
        assert!(tracker.record(&json!({"method":"turn/started","params":{"threadId":"overflow","turn":{"id":"one"}}})).is_err());
        assert_eq!(tracker.snapshot().len(), 128);
    }
}
