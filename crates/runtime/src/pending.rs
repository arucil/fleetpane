//! Host-local occurrence identity for requests that are still pending.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub id: Value,
    pub sequence: u64,
    /// Host observation time, not a provider timestamp.
    pub observed_unix_ms: u64,
}

impl Observation {
    pub(crate) fn new(message: &Value, sequence: u64) -> Result<Option<Self>, String> {
        if message.get("method").is_none() {
            return Ok(None);
        }
        let Some(id) = message.get("id") else {
            return Ok(None);
        };
        if !id.is_string() && !id.is_number() {
            return Err("provider request ID must be a string or number".into());
        }
        let observed_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis()
            .try_into()
            .map_err(|error: std::num::TryFromIntError| error.to_string())?;
        Ok(Some(Self {
            id: id.clone(),
            sequence,
            observed_unix_ms,
        }))
    }
}

pub fn validate_snapshot(snapshot: &crate::Snapshot) -> Result<(), String> {
    let Some(observations) = &snapshot.pending_observations else {
        return Ok(());
    };
    if observations.len() != snapshot.pending_requests.len() || observations.len() > 64 {
        return Err("Host pending occurrence inventory differs from requests".into());
    }
    let mut ids = Vec::new();
    let mut sequences = std::collections::BTreeSet::new();
    for observation in observations {
        if observation.sequence == 0
            || observation.sequence > snapshot.sequence
            || !sequences.insert(observation.sequence)
            || ids.contains(&&observation.id)
            || !snapshot
                .pending_requests
                .iter()
                .any(|request| request.get("id") == Some(&observation.id))
        {
            return Err("Host has invalid or duplicate pending occurrences".into());
        }
        ids.push(&observation.id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn duplicate_keeps_occurrence_and_answered_id_reuse_gets_a_new_one() {
        let mut state = crate::State::new(42, json!({}));
        let request =
            json!({"id":1,"method":"session/request_permission","params":{"threadId":"same"}});
        state.record(request.clone()).unwrap();
        let first = state.snapshot().pending_observations.unwrap();
        for _ in 0..140 {
            state
                .record(json!({"method":"item/agentMessage/delta","params":{"delta":"text"}}))
                .unwrap();
        }
        state.record(request.clone()).unwrap();
        assert_eq!(state.snapshot().pending_observations.unwrap(), first);
        state.record(json!({"method":"serverRequest/resolved","params":{"requestId":1,"threadId":"same"}})).unwrap();
        assert!(state.snapshot().pending_observations.unwrap().is_empty());
        state.record(request).unwrap();
        let next = state.snapshot().pending_observations.unwrap();
        assert!(next[0].sequence > first[0].sequence);
        assert!(next[0].observed_unix_ms >= first[0].observed_unix_ms);
        state.answered(&json!(1));
        assert!(state.snapshot().pending_observations.unwrap().is_empty());
        let mut legacy = serde_json::to_value(state.snapshot()).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("pending_observations");
        assert!(
            serde_json::from_value::<crate::Snapshot>(legacy)
                .unwrap()
                .pending_observations
                .is_none()
        );
    }
}
