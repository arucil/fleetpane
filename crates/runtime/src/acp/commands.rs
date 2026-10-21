//! ACP v1 advertised slash commands. Availability belongs to the live session only.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_COMMANDS: usize = 128;
pub const MAX_COMMAND_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Input>,
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Map<String, Value>>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Input {
    pub hint: String,
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Map<String, Value>>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Validate a complete replacement without normalizing provider names or hints.
pub fn validate(commands: &[Command]) -> Result<(), String> {
    if commands.len() > MAX_COMMANDS {
        return Err("ACP command list exceeds 128 commands".into());
    }
    let mut names = BTreeSet::new();
    for command in commands {
        if command.name.is_empty()
            || command.name.len() > 256
            || command.name.starts_with('/')
            || command
                .name
                .chars()
                .any(|ch| ch.is_whitespace() || ch.is_control())
            || !names.insert(&command.name)
        {
            return Err("ACP commands require unique nonempty token names of at most 256 bytes, without whitespace, controls or a leading slash".into());
        }
        if command.description.len() > 8192
            || command
                .input
                .as_ref()
                .is_some_and(|input| input.hint.len() > 4096)
        {
            return Err("ACP command description/input hint exceeds 8192/4096 bytes".into());
        }
    }
    if serde_json::to_vec(commands)
        .map_err(|error| format!("cannot serialize ACP commands: {error}"))?
        .len()
        > MAX_COMMAND_BYTES
    {
        return Err("ACP command list exceeds 64 KiB serialized".into());
    }
    Ok(())
}

pub(super) fn parse(update: &Value) -> Result<Vec<Command>, String> {
    let commands: Vec<Command> = serde_json::from_value(
        update
            .get("availableCommands")
            .cloned()
            .ok_or("ACP command update lacks availableCommands")?,
    )
    .map_err(|error| format!("invalid ACP commands: {error}"))?;
    validate(&commands)?;
    Ok(commands)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_names_hints_and_opaque_extensions_roundtrip() {
        let value = json!([
            {"name":"Web:查/找","description":"  Search 中文\n", "input":{"hint":" query \n中文 ","_meta":{"vendor":[true,null]},"future":{"a":1}},"_meta":{"provider":{"enabled":false}},"extra":[1,"",null]},
            {"name":"web","description":""},
            {"name":"Web","description":"Different exact name"}
        ]);
        let commands = parse(&json!({"availableCommands":value})).unwrap();
        assert_eq!(commands[0].name, "Web:查/找");
        assert_eq!(commands[0].input.as_ref().unwrap().hint, " query \n中文 ");
        assert_eq!(serde_json::to_value(commands).unwrap(), value);
    }

    #[test]
    fn command_tokens_and_field_byte_limits_are_validated() {
        for name in [
            "",
            "/web",
            "two words",
            "tab\t",
            "line\n",
            "a\0b",
            "space\u{2003}x",
            "control\u{7f}",
        ] {
            assert!(
                parse(&json!({"availableCommands":[{"name":name,"description":""}]})).is_err(),
                "{name:?}"
            );
        }
        for (name, description, hint) in [
            ("x".repeat(257), String::new(), String::new()),
            ("查".repeat(86), String::new(), String::new()),
            ("valid".into(), "d".repeat(8193), String::new()),
            ("valid".into(), String::new(), "h".repeat(4097)),
        ] {
            assert!(parse(&json!({"availableCommands":[{"name":name,"description":description,"input":{"hint":hint}}]})).is_err());
        }
        parse(&json!({"availableCommands":[{"name":"n".repeat(256),"description":"d".repeat(8192),"input":{"hint":"h".repeat(4096)}}]})).unwrap();
    }

    #[test]
    fn malformed_shapes_and_duplicate_names_are_refused() {
        assert!(parse(&json!({})).is_err());
        for value in [
            json!(null),
            json!({}),
            json!([{}]),
            json!([{"name":"web"}]),
            json!([{"name":1,"description":"d"}]),
            json!([{"name":"web","description":false}]),
            json!([{"name":"web","description":"d","input":{}}]),
            json!([{"name":"web","description":"d","input":{"hint":42}}]),
            json!([{"name":"web","description":"d","_meta":[]}]),
            json!([{"name":"web","description":"d","input":{"hint":"h","_meta":false}}]),
            json!([{"name":"web","description":"a"},{"name":"web","description":"b"}]),
        ] {
            assert!(
                parse(&json!({"availableCommands":value})).is_err(),
                "{value}"
            );
        }
    }

    #[test]
    fn whole_list_count_and_serialized_extension_bytes_are_bounded() {
        let mut value: Vec<_> = (0..MAX_COMMANDS)
            .map(|i| json!({"name":format!("command-{i}"),"description":""}))
            .collect();
        parse(&json!({"availableCommands":value})).unwrap();
        value.push(json!({"name":"extra","description":""}));
        assert!(parse(&json!({"availableCommands":value})).is_err());

        let mut value = json!([{"name":"web","description":"","_meta":{"padding":""}}]);
        let overhead = serde_json::to_vec(&value).unwrap().len();
        value[0]["_meta"]["padding"] = json!("x".repeat(MAX_COMMAND_BYTES - overhead));
        parse(&json!({"availableCommands":value})).unwrap();
        value[0]["_meta"]["padding"] = json!("x".repeat(MAX_COMMAND_BYTES - overhead + 1));
        let error = parse(&json!({"availableCommands":value})).unwrap_err();
        assert!(error.contains("64 KiB"), "{error}");
    }
}
