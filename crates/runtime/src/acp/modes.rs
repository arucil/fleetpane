//! ACP v1's optional dedicated mode API; v2/config-option negotiation is separate.
use super::{Adapter, Batch, Pending, notification, reply, string};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub current_mode_id: String,
    pub available_modes: Vec<Mode>,
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Map<String, Value>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mode {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Map<String, Value>>,
}

impl State {
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = std::collections::BTreeSet::new();
        if self.current_mode_id.is_empty()
            || self
                .available_modes
                .iter()
                .any(|mode| mode.id.is_empty() || mode.name.is_empty() || !ids.insert(&mode.id))
        {
            return Err("ACP modes require a current ID and unique nonempty mode IDs/names".into());
        }
        Ok(())
    }

    pub fn current_name(&self) -> &str {
        self.available_modes
            .iter()
            .find(|mode| mode.id == self.current_mode_id)
            .map_or(&self.current_mode_id, |mode| &mode.name)
    }
}

pub(super) fn parse(result: &Value) -> Result<Option<State>, String> {
    let state: Option<State> = serde_json::from_value(result["modes"].clone())
        .map_err(|error| format!("invalid ACP modes: {error}"))?;
    if let Some(state) = &state {
        state.validate()?;
    }
    Ok(state)
}

pub(super) fn updated(id: &str, state: &State) -> Value {
    notification(
        "thread/acpModes/updated",
        json!({"threadId":id,"modes":state}),
    )
}

impl Adapter {
    pub(super) fn set_mode(
        &mut self,
        host: &str,
        params: &Value,
        batch: &mut Batch,
    ) -> Result<(), String> {
        let id = string(params, "threadId")?;
        self.configuration_settled(id)?;
        let requested = string(params, "modeId")?;
        let session = self.sessions.get(id).ok_or("ACP session is not resident")?;
        if !session.resident {
            return Err("Resume the ACP session before mode changes".into());
        }
        let modes = session
            .modes
            .as_ref()
            .ok_or("ACP session advertises no modes")?;
        if !modes
            .available_modes
            .iter()
            .any(|mode| mode.id == requested)
        {
            return Err("ACP mode is no longer available".into());
        }
        // ACP v1 explicitly permits changing a mode while a prompt is active.
        self.rpc(
            "session/set_mode",
            json!({"sessionId":id,"modeId":requested}),
            Pending::Mode {
                host: host.into(),
                id: id.into(),
                requested: requested.into(),
                revision: session.mode_update_version,
            },
            batch,
        );
        Ok(())
    }

    pub(super) fn mode_response(
        &mut self,
        host: &str,
        id: &str,
        requested: String,
        revision: u64,
        batch: &mut Batch,
    ) -> Result<(), String> {
        let session = self
            .sessions
            .get_mut(id)
            .ok_or("ACP mode session disappeared")?;
        let modes = session
            .modes
            .as_mut()
            .ok_or("ACP mode catalog disappeared")?;
        // An empty successful response acknowledges the requested choice. A newer
        // provider notification is the more recent state and must not be undone.
        if session.mode_update_version == revision {
            modes.current_mode_id = requested;
        }
        #[cfg(unix)]
        self.dirty.insert(id.into());
        batch.events.push(updated(id, modes));
        batch.events.push(reply(host, json!({"modes":modes})));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Value {
        json!({"currentModeId":"ask","availableModes":[{"id":"ask","name":"Ask"},{"id":"code","name":"Code"}],"_meta":{"vendor":[0,false]}})
    }
    fn create(adapter: &mut Adapter, id: &str, modes: Value) {
        let new = adapter
            .dispatch(
                "new".into(),
                "thread/start",
                json!({"cwd":std::env::temp_dir()}),
            )
            .unwrap();
        ack(
            adapter,
            &new,
            json!({"sessionId":id,"modes":modes,"configOptions":[{"id":"model","options":[{"value":"a"},{"value":"b"}]}]}),
        );
    }
    fn ack(adapter: &mut Adapter, request: &Batch, result: Value) -> Batch {
        adapter
            .receive(json!({"jsonrpc":"2.0","id":request.wire[0]["id"],"result":result}))
            .unwrap()
    }
    fn change(adapter: &mut Adapter, id: &str, mode: &str) -> Batch {
        adapter
            .dispatch(
                "mode".into(),
                "acp/mode/set",
                json!({"threadId":id,"modeId":mode}),
            )
            .unwrap()
    }
    fn update(adapter: &mut Adapter, mode: Value) -> Result<Batch, String> {
        adapter.receive(json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"current_mode_update","currentModeId":mode}}}))
    }
    fn rejected(adapter: &mut Adapter, method: &str, params: Value) {
        let batch = adapter.dispatch("rejected".into(), method, params).unwrap();
        assert!(batch.wire.is_empty());
        assert!(batch.events[0].get("error").is_some());
    }

    #[test]
    fn idle_updates_and_late_ack_keep_newer_provider_state() {
        let mut adapter = Adapter::default();
        create(&mut adapter, "s", catalog());
        let request = change(&mut adapter, "s", "code");
        let update = update(&mut adapter, json!("vendor-mode")).unwrap();
        assert_eq!(
            update.events[0]["params"]["modes"]["currentModeId"],
            "vendor-mode"
        );
        let response = ack(&mut adapter, &request, json!({}));
        assert_eq!(
            response.events[1]["result"]["modes"]["currentModeId"],
            "vendor-mode"
        );
        let session = &adapter.sessions["s"];
        assert!(session.turns.is_empty());
        assert_eq!(
            session.modes.as_ref().unwrap().current_name(),
            "vendor-mode"
        );
        assert_eq!(session.modes.as_ref().unwrap().available_modes.len(), 2);
        let request = change(&mut adapter, "s", "code");
        let response = ack(&mut adapter, &request, json!({}));
        assert_eq!(
            response.events[1]["result"]["modes"]["currentModeId"],
            "code"
        );
    }

    #[test]
    fn active_modes_preserve_permissions_and_stop_and_are_serial_per_session() {
        let mut adapter = Adapter::default();
        create(&mut adapter, "s", catalog());
        create(&mut adapter, "other", catalog());
        let prompt = adapter
            .dispatch(
                "send".into(),
                "turn/start",
                json!({"threadId":"s","input":[{"type":"text","text":"hello"}]}),
            )
            .unwrap();
        adapter.receive(json!({"jsonrpc":"2.0","id":71,"method":"session/request_permission","params":{"sessionId":"s","toolCall":{"toolCallId":"tool","title":"Review"},"options":[{"optionId":"yes","name":"Allow","kind":"allow_once"}]}})).unwrap();
        let before = adapter.sessions["s"].thread(true);
        let request = change(&mut adapter, "s", "code");
        assert!(!adapter.permissions.is_empty());
        assert_eq!(adapter.sessions["s"].thread(true), before);
        for (method, params) in [
            ("acp/mode/set", json!({"threadId":"s","modeId":"ask"})),
            (
                "acp/config/set",
                json!({"threadId":"s","configId":"model","value":"b"}),
            ),
            (
                "turn/start",
                json!({"threadId":"s","input":[{"type":"text","text":"blocked"}]}),
            ),
        ] {
            rejected(&mut adapter, method, params);
        }
        let other = change(&mut adapter, "other", "code");
        ack(&mut adapter, &other, json!({}));
        let interrupted = adapter
            .dispatch(
                "stop".into(),
                "turn/interrupt",
                json!({"threadId":"s","turnId":before["turns"][0]["id"]}),
            )
            .unwrap();
        assert!(
            interrupted
                .wire
                .iter()
                .any(|w| w["method"] == "session/cancel")
        );
        ack(&mut adapter, &request, json!({}));
        assert_eq!(adapter.sessions["s"].thread(true)["turns"], before["turns"]);
        ack(&mut adapter, &prompt, json!({"stopReason":"cancelled"}));
        assert!(adapter.sessions["s"].active.is_none());
    }

    #[test]
    fn failed_modes_do_not_change_state_and_invalid_requests_never_dispatch() {
        let mut adapter = Adapter::default();
        create(&mut adapter, "s", catalog());
        let request = change(&mut adapter, "s", "code");
        let error = adapter.receive(json!({"jsonrpc":"2.0","id":request.wire[0]["id"],"error":{"code":-32000,"message":"Denied"}})).unwrap();
        assert_eq!(error.events[0]["error"]["message"], "Denied");
        assert_eq!(
            adapter.sessions["s"]
                .modes
                .as_ref()
                .unwrap()
                .current_mode_id,
            "ask"
        );
        let pending = adapter.pending.len();
        for id in ["missing", "s"] {
            rejected(
                &mut adapter,
                "acp/mode/set",
                json!({"threadId":id,"modeId":"unknown"}),
            );
            assert_eq!(adapter.pending.len(), pending);
        }
        adapter.sessions.get_mut("s").unwrap().resident = false;
        rejected(
            &mut adapter,
            "acp/mode/set",
            json!({"threadId":"s","modeId":"code"}),
        );
        adapter.sessions.get_mut("s").unwrap().resident = true;
        adapter.sessions.get_mut("s").unwrap().modes = None;
        rejected(
            &mut adapter,
            "acp/mode/set",
            json!({"threadId":"s","modeId":"code"}),
        );
        update(&mut adapter, json!("provider-only")).unwrap();
        let state = adapter.sessions["s"].modes.as_ref().unwrap();
        assert_eq!(state.current_name(), "provider-only");
        assert!(state.available_modes.is_empty());
    }

    #[test]
    fn malformed_catalogs_updates_and_responses_are_visible_errors() {
        for modes in [
            json!({}),
            json!({"currentModeId":"","availableModes":[]}),
            json!({"currentModeId":"ask","availableModes":[{"id":"ask","name":"Ask"},{"id":"ask","name":"Again"}]}),
            json!({"currentModeId":"ask","availableModes":[{"id":"ask","name":false}]}),
        ] {
            assert!(parse(&json!({"modes":modes})).is_err());
        }
        let mut adapter = Adapter::default();
        create(&mut adapter, "s", catalog());
        for invalid in [json!(null), json!(""), json!(42)] {
            let before = adapter.sessions["s"].thread(true);
            assert!(update(&mut adapter, invalid).is_err());
            assert_eq!(adapter.sessions["s"].thread(true), before);
        }
        let request = change(&mut adapter, "s", "code");
        assert!(
            adapter
                .receive(json!({"jsonrpc":"2.0","id":request.wire[0]["id"],"result":null}))
                .is_err()
        );
    }
}
