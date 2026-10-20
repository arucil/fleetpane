//! Toolkit-independent conversation projection and Session Host commands.
//! Codex app-server and DeepSeek ACP map into this shared view with explicit capabilities.

use fleetpane_runtime::Event;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[cfg(unix)]
pub mod client;

pub mod acp;
pub mod content;
#[cfg(unix)]
pub mod creations;
pub mod directory;
pub mod history;
pub mod hosts;
pub mod models;
pub mod questions;
#[cfg(unix)]
pub mod routing;

pub const MAX_ITEMS: usize = 500;
pub const MAX_CONVERSATION_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_PROMPT_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
    pub cwd: String,
    pub preview: String,
    pub name: Option<String>,
    pub history_mode: Option<String>,
    #[serde(default)]
    pub history_paging: bool,
    #[serde(default)]
    pub config_options: Vec<acp::ConfigOption>,
    pub status: ThreadStatus,
    pub turns: Vec<Turn>,
    #[serde(default)]
    pub task_plan: Option<fleetpane_runtime::tasks::View>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ThreadStatus {
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Turn {
    pub id: String,
    pub status: String,
    pub items: Vec<Item>,
    pub error: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Item {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(flatten)]
    pub fields: BTreeMap<String, Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid provider payload: {0}")]
    Payload(#[from] serde_json::Error),
    #[error("conversation projection: {0}")]
    Projection(String),
}

impl Thread {
    pub fn supports_history_pages(&self) -> bool {
        self.history_paging || self.history_mode.as_deref() == Some("paginated")
    }

    pub fn title(&self) -> &str {
        self.name
            .as_deref()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| {
                if self.preview.is_empty() {
                    &self.id
                } else {
                    &self.preview
                }
            })
    }

    pub fn active_turn(&self) -> Option<&Turn> {
        self.turns.iter().find(|turn| turn.status == "inProgress")
    }

    pub fn can_send(&self) -> bool {
        self.status.kind == "idle" && self.active_turn().is_none()
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self
            .turns
            .iter()
            .map(|turn| turn.items.len())
            .sum::<usize>()
            > MAX_ITEMS
            || self.turns.len() > MAX_ITEMS
            || serde_json::to_vec(self)?.len() > MAX_CONVERSATION_BYTES
        {
            return Err(Error::Projection("loaded history exceeds the 500-item/turn / 2 MiB limit; reload to return to recent messages".into()));
        }
        Ok(())
    }

    /// Apply known notifications, keep unknown items visible, ignore other threads.
    /// Callers must stop incremental replay and reload after an error.
    pub fn apply(&mut self, event: &Event) -> Result<bool, Error> {
        #[derive(Deserialize)]
        struct Envelope {
            method: String,
            params: Value,
        }
        let Some(method) = event.message.get("method").and_then(Value::as_str) else {
            return Ok(false);
        };
        if !matches!(
            method,
            "turn/plan/updated"
                | "thread/status/changed"
                | "thread/name/updated"
                | "thread/config/updated"
                | "turn/started"
                | "turn/completed"
                | "item/started"
                | "item/completed"
                | "item/agentMessage/delta"
        ) {
            return Ok(false);
        }
        let envelope: Envelope = serde_json::from_value(event.message.clone())?;
        let params = envelope.params;
        let thread_id = params
            .get("threadId")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Projection("known notification has no thread ID".into()))?;
        if thread_id != self.id {
            return Ok(false);
        }
        match envelope.method.as_str() {
            "turn/plan/updated" => {
                if self
                    .task_plan
                    .as_ref()
                    .is_none_or(|view| event.sequence > view.sequence)
                {
                    let plan = fleetpane_runtime::tasks::Plan::from_event(&event.message)
                        .map_err(Error::Projection)?;
                    self.task_plan = Some(fleetpane_runtime::tasks::View {
                        sequence: event.sequence,
                        plan,
                    });
                }
            }
            "thread/config/updated" => {
                self.config_options =
                    serde_json::from_value(params.get("configOptions").cloned().ok_or_else(
                        || Error::Projection("ACP config update lacks options".into()),
                    )?)?;
            }
            "thread/name/updated" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Named {
                    thread_name: Option<String>,
                }
                self.name = serde_json::from_value::<Named>(params)?.thread_name;
            }
            "thread/status/changed" => {
                #[derive(Deserialize)]
                struct Changed {
                    status: ThreadStatus,
                }
                self.status = serde_json::from_value::<Changed>(params)?.status;
            }
            "turn/started" | "turn/completed" => {
                #[derive(Deserialize)]
                struct Changed {
                    turn: Turn,
                }
                let turn = serde_json::from_value::<Changed>(params)?.turn;
                if let Some(existing) = self.turns.iter_mut().find(|old| old.id == turn.id) {
                    // Completion notifications may omit previously streamed items.
                    for item in turn.items {
                        upsert(&mut existing.items, item);
                    }
                    existing.status = turn.status;
                    existing.error = turn.error;
                } else {
                    self.turns.push(turn);
                }
            }
            "item/started" | "item/completed" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Changed {
                    turn_id: String,
                    item: Item,
                }
                let changed: Changed = serde_json::from_value(params)?;
                let turn = self
                    .turns
                    .iter_mut()
                    .find(|turn| turn.id == changed.turn_id)
                    .ok_or_else(|| {
                        Error::Projection(
                            "item arrived before its turn; reload conversation".into(),
                        )
                    })?;
                upsert(&mut turn.items, changed.item);
            }
            "item/agentMessage/delta" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Delta {
                    turn_id: String,
                    item_id: String,
                    delta: String,
                }
                let delta: Delta = serde_json::from_value(params)?;
                let item = self
                    .turns
                    .iter_mut()
                    .find(|turn| turn.id == delta.turn_id)
                    .and_then(|turn| turn.items.iter_mut().find(|item| item.id == delta.item_id))
                    .ok_or_else(|| {
                        Error::Projection("text delta has no item; reload conversation".into())
                    })?;
                match item.fields.get_mut("text") {
                    Some(Value::String(text)) if item.kind == "agentMessage" => {
                        text.push_str(&delta.delta)
                    }
                    _ => {
                        return Err(Error::Projection(
                            "text delta does not target an agent message".into(),
                        ));
                    }
                }
            }
            _ => return Ok(false),
        }
        self.validate()?;
        Ok(true)
    }
}

fn upsert(items: &mut Vec<Item>, item: Item) {
    match items.iter_mut().find(|existing| existing.id == item.id) {
        Some(existing) => *existing = item,
        None => items.push(item),
    }
}

impl Item {
    pub fn text(&self) -> Result<String, Error> {
        match self.kind.as_str() {
            "agentMessage" | "plan" => {
                #[derive(Deserialize)]
                struct Text {
                    text: String,
                }
                Ok(serde_json::from_value::<Text>(serde_json::to_value(self)?)?.text)
            }
            "userMessage" | "acpMessage" => {
                #[derive(Deserialize)]
                struct Message {
                    content: Vec<Value>,
                }
                let message: Message = serde_json::from_value(serde_json::to_value(self)?)?;
                message
                    .content
                    .into_iter()
                    .map(|content| {
                        if content.get("type").and_then(Value::as_str) == Some("text") {
                            #[derive(Deserialize)]
                            struct Text {
                                text: String,
                            }
                            Ok(serde_json::from_value::<Text>(content)?.text)
                        } else {
                            Ok(serde_json::to_string_pretty(&content)?)
                        }
                    })
                    .collect::<Result<Vec<_>, Error>>()
                    .map(|parts| parts.join("\n"))
            }
            _ => Ok(serde_json::to_string_pretty(&self.fields)?),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn task_plan_replacement_is_thread_scoped_and_old_replay_cannot_regress_a_read() {
        let mut thread = thread();
        let mut changed = event(
            "turn/plan/updated",
            json!({"threadId":"thread-1","turnId":"turn-1","explanation":null,"plan":[{"step":"原始步骤","status":"inProgress"}]}),
        );
        changed.sequence = 5;
        assert!(thread.apply(&changed).unwrap());
        assert_eq!(
            thread
                .task_plan
                .as_ref()
                .unwrap()
                .plan
                .as_ref()
                .unwrap()
                .entries[0]
                .status,
            "in_progress"
        );
        changed.sequence = 7;
        changed.message["params"]["plan"] = json!([{"step":"重规划的新步骤","status":"pending"},{"step":"完成","status":"completed"}]);
        thread.apply(&changed).unwrap();
        assert_eq!(
            thread
                .task_plan
                .as_ref()
                .unwrap()
                .plan
                .as_ref()
                .unwrap()
                .entries
                .len(),
            2
        );
        let saved = serde_json::to_value(&thread).unwrap();
        changed.sequence = 6;
        changed.message["params"]["plan"] = json!([]);
        thread.apply(&changed).unwrap();
        assert_eq!(serde_json::to_value(&thread).unwrap(), saved);
        changed.sequence = 8;
        changed.message["params"]["threadId"] = json!("another-thread");
        assert!(!thread.apply(&changed).unwrap());
        assert_eq!(serde_json::to_value(&thread).unwrap(), saved);
        changed.message["params"]["threadId"] = json!("thread-1");
        thread.apply(&changed).unwrap();
        assert!(thread.task_plan.unwrap().plan.unwrap().entries.is_empty());
    }

    fn thread() -> Thread {
        serde_json::from_value(json!({"id":"thread-1","cwd":"/example","preview":"","status":{"type":"idle"},"turns":[]})).unwrap()
    }

    fn event(method: &str, params: Value) -> Event {
        Event {
            sequence: 1,
            message: json!({"method":method,"params":params}),
        }
    }

    #[test]
    fn streamed_items_are_reconciled_by_identity_and_completion_keeps_history() {
        let mut thread = thread();
        thread.apply(&event("turn/started", json!({"threadId":"thread-1","turn":{"id":"turn-1","status":"inProgress","items":[]}}))).unwrap();
        thread.apply(&event("item/started", json!({"threadId":"thread-1","turnId":"turn-1","item":{"id":"message","type":"agentMessage","text":""}}))).unwrap();
        for delta in ["Hello ", "世界"] {
            thread.apply(&event("item/agentMessage/delta", json!({"threadId":"thread-1","turnId":"turn-1","itemId":"message","delta":delta}))).unwrap();
        }
        assert_eq!(thread.turns[0].items[0].text().unwrap(), "Hello 世界");
        thread.apply(&event("item/completed", json!({"threadId":"thread-1","turnId":"turn-1","item":{"id":"message","type":"agentMessage","text":"Hello 世界"}}))).unwrap();
        thread.apply(&event("turn/completed", json!({"threadId":"thread-1","turn":{"id":"turn-1","status":"completed","items":[]}}))).unwrap();
        assert_eq!(thread.turns.len(), 1);
        assert_eq!(thread.turns[0].items.len(), 1);
        assert_eq!(thread.turns[0].items[0].text().unwrap(), "Hello 世界");
        assert!(thread.can_send());
    }

    #[test]
    fn orphan_delta_and_malformed_known_notifications_require_reload() {
        let mut thread = thread();
        assert!(
            thread
                .apply(&event(
                    "item/agentMessage/delta",
                    json!({"threadId":"thread-1","turnId":"missing","itemId":"x","delta":"text"})
                ))
                .is_err()
        );
        assert!(
            thread
                .apply(&event(
                    "turn/completed",
                    json!({"threadId":"thread-1","turn":{"id":"x"}})
                ))
                .is_err()
        );
        assert!(
            thread
                .apply(&event("turn/completed", json!({"threadId":5})))
                .is_err()
        );
        assert!(
            !thread
                .apply(&Event {
                    sequence: 1,
                    message: json!({"method":"future/event"})
                })
                .unwrap()
        );
        assert!(
            !thread
                .apply(&event("turn/started", json!({"threadId":"another-thread"})))
                .unwrap()
        );
    }

    #[test]
    fn future_tool_items_remain_inspectable_and_bad_text_is_not_hidden() {
        let item: Item = serde_json::from_value(
            json!({"id":"tool","type":"futureTool","status":"running","extra":42}),
        )
        .unwrap();
        assert!(item.text().unwrap().contains("42"));
        let bad: Item =
            serde_json::from_value(json!({"id":"message","type":"agentMessage","text":42}))
                .unwrap();
        assert!(bad.text().is_err());
    }

    #[test]
    fn rename_notifications_update_only_the_matching_thread() {
        let mut thread = thread();
        assert!(
            thread
                .apply(&event(
                    "thread/name/updated",
                    json!({"threadId":"thread-1","threadName":"中文新标题"})
                ))
                .unwrap()
        );
        assert_eq!(thread.title(), "中文新标题");
        assert!(
            !thread
                .apply(&event(
                    "thread/name/updated",
                    json!({"threadId":"another","threadName":"wrong"})
                ))
                .unwrap()
        );
        assert_eq!(thread.title(), "中文新标题");
        assert!(
            thread
                .apply(&event(
                    "thread/name/updated",
                    json!({"threadId":"thread-1","threadName":42})
                ))
                .is_err()
        );
    }

    #[test]
    fn active_and_unknown_statuses_do_not_admit_another_turn() {
        let mut thread = thread();
        thread.status.kind = "futureState".into();
        assert!(!thread.can_send());
        thread.status.kind = "idle".into();
        thread.turns.push(Turn {
            id: "turn-1".into(),
            status: "inProgress".into(),
            items: vec![],
            error: None,
        });
        assert!(!thread.can_send());
    }

    #[test]
    fn history_limits_are_explicit() {
        let mut thread = thread();
        let item: Item = serde_json::from_value(
            json!({"id":"large","type":"agentMessage","text":"x".repeat(MAX_CONVERSATION_BYTES)}),
        )
        .unwrap();
        thread.turns.push(Turn {
            id: "turn-1".into(),
            status: "completed".into(),
            items: vec![item],
            error: None,
        });
        assert!(thread.validate().is_err());
    }
}

#[cfg(unix)]
mod storage;
#[cfg(unix)]
pub mod workspaces;
#[cfg(unix)]
pub mod worktrees;

#[cfg(unix)]
pub mod diffs;

#[cfg(unix)]
pub mod drafts;
