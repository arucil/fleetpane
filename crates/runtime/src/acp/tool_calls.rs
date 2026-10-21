//! Validate and merge provider tool snapshots without presentation state.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub path: String,
    pub line: Option<u32>,
}

/// ACP updates replace supplied values; null/omitted fields leave old values.
/// Defaults belong only to a new tool call, never to each incremental patch.
pub(super) fn merge(previous: Option<&Value>, update: &Value) -> Result<Value, String> {
    let mut merged = previous
        .cloned()
        .unwrap_or_else(|| json!({"status":"pending","kind":"other","content":[],"locations":[]}));
    let fields = update
        .as_object()
        .ok_or("ACP tool update is not an object")?;
    for (key, value) in fields {
        if previous.is_none() || !value.is_null() {
            merged[key] = value.clone();
        }
    }
    for key in ["title", "status", "kind"] {
        if !merged.get(key).is_some_and(Value::is_string) {
            return Err(format!("ACP tool {key} must be a string"));
        }
    }
    if merged
        .get("name")
        .is_some_and(|v| !v.is_null() && !v.is_string())
    {
        return Err("ACP tool name must be a string or null".into());
    }
    if merged
        .get("_meta")
        .is_some_and(|v| !v.is_null() && !v.is_object())
    {
        return Err("ACP tool _meta must be an object or null".into());
    }
    if !merged.get("content").is_some_and(Value::is_array) {
        return Err("ACP tool content must be an array".into());
    }
    let locations: Vec<Location> = serde_json::from_value(merged["locations"].clone())
        .map_err(|error| format!("invalid ACP tool locations: {error}"))?;
    if locations
        .iter()
        .any(|location| !std::path::Path::new(&location.path).is_absolute())
    {
        return Err("ACP tool location must be an absolute path".into());
    }
    Ok(merged)
}
