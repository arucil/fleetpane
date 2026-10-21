//! Stop acknowledgement and exact-turn completion are independent boundaries.
use crate::{HostError, lifecycle::ActiveTurn};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;

#[cfg(test)]
mod tests;

fn error_excerpt(reason: &str) -> String {
    if reason.len() <= 512 {
        return reason.to_owned();
    }
    let mut end = 512;
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}… (excerpt; inspect the operation receipt for the full error)",
        &reason[..end]
    )
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Dispatching,
    AwaitingCompletion,
    Rejected,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Stop {
    pub thread_id: String,
    pub turn_id: String,
    pub operation_id: Uuid,
    pub phase: Phase,
    pub terminal_status: Option<String>,
    pub error: Option<String>,
}

impl Stop {
    pub fn can_retry(&self) -> bool {
        self.phase == Phase::Rejected && self.terminal_status.is_none()
    }

    pub fn description(&self) -> &'static str {
        match (self.phase, self.terminal_status.is_some()) {
            (Phase::Dispatching, false) => {
                "Stop submitted. Waiting for acknowledgement and the original turn to finish."
            }
            (Phase::Dispatching, true) => {
                "The original turn finished. Waiting for the Stop reply before enabling new input."
            }
            (Phase::AwaitingCompletion, _) => {
                "Stop acknowledged. Waiting for the original turn to finish."
            }
            (Phase::Rejected, _) => {
                "Stop was rejected. Review the error and retry explicitly, or wait for the original turn to finish."
            }
            (Phase::Unknown, _) => {
                "Stop outcome is unknown. Inspect the Host and original turn before continuing; do not resend automatically."
            }
        }
    }
}

#[derive(Default)]
pub struct Tracker {
    pending: BTreeMap<String, Stop>,
}

impl Tracker {
    pub fn snapshot(&self) -> Vec<Stop> {
        self.pending.values().cloned().collect()
    }

    /// Validate without changing state; commit only after operation admission.
    pub fn prepare(
        &self,
        operation_id: Uuid,
        method: &str,
        params: &Value,
        active: &[ActiveTurn],
    ) -> Result<Option<Stop>, HostError> {
        let thread = params.get("threadId").and_then(Value::as_str);
        if let Some(stop) = thread.and_then(|id| self.pending.get(id))
            && !(method == "turn/interrupt" && stop.can_retry())
        {
            return Err(HostError::new("stop_pending", stop.description()));
        }
        if method != "turn/interrupt" {
            return Ok(None);
        }
        let identity = |key| {
            params
                .get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty() && value.len() <= 512)
                .map(str::to_owned)
                .ok_or_else(|| {
                    HostError::new(
                        "invalid_stop",
                        "Stop requires threadId and turnId strings of 1 to 512 bytes",
                    )
                })
        };
        let thread_id = identity("threadId")?;
        let turn_id = identity("turnId")?;
        if !active.iter().any(|turn| {
            turn.thread_id == thread_id && turn.turn_id.as_deref() == Some(turn_id.as_str())
        }) {
            return Err(HostError::new(
                "stop_target_changed",
                "Stop must target the exact observed active turn; refresh before continuing",
            ));
        }
        if self.pending.len() >= 128 && !self.pending.contains_key(&thread_id) {
            return Err(HostError::new(
                "capacity",
                "Stop tracking capacity exhausted; command was not submitted",
            ));
        }
        let stop = Stop {
            thread_id,
            turn_id,
            operation_id,
            phase: Phase::Dispatching,
            terminal_status: None,
            error: None,
        };
        // Reserve error/status expansion for every live Stop within the existing
        // control-frame budget. Full errors remain in operation receipts.
        let required = self
            .pending
            .values()
            .filter(|pending| pending.thread_id != stop.thread_id)
            .chain(std::iter::once(&stop))
            .map(|pending| {
                serde_json::to_vec(pending)
                    .expect("Stop JSON serialization")
                    .len()
                    + 1024
            })
            .sum::<usize>();
        if required > 64 * 1024 {
            return Err(HostError::new(
                "capacity",
                "Stop progress byte budget exhausted; command was not submitted",
            ));
        }
        Ok(Some(stop))
    }

    pub fn commit(&mut self, stop: Stop) {
        self.pending.insert(stop.thread_id.clone(), stop);
    }

    /// Called only after the Host has validated the provider envelope.
    pub fn record(&mut self, message: &Value) -> Result<(), String> {
        if message.get("method").and_then(Value::as_str) == Some("turn/completed") {
            let params = &message["params"];
            if let Some(stop) = params["threadId"]
                .as_str()
                .and_then(|id| self.pending.get_mut(id))
                && params.pointer("/turn/id").and_then(Value::as_str) == Some(stop.turn_id.as_str())
            {
                let status = params
                    .pointer("/turn/status")
                    .and_then(Value::as_str)
                    .filter(|status| matches!(*status, "completed" | "interrupted" | "failed"))
                    .ok_or("Stop target completion lacks a supported terminal status")?;
                stop.terminal_status = Some(status.to_owned());
            }
        } else if message.get("method").is_none()
            && let Some(stop) = message["id"].as_str().and_then(|id| {
                self.pending
                    .values_mut()
                    .find(|stop| stop.operation_id.to_string() == id)
            })
        {
            if let Some(error) = message.get("error") {
                if stop.phase != Phase::Unknown {
                    stop.phase = Phase::Rejected;
                    stop.error = Some(error_excerpt(&error.to_string()));
                }
            } else if stop.phase != Phase::Unknown {
                stop.phase = Phase::AwaitingCompletion;
            }
        }
        self.pending.retain(|_, stop| {
            stop.terminal_status.is_none()
                || !matches!(stop.phase, Phase::AwaitingCompletion | Phase::Rejected)
        });
        Ok(())
    }

    pub fn fail(&mut self, reason: &str) {
        for stop in self.pending.values_mut() {
            stop.phase = Phase::Unknown;
            stop.error = Some(error_excerpt(reason));
        }
    }
}
