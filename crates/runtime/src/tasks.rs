//! Provider-reported task lists, independent of plan-mode proposal text.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::VecDeque;

pub const READ_METHOD: &str = "fleetpane/tasks/read";
pub const MAX_ENTRIES: usize = 128;
pub const MAX_PLAN_BYTES: usize = 32 * 1024;
const MAX_THREADS: usize = 64;
const MAX_CACHE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub thread_id: String,
    pub turn_id: String,
    pub explanation: Option<String>,
    pub entries: Vec<Entry>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Entry {
    pub content: String,
    pub status: String,
    pub priority: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct View {
    pub sequence: u64,
    pub plan: Option<Plan>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Read {
    pub thread_id: String,
}

impl Plan {
    pub fn validate(&self) -> Result<(), String> {
        if self.thread_id.is_empty()
            || self.turn_id.is_empty()
            || self.thread_id.len() > 4096
            || self.turn_id.len() > 4096
            || self.entries.len() > MAX_ENTRIES
            || self.entries.iter().any(|e| {
                e.content.len() > 4096
                    || e.status.is_empty()
                    || e.status.len() > 128
                    || e.priority
                        .as_ref()
                        .is_some_and(|p| p.is_empty() || p.len() > 128)
            })
            || serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > MAX_PLAN_BYTES
        {
            return Err("task plan exceeds identity, 128-entry, 4 KiB step or 32 KiB plan limits, or lacks a status/priority".into());
        }
        Ok(())
    }

    pub fn from_event(message: &Value) -> Result<Option<Self>, String> {
        if message.get("method").and_then(Value::as_str) != Some("turn/plan/updated") {
            return Ok(None);
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Updated {
            thread_id: String,
            turn_id: String,
            explanation: Option<String>,
            plan: Vec<Step>,
        }
        #[derive(Deserialize)]
        struct Step {
            step: String,
            status: String,
            priority: Option<String>,
        }
        let update: Updated = serde_json::from_value(
            message
                .get("params")
                .cloned()
                .ok_or("task notification lacks params")?,
        )
        .map_err(|e| e.to_string())?;
        let plan = Self {
            thread_id: update.thread_id,
            turn_id: update.turn_id,
            explanation: update.explanation,
            entries: update
                .plan
                .into_iter()
                .map(|s| Entry {
                    content: s.step,
                    status: if s.status == "inProgress" {
                        "in_progress".into()
                    } else {
                        s.status
                    },
                    priority: s.priority,
                })
                .collect(),
        };
        plan.validate()?;
        Ok(Some(plan))
    }

    pub fn from_acp(thread_id: &str, turn_id: &str, update: &Value) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct AcpEntry {
            content: String,
            status: String,
            priority: String,
        }
        let rows: Vec<AcpEntry> = serde_json::from_value(
            update
                .get("entries")
                .cloned()
                .ok_or("ACP plan lacks entries")?,
        )
        .map_err(|e| e.to_string())?;
        let plan = Self {
            thread_id: thread_id.into(),
            turn_id: turn_id.into(),
            explanation: None,
            entries: rows
                .into_iter()
                .map(|e| Entry {
                    content: e.content,
                    status: e.status,
                    priority: Some(e.priority),
                })
                .collect(),
        };
        plan.validate()?;
        Ok(plan)
    }

    pub fn notification(&self) -> Value {
        let entries: Vec<_> = self
            .entries
            .iter()
            .map(|e| json!({"step":e.content,"status":e.status,"priority":e.priority}))
            .collect();
        json!({"method":"turn/plan/updated","params":{"threadId":self.thread_id,"turnId":self.turn_id,"explanation":self.explanation,"plan":entries}})
    }

    /// DSH's ACP bridge reports todo_write as a generic successful tool call.
    /// Its committed input has no priorities, unlike standard ACP plan entries.
    pub fn from_deepseek_todos(
        thread_id: &str,
        turn_id: &str,
        input: &Value,
    ) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct Todo {
            content: String,
            status: String,
        }
        #[derive(Deserialize)]
        struct Input {
            todos: Vec<Todo>,
        }
        let input: Input = serde_json::from_value(input.clone()).map_err(|e| e.to_string())?;
        let plan = Self {
            thread_id: thread_id.into(),
            turn_id: turn_id.into(),
            explanation: None,
            entries: input
                .todos
                .into_iter()
                .map(|todo| Entry {
                    content: todo.content,
                    status: todo.status,
                    priority: None,
                })
                .collect(),
        };
        plan.validate()?;
        Ok(plan)
    }

    pub fn text(&self) -> String {
        let mut lines = Vec::new();
        if let Some(explanation) = &self.explanation {
            lines.push(explanation.clone());
        }
        for entry in &self.entries {
            let priority = entry
                .priority
                .as_ref()
                .map(|p| format!(" ({p})"))
                .unwrap_or_default();
            lines.push(format!("[{}]{} {}", entry.status, priority, entry.content));
        }
        lines.join("\n")
    }
}

#[derive(Default)]
pub(crate) struct Cache {
    plans: VecDeque<(Plan, usize)>,
    bytes: usize,
}
impl Cache {
    pub fn record(&mut self, plan: Plan) {
        if let Some(index) = self
            .plans
            .iter()
            .position(|(old, _)| old.thread_id == plan.thread_id)
        {
            self.bytes -= self.plans.remove(index).expect("existing index").1;
        }
        let bytes = serde_json::to_vec(&plan)
            .expect("serializable task plan")
            .len();
        self.bytes += bytes;
        self.plans.push_back((plan, bytes));
        while self.plans.len() > MAX_THREADS || self.bytes > MAX_CACHE_BYTES {
            self.bytes -= self
                .plans
                .pop_front()
                .expect("nonempty bounded task cache")
                .1;
        }
    }
    #[cfg(any(unix, test))]
    pub fn read(&self, thread: &str, sequence: u64) -> View {
        View {
            sequence,
            plan: self
                .plans
                .iter()
                .find(|(p, _)| p.thread_id == thread)
                .map(|(p, _)| p.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Lease, Response, State};
    use uuid::Uuid;

    fn event(thread: &str, entries: Value) -> Value {
        json!({"method":"turn/plan/updated","params":{"threadId":thread,"turnId":"turn","explanation":"执行计划\n保留说明", "plan":entries}})
    }

    #[test]
    fn provider_formats_replace_clear_and_preserve_future_statuses() {
        let plan=Plan::from_event(&event("a",json!([{"step":"检查中文","status":"inProgress"},{"step":"Future","status":"awaiting_review"}]))).unwrap().unwrap();
        assert_eq!(plan.entries[0].status, "in_progress");
        assert_eq!(plan.entries[1].status, "awaiting_review");
        assert!(plan.text().contains("执行计划\n保留说明"));
        let acp = Plan::from_acp(
            "a",
            "turn",
            &json!({"entries":[{"content":"执行","priority":"high","status":"in_progress"}]}),
        )
        .unwrap();
        assert_eq!(Plan::from_event(&acp.notification()).unwrap(), Some(acp));
        assert!(
            Plan::from_acp(
                "a",
                "turn",
                &json!({"entries":[{"content":"缺少优先级","status":"pending"}]})
            )
            .is_err()
        );
        assert!(Plan::from_event(&event("a", json!([{"step":"No status"}]))).is_err());
        let mut cache = Cache::default();
        cache.record(plan);
        let cleared = Plan::from_event(&event("a", json!([]))).unwrap().unwrap();
        cache.record(cleared.clone());
        assert_eq!(cache.plans.len(), 1);
        assert_eq!(cache.read("a", 7).plan, Some(cleared));
        assert!(cache.read("other", 7).plan.is_none());
    }

    #[test]
    fn bounded_cache_survives_event_expiry_and_does_not_infer_completion() {
        let mut state = State::new(1, json!({}));
        let plan = event("a", json!([{"step":"not completed","status":"inProgress"}]));
        state.record(plan).unwrap();
        for index in 0..140 {
            state
                .record(json!({"method":"future/activity","params":{"index":index}}))
                .unwrap();
        }
        state.record(json!({"method":"turn/completed","params":{"threadId":"a","turn":{"id":"turn","status":"interrupted","items":[],"error":null}}})).unwrap();
        assert!(matches!(
            state.events(state.snapshot.host_id, 0),
            Response::Error { .. }
        ));
        assert_eq!(
            state
                .tasks
                .read("a", state.snapshot.sequence)
                .plan
                .unwrap()
                .entries[0]
                .status,
            "in_progress"
        );
        let sequence = state.snapshot.sequence;
        assert!(
            state
                .record(event("a", json!([{"step":"x","status":false}])))
                .is_err()
        );
        assert_eq!(state.snapshot.sequence, sequence);
        // Local task reads retain controller fencing while tolerating a failed provider.
        let Response::Lease(lease) = state.claim(
            crate::PROTOCOL_VERSION,
            state.snapshot.host_id,
            0,
            Uuid::new_v4(),
        ) else {
            panic!("claim")
        };
        let stale = Lease {
            generation: lease.generation + 1,
            ..lease.clone()
        };
        assert!(state.authorize_controller(&stale).is_err());
        state.fail("synthetic provider fault".into());
        assert!(state.authorize_controller(&lease).is_ok());
        let mut cache = Cache::default();
        for index in 0..70 {
            cache.record(
                Plan::from_event(&event(&index.to_string(), json!([])))
                    .unwrap()
                    .unwrap(),
            );
        }
        assert_eq!(cache.plans.len(), 64);
        assert!(cache.read("0", 0).plan.is_none());
        for index in 0..70 {
            let rows: Vec<_> = (0..7)
                .map(|_| json!({"step":"x".repeat(4000),"status":"pending"}))
                .collect();
            cache.record(
                Plan::from_event(&event(&index.to_string(), json!(rows)))
                    .unwrap()
                    .unwrap(),
            );
        }
        assert!(cache.bytes <= MAX_CACHE_BYTES && cache.plans.len() < 64);
        let rows: Vec<_> = (0..129)
            .map(|_| json!({"step":"x","status":"pending"}))
            .collect();
        assert!(Plan::from_event(&event("a", json!(rows))).is_err());
        assert!(
            Plan::from_event(&event(
                "a",
                json!([{"step":"x".repeat(4097),"status":"pending"}])
            ))
            .is_err()
        );
    }
}
