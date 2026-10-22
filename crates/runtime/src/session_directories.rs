//! Bounded provider-observed session directories, scoped to one Host incarnation.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;

const MAX_RECORDS: usize = 64;
const MAX_BYTES: usize = 128 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub thread_id: String,
    pub cwd: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Feed {
    pub records: VecDeque<Record>,
    pub evicted: u64,
}

impl Record {
    pub fn from_message(message: &Value) -> Result<Option<Self>, String> {
        // Both Codex and the ACP adapter return the same thread metadata shape.
        // A notification is metadata only when the provider identifies it as such.
        let thread = if message.get("method").is_none() {
            message.pointer("/result/thread")
        } else if message.get("method").and_then(Value::as_str) == Some("thread/started") {
            message.pointer("/params/thread")
        } else {
            None
        };
        let Some(thread) = thread else {
            return Ok(None);
        };
        let id = thread.get("id").and_then(Value::as_str);
        let cwd = thread.get("cwd").and_then(Value::as_str);
        let (Some(id), Some(cwd)) = (id, cwd) else {
            return Err("session metadata requires its exact ID and directory".into());
        };
        if id.is_empty()
            || id.len() > 4096
            || id.contains('\0')
            || !std::path::Path::new(cwd).is_absolute()
            || cwd.len() > 4096
            || cwd.contains('\0')
        {
            return Err("session metadata has invalid ID or absolute directory".into());
        }
        Ok(Some(Self {
            thread_id: id.to_owned(),
            cwd: cwd.to_owned(),
        }))
    }
}

impl Feed {
    pub fn directory(&self, thread_id: &str) -> Option<&str> {
        self.records
            .iter()
            .find(|r| r.thread_id == thread_id)
            .map(|r| r.cwd.as_str())
    }

    pub fn record(&mut self, value: Record) {
        self.records.retain(|r| r.thread_id != value.thread_id);
        self.records.push_back(value);
        while self.records.len() > MAX_RECORDS
            || serde_json::to_vec(&self.records)
                .expect("typed session directories serialize")
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
    fn directories_survive_event_expiry_but_not_host_replacement() {
        let mut host = crate::State::new(42, json!({}));
        let cwd = std::env::temp_dir()
            .join("exact 中文")
            .to_string_lossy()
            .into_owned();
        host.record(json!({"method":"thread/started","params":{"thread":{"id":"s","cwd":cwd}}}))
            .unwrap();
        for _ in 0..150 {
            host.record(json!({"method":"item/agentMessage/delta","params":{"delta":"normal"}}))
                .unwrap();
        }
        assert_eq!(
            host.snapshot().session_directories.unwrap().directory("s"),
            Some(cwd.as_str())
        );
        assert_eq!(
            crate::State::new(42, json!({}))
                .snapshot()
                .session_directories
                .unwrap()
                .directory("s"),
            None
        );
    }

    #[test]
    fn metadata_is_exact_and_cache_eviction_is_bounded_and_visible() {
        let mut feed = Feed::default();
        for index in 0..65 {
            feed.record(
                Record::from_message(
                    &json!({"result":{"thread":{"id":index.to_string(),"cwd":std::env::temp_dir()}}}),
                )
                .unwrap()
                .unwrap(),
            );
        }
        assert_eq!(feed.records.len(), 64);
        assert_eq!(feed.evicted, 1);
        assert_eq!(feed.directory("0"), None);
        feed.record(Record {
            thread_id: "1".into(),
            cwd: "/changed".into(),
        });
        assert_eq!(feed.directory("1"), Some("/changed"));
        assert_eq!(feed.evicted, 1);
        for value in [
            json!({"result":{"thread":{"id":"s","cwd":"relative"}}}),
            json!({"result":{"thread":{"id":"s"}}}),
        ] {
            assert!(Record::from_message(&value).is_err());
        }
        assert!(
            Record::from_message(
                &json!({"method":"unknown","params":{"thread":{"id":"s","cwd":"/guess"}}})
            )
            .unwrap()
            .is_none()
        );
        for index in 0..64 {
            feed.record(Record {
                thread_id: index.to_string(),
                cwd: format!("/{}", "x".repeat(4090)),
            });
        }
        assert!(serde_json::to_vec(&feed.records).unwrap().len() <= MAX_BYTES);
        assert!(feed.records.len() < MAX_RECORDS);
    }

    #[test]
    fn an_older_host_has_no_invented_directory_support() {
        let mut value = serde_json::to_value(crate::State::new(2, json!({})).snapshot()).unwrap();
        value.as_object_mut().unwrap().remove("session_directories");
        let snapshot: crate::Snapshot = serde_json::from_value(value).unwrap();
        assert!(snapshot.session_directories.is_none());
    }
}
