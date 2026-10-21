//! Toolkit-independent conversation projection and Session Host commands.
//! Codex app-server and DeepSeek ACP map into this shared view with explicit capabilities.

use fleetpane_runtime::Event;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[cfg(unix)]
pub mod client;
#[cfg(unix)]
pub mod terminals;

pub mod acp;
#[cfg(unix)]
pub mod appearance;
#[cfg(unix)]
pub mod attention;
pub mod content;
#[cfg(unix)]
pub mod creations;
pub mod directory;
#[cfg(unix)]
pub mod files;
#[cfg(unix)]
pub mod fleet_directory;
pub mod history;
#[cfg(unix)]
pub mod host_selection;
pub mod hosts;
pub mod models;
#[cfg(unix)]
pub mod paseo_import;
pub mod progress;
pub mod questions;
mod reasoning;
#[cfg(unix)]
pub mod recent_directories;
#[cfg(unix)]
pub mod routing;
#[cfg(unix)]
pub mod session_tree;
#[cfg(unix)]
pub mod wait;
#[cfg(unix)]
pub mod workspace_pins;

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
    pub acp_modes: Option<fleetpane_runtime::acp::modes::State>,
    /// ACP metadata uses provider timestamp strings, never Codex epoch units.
    pub acp_session_info: Option<fleetpane_runtime::acp::SessionInfo>,
    pub status: ThreadStatus,
    pub turns: Vec<Turn>,
    #[serde(default)]
    pub task_plan: Option<fleetpane_runtime::tasks::View>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ThreadStatus {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, rename = "cancelRequested")]
    pub cancel_requested: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Turn {
    pub id: String,
    pub status: String,
    pub items: Vec<Item>,
    pub error: Option<Value>,
    /// ACP prompt settlement; absent for providers that do not expose this field.
    #[serde(rename = "stopReason", skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    MaxTurnRequests,
    Refusal,
    Cancelled,
}

impl StopReason {
    pub fn description(self) -> &'static str {
        match self {
            Self::EndTurn => "End of turn",
            Self::MaxTokens => "Token limit reached",
            Self::MaxTurnRequests => "Turn request limit reached",
            Self::Refusal => "Refused by provider",
            Self::Cancelled => "Cancelled",
        }
    }
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
        if let Some(modes) = &self.acp_modes {
            modes.validate().map_err(Error::Projection)?;
        }
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
                | "thread/acpSessionInfo/updated"
                | "thread/config/updated"
                | "thread/acpModes/updated"
                | "turn/started"
                | "turn/completed"
                | "item/started"
                | "item/completed"
                | "item/agentMessage/delta"
                | "item/reasoning/summaryPartAdded"
                | "item/reasoning/summaryTextDelta"
                | "item/reasoning/textDelta"
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
            method if reasoning::is_delta(method) => reasoning::apply(self, method, params)?,
            "thread/acpSessionInfo/updated" => {
                let name = params
                    .get("threadName")
                    .ok_or_else(|| Error::Projection("ACP metadata lacks title snapshot".into()))?;
                let info = params
                    .get("sessionInfo")
                    .ok_or_else(|| Error::Projection("ACP metadata lacks info snapshot".into()))?;
                let name = serde_json::from_value(name.clone())?;
                let info = serde_json::from_value(info.clone())?;
                self.name = name;
                self.acp_session_info = Some(info);
            }
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
            "thread/acpModes/updated" => {
                let modes: fleetpane_runtime::acp::modes::State = serde_json::from_value(
                    params
                        .get("modes")
                        .cloned()
                        .ok_or_else(|| Error::Projection("ACP mode update lacks state".into()))?,
                )?;
                modes.validate().map_err(Error::Projection)?;
                self.acp_modes = Some(modes);
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
                    existing.stop_reason = turn.stop_reason;
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
    fn cancellation_status_stays_active_until_provider_settlement() {
        let mut thread = thread();
        assert!(!thread.status.cancel_requested);
        thread
            .apply(&event(
                "thread/status/changed",
                json!({"threadId":"thread-1","status":{"type":"active","cancelRequested":true}}),
            ))
            .unwrap();
        assert!(thread.status.cancel_requested);
        assert!(!thread.can_send());
        thread
            .apply(&event(
                "thread/status/changed",
                json!({"threadId":"thread-1","status":{"type":"idle"}}),
            ))
            .unwrap();
        assert!(!thread.status.cancel_requested);
    }

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
    fn mode_updates_are_scoped_validated_metadata_without_timeline_changes() {
        let mut thread = thread();
        let modes = json!({"currentModeId":"code","availableModes":[{"id":"code","name":"Code"}]});
        assert!(
            !thread
                .apply(&event(
                    "thread/acpModes/updated",
                    json!({"threadId":"other","modes":modes})
                ))
                .unwrap()
        );
        assert!(thread.acp_modes.is_none());
        thread
            .apply(&event(
                "thread/acpModes/updated",
                json!({"threadId":"thread-1","modes":modes}),
            ))
            .unwrap();
        assert_eq!(thread.acp_modes.as_ref().unwrap().current_name(), "Code");
        assert!(thread.turns.is_empty());
        let saved = serde_json::to_value(&thread).unwrap();
        for invalid in [json!(null), json!({"currentModeId":"","availableModes":[]})] {
            assert!(
                thread
                    .apply(&event(
                        "thread/acpModes/updated",
                        json!({"threadId":"thread-1","modes":invalid})
                    ))
                    .is_err()
            );
            assert_eq!(serde_json::to_value(&thread).unwrap(), saved);
        }
        let mut invalid = saved;
        invalid["acpModes"]["currentModeId"] = json!("");
        assert!(
            serde_json::from_value::<Thread>(invalid)
                .unwrap()
                .validate()
                .is_err()
        );
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
    fn stop_reason_survives_snapshots_and_completion_updates() {
        for reason in [
            "end_turn",
            "max_tokens",
            "max_turn_requests",
            "refusal",
            "cancelled",
        ] {
            let mut thread = thread();
            thread.apply(&event("turn/started", json!({"threadId":"thread-1","turn":{"id":"turn-1","status":"inProgress","items":[]}}))).unwrap();
            thread.apply(&event("turn/completed", json!({"threadId":"thread-1","turn":{"id":"turn-1","status":"completed","items":[],"stopReason":reason}}))).unwrap();
            let saved = serde_json::to_value(&thread).unwrap();
            assert_eq!(saved["turns"][0]["stopReason"], reason);
            let reloaded: Thread = serde_json::from_value(saved).unwrap();
            assert_eq!(
                serde_json::to_value(reloaded).unwrap()["turns"][0]["stopReason"],
                reason
            );
        }
    }

    #[test]
    fn absent_stop_reason_keeps_other_providers_compatible_but_unknown_is_rejected() {
        let mut thread = thread();
        thread.apply(&event("turn/completed", json!({"threadId":"thread-1","turn":{"id":"turn-1","status":"completed","items":[]}}))).unwrap();
        assert!(thread.turns[0].stop_reason.is_none());
        assert!(
            serde_json::to_value(&thread).unwrap()["turns"][0]
                .get("stopReason")
                .is_none()
        );
        let before = serde_json::to_value(&thread).unwrap();
        for invalid in [json!("future_success"), json!(4)] {
            assert!(thread.apply(&event("turn/completed", json!({"threadId":"thread-1","turn":{"id":"turn-1","status":"completed","items":[],"stopReason":invalid}}))).is_err());
            assert_eq!(serde_json::to_value(&thread).unwrap(), before);
        }
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
    fn acp_info_projection_updates_atomically_without_touching_conversation() {
        let mut thread = thread();
        let turns = serde_json::to_value(&thread.turns).unwrap();
        let mut change = event(
            "thread/acpSessionInfo/updated",
            json!({"threadId":"thread-1","threadName":"中文 title","sessionInfo":{"updatedAt":"2026-10-21T01:00:00Z","_meta":{"opaque":[false,null]}}}),
        );
        assert!(thread.apply(&change).unwrap());
        assert_eq!(thread.title(), "中文 title");
        assert_eq!(
            thread
                .acp_session_info
                .as_ref()
                .unwrap()
                .updated_at
                .as_deref(),
            Some("2026-10-21T01:00:00Z")
        );
        assert_eq!(serde_json::to_value(&thread.turns).unwrap(), turns);
        change.message["params"]["threadId"] = json!("other");
        assert!(!thread.apply(&change).unwrap());
        change.message["params"]["threadId"] = json!("thread-1");
        change.message["params"]["sessionInfo"]["updatedAt"] = json!(42);
        change.message["params"]["threadName"] = json!("bad");
        assert!(thread.apply(&change).is_err());
        assert_eq!(thread.title(), "中文 title");
        change.message["params"]["threadName"] = Value::Null;
        change.message["params"]["sessionInfo"] = json!({"updatedAt":null,"_meta":null});
        thread.apply(&change).unwrap();
        assert!(thread.name.is_none());
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
            stop_reason: None,
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
            stop_reason: None,
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
