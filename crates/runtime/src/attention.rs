//! A bounded, Host-incarnation-local record of explicit terminal turn events.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;

pub const CAPACITY: usize = 64;
const MAX_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Completed,
    Failed,
    Interrupted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub sequence: u64,
    /// Host observation time, not the provider's event timestamp.
    pub observed_unix_ms: u64,
    pub thread_id: String,
    pub turn_id: String,
    pub kind: Kind,
    pub stop_reason: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Feed {
    pub records: VecDeque<Record>,
    pub evicted: u64,
}

impl Record {
    pub fn from_event(message: &Value, sequence: u64) -> Result<Option<Self>, String> {
        if message.get("method").and_then(Value::as_str) != Some("turn/completed") {
            return Ok(None);
        }
        let identity = |pointer| {
            message
                .pointer(pointer)
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty() && text.len() <= 512)
                .map(str::to_owned)
                .ok_or_else(|| format!("completion has invalid {pointer}"))
        };
        let kind = match message
            .pointer("/params/turn/status")
            .and_then(Value::as_str)
        {
            Some("completed") => Kind::Completed,
            Some("failed") => Kind::Failed,
            Some("interrupted") => Kind::Interrupted,
            _ => return Err("completion has unsupported terminal status".into()),
        };
        let stop_reason = match message.pointer("/params/turn/stopReason") {
            None | Some(Value::Null) => None,
            Some(Value::String(reason)) if !reason.is_empty() && reason.len() <= 128 => {
                Some(reason.clone())
            }
            _ => return Err("completion has invalid stop reason".into()),
        };
        let observed_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis()
            .try_into()
            .map_err(|error: std::num::TryFromIntError| error.to_string())?;
        Ok(Some(Self {
            sequence,
            observed_unix_ms,
            thread_id: identity("/params/threadId")?,
            turn_id: identity("/params/turn/id")?,
            kind,
            stop_reason,
        }))
    }
}

impl Feed {
    pub fn check(&self, record: &Record) -> Result<(), String> {
        if let Some(old) = self
            .records
            .iter()
            .find(|old| old.thread_id == record.thread_id && old.turn_id == record.turn_id)
            && (old.kind != record.kind || old.stop_reason != record.stop_reason)
        {
            return Err("provider changed a retained terminal turn result".into());
        }
        Ok(())
    }

    pub fn record(&mut self, record: Record) {
        if self
            .records
            .iter()
            .any(|old| old.thread_id == record.thread_id && old.turn_id == record.turn_id)
        {
            return;
        }
        self.records.push_back(record);
        while self.records.len() > CAPACITY
            || serde_json::to_vec(&self.records)
                .expect("typed completion records serialize")
                .len()
                > MAX_BYTES
        {
            self.records.pop_front();
            self.evicted += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn host_snapshot_retains_results_after_event_expiry_and_refuses_changed_outcomes() {
        let mut state = crate::State::new(2, json!({"synthetic":true}));
        let completed = json!({"method":"turn/completed","params":{"threadId":"thread","turn":{"id":"turn","status":"failed","items":[],"error":{"message":"original failure"}}}});
        state.record(completed.clone()).unwrap();
        let first = state.snapshot();
        for _ in 0..150 {
            state.record(json!({"method":"item/agentMessage/delta","params":{"delta":"ordinary output"}})).unwrap();
        }
        state.record(completed.clone()).unwrap();
        assert_eq!(
            state.snapshot().attention.as_ref().unwrap().records,
            first.attention.as_ref().unwrap().records
        );
        assert!(matches!(
            state.events(first.host_id, 0),
            crate::Response::Error { .. }
        ));
        let before = state.snapshot().sequence;
        let mut changed = completed;
        changed["params"]["turn"]["status"] = json!("completed");
        assert!(state.record(changed).is_err());
        assert_eq!(state.snapshot().sequence, before);
        let mut legacy = serde_json::to_value(first).unwrap();
        legacy.as_object_mut().unwrap().remove("attention");
        assert!(
            serde_json::from_value::<crate::Snapshot>(legacy)
                .unwrap()
                .attention
                .is_none()
        );
        assert!(
            crate::State::new(3, json!({}))
                .snapshot()
                .attention
                .unwrap()
                .records
                .is_empty()
        );
    }

    #[test]
    fn only_explicit_terminal_turns_produce_bounded_deduplicated_records() {
        let mut feed = Feed::default();
        for value in [
            json!({"method":"item/agentMessage/delta"}),
            json!({"method":"thread/status/changed","params":{"status":{"type":"idle"}}}),
        ] {
            assert!(Record::from_event(&value, 1).unwrap().is_none());
        }
        for index in 0..70 {
            let event = json!({"method":"turn/completed","params":{"threadId":"same/中文","turn":{"id":format!("turn-{index}"),"status":"failed"}}});
            let record = Record::from_event(&event, index + 1).unwrap().unwrap();
            feed.record(record.clone());
            feed.record(record);
        }
        assert_eq!(feed.evicted, 6);
        assert_eq!(feed.records.len(), CAPACITY);
        assert_eq!(feed.records.front().unwrap().turn_id, "turn-6");
        assert!(
            feed.records
                .iter()
                .all(|record| record.kind == Kind::Failed)
        );
        let bad = json!({"method":"turn/completed","params":{"threadId":"t","turn":{"id":"u","status":"inProgress"}}});
        assert!(Record::from_event(&bad, 71).is_err());
        let mut changed = feed.records.back().unwrap().clone();
        changed.kind = Kind::Completed;
        assert!(feed.check(&changed).is_err());
    }

    #[test]
    fn escaped_identifiers_obey_the_encoded_byte_budget() {
        let mut feed = Feed::default();
        for sequence in 1..=64 {
            let event = json!({"method":"turn/completed","params":{"threadId":"\u{0001}".repeat(512),"turn":{"id":format!("{}-{sequence}","\u{0001}".repeat(500)),"status":"interrupted"}}});
            feed.record(Record::from_event(&event, sequence).unwrap().unwrap());
        }
        assert!(serde_json::to_vec(&feed.records).unwrap().len() <= MAX_BYTES);
        assert!(feed.evicted > 0);
    }
}
