//! ACP's advertised session configuration and exact permission options.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigOption {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub current_value: Value,
    #[serde(default)]
    pub options: Vec<Choice>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Choice {
    Value {
        value: String,
        name: String,
    },
    Group {
        group: String,
        name: String,
        options: Vec<Choice>,
    },
}
impl Choice {
    pub fn values(&self) -> Vec<(&str, &str)> {
        match self {
            Self::Value { value, name } => vec![(value, name)],
            Self::Group { options, .. } => options.iter().flat_map(Self::values).collect(),
        }
    }
}
impl ConfigOption {
    pub fn choices(&self) -> Vec<(&str, &str)> {
        self.options.iter().flat_map(Choice::values).collect()
    }
    pub fn selected_name(&self) -> &str {
        self.choices()
            .into_iter()
            .find(|(value, _)| Some(*value) == self.current_value.as_str())
            .map_or("Unavailable selection", |(_, name)| name)
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Permission {
    pub session_id: String,
    pub tool_call: PermissionTool,
    pub options: Vec<PermissionOption>,
}

/// The request carries a ToolCallUpdate, even if no tool event preceded it.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionTool {
    pub tool_call_id: String,
    pub title: Option<String>,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub raw_input: Option<Value>,
    pub raw_output: Option<Value>,
    pub content: Option<Vec<Value>>,
    pub locations: Option<Vec<fleetpane_runtime::acp::tool_calls::Location>>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionOption {
    pub option_id: String,
    pub name: String,
    pub kind: String,
}
impl Permission {
    pub fn decode(pending: &Value) -> Result<Option<Self>, String> {
        if pending.get("method").and_then(Value::as_str) != Some("session/request_permission") {
            return Ok(None);
        }
        let permission: Self = serde_json::from_value(
            pending
                .get("params")
                .cloned()
                .ok_or("ACP permission lacks params")?,
        )
        .map_err(|e| e.to_string())?;
        if permission.session_id.is_empty()
            || permission.tool_call.tool_call_id.is_empty()
            || permission.options.is_empty()
            || permission.options.len() > 16
        {
            return Err("ACP permission must contain a session and 1 to 16 choices".into());
        }
        if pending
            .pointer("/params/threadId")
            .is_some_and(|id| id != &permission.session_id)
        {
            return Err("ACP permission routing does not match its session".into());
        }
        Ok(Some(permission))
    }

    /// Request fields take precedence; the current turn only fills omitted/null fields.
    /// The preview never mutates the observed tool or makes it an executed operation.
    pub fn review(
        &self,
        thread: Option<&crate::Thread>,
    ) -> Result<crate::content::ToolCall, String> {
        if thread.is_some_and(|thread| thread.id != self.session_id) {
            return Err("ACP permission belongs to another session".into());
        }
        let observed = thread
            .and_then(crate::Thread::active_turn)
            .into_iter()
            .flat_map(|turn| &turn.items)
            .filter(|item| item.kind == "acpToolCall")
            .filter_map(|item| item.fields.get("call"))
            .find(|call| {
                call.get("toolCallId")
                    .is_some_and(|id| id == &self.tool_call.tool_call_id)
            })
            .map(|call| {
                serde_json::from_value::<crate::content::ToolCall>(call.clone())
                    .map_err(|error| format!("invalid observed permission tool: {error}"))
            })
            .transpose()?;
        let request = &self.tool_call;
        let merged = crate::content::ToolCall {
            title: request
                .title
                .clone()
                .or_else(|| observed.as_ref().map(|call| call.title.clone()))
                .unwrap_or_else(|| "Tool permission".into()),
            status: request
                .status
                .clone()
                .or_else(|| observed.as_ref().map(|call| call.status.clone()))
                .unwrap_or_else(|| "pending".into()),
            name: request
                .name
                .clone()
                .or_else(|| observed.as_ref().and_then(|call| call.name.clone())),
            kind: request
                .kind
                .clone()
                .or_else(|| observed.as_ref().and_then(|call| call.kind.clone())),
            raw_input: request
                .raw_input
                .clone()
                .or_else(|| observed.as_ref().and_then(|call| call.raw_input.clone())),
            raw_output: request
                .raw_output
                .clone()
                .or_else(|| observed.as_ref().and_then(|call| call.raw_output.clone())),
            content: request
                .content
                .clone()
                .or_else(|| observed.as_ref().and_then(|call| call.content.clone())),
            locations: request
                .locations
                .clone()
                .or_else(|| observed.as_ref().and_then(|call| call.locations.clone())),
        };
        if merged
            .locations
            .iter()
            .flatten()
            .any(|location| !std::path::Path::new(&location.path).is_absolute())
        {
            return Err("ACP permission location must be absolute".into());
        }
        merged.parts().map_err(|error| error.to_string())?;
        Ok(merged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn standalone_permission_preserves_falsy_input_and_rejects_wrong_session() {
        let pending = json!({"method":"session/request_permission","params":{"sessionId":"s","threadId":"s","toolCall":{"toolCallId":"standalone","title":"Review plan","rawInput":false,"rawOutput":0,"content":[]},"options":[{"optionId":"yes","name":"Allow","kind":"allow_once"}]}});
        let request = Permission::decode(&pending).unwrap().unwrap();
        let call = request.review(None).unwrap();
        assert_eq!(call.title, "Review plan");
        assert_eq!(call.raw_input, Some(json!(false)));
        assert_eq!(call.raw_output, Some(json!(0)));
        let mut wrong = pending.clone();
        wrong["params"]["threadId"] = json!("other");
        assert!(Permission::decode(&wrong).is_err());
        let mut malformed = pending;
        malformed["params"]["toolCall"]["content"] = json!(42);
        assert!(Permission::decode(&malformed).is_err());
    }

    #[test]
    fn permission_update_replaces_observed_fields_without_changing_history() {
        let mut thread: crate::Thread = serde_json::from_value(json!({"id":"s","cwd":"/tmp","preview":"","name":null,"historyMode":"acp-observed","status":{"type":"active"},"turns":[{"id":"turn","status":"inProgress","error":null,"items":[{"id":"tool","type":"acpToolCall","call":{"toolCallId":"call","title":"Old title","status":"pending","rawInput":{"old":true},"content":[{"type":"content","content":{"type":"text","text":"Old content"}}],"locations":[{"path":"/tmp/old","line":0}]}}]}]})).unwrap();
        let before = serde_json::to_value(&thread).unwrap();
        let pending = json!({"method":"session/request_permission","params":{"sessionId":"s","toolCall":{"toolCallId":"call","title":"New title","rawInput":{"new":false},"content":[],"locations":[]},"options":[{"optionId":"no","name":"Reject","kind":"reject_once"}]}});
        let mut request = Permission::decode(&pending).unwrap().unwrap();
        let review = request.review(Some(&thread)).unwrap();
        assert_eq!(review.title, "New title");
        assert_eq!(review.raw_input, Some(json!({"new":false})));
        assert_eq!(review.content, Some(vec![]));
        assert_eq!(review.locations, Some(vec![]));
        assert_eq!(serde_json::to_value(&thread).unwrap(), before);
        request.tool_call.raw_input = None;
        assert_eq!(
            request.review(Some(&thread)).unwrap().raw_input,
            Some(json!({"old":true}))
        );
        thread.id = "other".into();
        assert!(request.review(Some(&thread)).is_err());
        thread.id = "s".into();
        thread.turns[0].status = "completed".into();
        assert_eq!(request.review(Some(&thread)).unwrap().raw_input, None);
    }

    #[test]
    fn grouped_config_keeps_exact_provider_values() {
        let option: ConfigOption = serde_json::from_value(json!({"id":"model","name":"Model","type":"select","currentValue":"[\"deepseek\",\"flash\"]","options":[{"group":"deepseek","name":"DeepSeek","options":[{"value":"[\"deepseek\",\"flash\"]","name":"Flash"}]}]})).unwrap();
        assert_eq!(option.selected_name(), "Flash");
        assert_eq!(
            option.choices(),
            vec![("[\"deepseek\",\"flash\"]", "Flash")]
        );
        assert!(serde_json::from_value::<ConfigOption>(json!({"id":"bad","name":"bad","type":"select","currentValue":"a","options":[{"group":"g","name":"g"}]})).is_err());
    }
}
