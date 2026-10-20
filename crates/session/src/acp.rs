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
    pub options: Vec<PermissionOption>,
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
            || permission.options.is_empty()
            || permission.options.len() > 16
        {
            return Err("ACP permission must contain a session and 1 to 16 choices".into());
        }
        Ok(Some(permission))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
