//! Advertised ACP commands only prepare text for the existing composer/send path.
use crate::{Error, Thread};
use fleetpane_runtime::acp::commands::Command;
use serde_json::Value;

/// Resolve the exact name against this thread's current advertisement. Ordinary
/// drafts and commands with arguments must be explicitly edited by the user.
pub fn prepare_prefix(thread: &Thread, name: &str, body: &str) -> Result<String, Error> {
    thread.validate()?;
    if matches!(thread.status.kind.as_str(), "archived" | "notLoaded") {
        return Err(Error::Projection(
            "Resume the ACP session before choosing a command".into(),
        ));
    }
    let command = thread
        .acp_commands
        .as_ref()
        .and_then(|commands| commands.iter().find(|command| command.name == name))
        .ok_or_else(|| {
            Error::Projection("ACP command is no longer advertised by this thread".into())
        })?;
    if !body.is_empty()
        && (!body.starts_with('/') || body.chars().any(|ch| ch.is_whitespace() || ch.is_control()))
    {
        return Err(Error::Projection("Preserve the current draft: clear it or select only a slash-command prefix before inserting a command".into()));
    }
    Ok(format!(
        "/{}{}",
        command.name,
        if command.input.is_some() { " " } else { "" }
    ))
}

impl Thread {
    pub(crate) fn apply_acp_commands(&mut self, params: &Value) -> Result<(), Error> {
        let commands: Vec<Command> = serde_json::from_value(
            params
                .get("commands")
                .cloned()
                .ok_or_else(|| Error::Projection("ACP command update lacks commands".into()))?,
        )?;
        // Validate the complete resulting projection before replacing anything,
        // including the aggregate conversation byte limit.
        let mut candidate = self.clone();
        candidate.acp_commands = Some(commands);
        candidate.validate()?;
        self.acp_commands = candidate.acp_commands;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleetpane_runtime::Event;
    use serde_json::json;

    fn thread() -> Thread {
        serde_json::from_value(json!({"id":"commands-thread","cwd":"/synthetic","preview":"","model":"confirmed-model","reasoningEffort":"confirmed-effort","status":{"type":"idle"},"turns":[{"id":"finished","status":"completed","items":[{"id":"message","type":"agentMessage","text":"Keep history"}],"error":null}]})).unwrap()
    }

    fn catalog() -> Value {
        json!([
            {"name":"Web:查/找","description":"  Search 中文\n", "input":{"hint":" query \n", "futureInput":{"a":null},"_meta":{"input":"opaque"}}, "future":[true,1,null],"_meta":{"nested":{"secret":false}}},
            {"name":"test","description":"Run tests"}
        ])
    }

    fn event(id: &str, commands: Value) -> Event {
        Event {
            sequence: 2,
            message: json!({"method":"thread/acpCommands/updated","params":{"threadId":id,"commands":commands}}),
        }
    }

    #[test]
    fn command_metadata_snapshot_preserves_exact_extensions_and_old_formats() {
        let mut thread = thread();
        thread.validate().unwrap();
        assert!(thread.acp_commands.is_none());
        assert!(
            serde_json::to_value(&thread)
                .unwrap()
                .get("acpCommands")
                .is_none()
        );
        thread.acp_commands = Some(serde_json::from_value(catalog()).unwrap());
        let snapshot = serde_json::to_value(&thread).unwrap();
        assert_eq!(snapshot["acpCommands"], catalog());
        let restored: Thread = serde_json::from_value(snapshot.clone()).unwrap();
        restored.validate().unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), snapshot);
        thread.acp_commands = Some(vec![]);
        assert_eq!(
            serde_json::to_value(&thread).unwrap()["acpCommands"],
            json!([])
        );
        let mut old = snapshot;
        old["acpCommands"] = Value::Null;
        assert!(
            serde_json::from_value::<Thread>(old)
                .unwrap()
                .acp_commands
                .is_none()
        );
    }

    #[test]
    fn command_metadata_events_are_exactly_scoped_and_replace_or_clear_without_activity() {
        let mut thread = thread();
        let before = serde_json::to_value(&thread).unwrap();
        assert!(
            !thread
                .apply(&event("other-thread", json!({"malformed":true})))
                .unwrap()
        );
        assert_eq!(serde_json::to_value(&thread).unwrap(), before);
        assert!(thread.apply(&event("commands-thread", catalog())).unwrap());
        let mut expected = before;
        expected["acpCommands"] = catalog();
        assert_eq!(serde_json::to_value(&thread).unwrap(), expected);
        let replacement = json!([{"name":"plan","description":"Only current command"}]);
        thread
            .apply(&event("commands-thread", replacement.clone()))
            .unwrap();
        assert_eq!(
            serde_json::to_value(&thread).unwrap()["acpCommands"],
            replacement
        );
        thread.apply(&event("commands-thread", json!([]))).unwrap();
        assert_eq!(thread.acp_commands, Some(vec![]));
        expected["acpCommands"] = json!([]);
        assert_eq!(serde_json::to_value(&thread).unwrap(), expected);

        thread.status.kind = "active".into();
        thread.turns[0].status = "inProgress".into();
        let before = serde_json::to_value(&thread).unwrap();
        thread.apply(&event("commands-thread", catalog())).unwrap();
        let after = serde_json::to_value(&thread).unwrap();
        assert_eq!(after["turns"], before["turns"]);
        assert_eq!(after["status"], before["status"]);
        assert_eq!(after["model"], before["model"]);
        assert_eq!(after["reasoningEffort"], before["reasoningEffort"]);
    }

    #[test]
    fn invalid_command_metadata_events_and_snapshots_are_refused_atomically() {
        let mut thread = thread();
        thread.apply(&event("commands-thread", catalog())).unwrap();
        let before = serde_json::to_value(&thread).unwrap();
        let too_many: Vec<_> = (0..129)
            .map(|i| json!({"name":format!("c{i}"),"description":""}))
            .collect();
        for commands in [
            json!(null),
            json!({}),
            json!([{"name":"missing-description"}]),
            json!([{"name":"new","description":"Valid"},{"name":"new","description":"Duplicate"}]),
            json!([{"name":"new","description":"Valid"},{"name":"invalid token","description":"Invalid"}]),
            json!([{"name":"new","description":"Valid"},{"name":"/leading-slash","description":"Invalid"}]),
            json!([{"name":"new","description":"x".repeat(8193)}]),
            json!([{"name":"new","description":"Valid","input":{"hint":"x".repeat(4097)}}]),
            json!([{"name":"new","description":"Valid","extra":"x".repeat(65536)}]),
            json!(too_many),
        ] {
            assert!(
                thread
                    .apply(&event("commands-thread", commands.clone()))
                    .is_err()
            );
            assert_eq!(serde_json::to_value(&thread).unwrap(), before);
            let mut snapshot = before.clone();
            snapshot["acpCommands"] = commands.clone();
            // null is the optional unsupported snapshot format, never a list update.
            if !commands.is_null() {
                match serde_json::from_value::<Thread>(snapshot) {
                    Ok(thread) => assert!(thread.validate().is_err()),
                    Err(error) => assert!(error.is_data()),
                }
            }
        }
        for params in [
            json!({"threadId":"commands-thread"}),
            json!({"commands":[]}),
            json!({"threadId":7,"commands":[]}),
        ] {
            let event = Event {
                sequence: 3,
                message: json!({"method":"thread/acpCommands/updated","params":params}),
            };
            assert!(thread.apply(&event).is_err());
            assert_eq!(serde_json::to_value(&thread).unwrap(), before);
        }
    }

    #[test]
    fn command_metadata_keeps_the_total_conversation_limit_atomic() {
        let mut thread = thread();
        thread.acp_commands = Some(vec![]);
        thread.preview.clear();
        let overhead = serde_json::to_vec(&thread).unwrap().len();
        thread.preview = "x".repeat(crate::MAX_CONVERSATION_BYTES - overhead - 32);
        thread.validate().unwrap();
        let before = thread.acp_commands.clone();
        assert!(thread.apply(&event("commands-thread", catalog())).is_err());
        assert_eq!(thread.acp_commands, before);
        thread.validate().unwrap();
    }

    #[test]
    fn prepare_prefix_uses_current_exact_command_and_preserves_input_hints() {
        let mut thread = thread();
        thread.apply(&event("commands-thread", catalog())).unwrap();
        let before = serde_json::to_value(&thread).unwrap();
        for body in ["", "/", "/W", "/Web:查/找", "/unknown"] {
            assert_eq!(
                prepare_prefix(&thread, "Web:查/找", body).unwrap(),
                "/Web:查/找 "
            );
            assert_eq!(prepare_prefix(&thread, "test", body).unwrap(), "/test");
        }
        assert!(prepare_prefix(&thread, "web:查/找", "").is_err());
        assert!(prepare_prefix(&thread, "/test", "").is_err());
        assert_eq!(serde_json::to_value(&thread).unwrap(), before);
        assert_eq!(
            thread.acp_commands.as_ref().unwrap()[0]
                .input
                .as_ref()
                .unwrap()
                .hint,
            " query \n"
        );
    }

    #[test]
    fn prepare_prefix_refuses_existing_drafts_arguments_stale_and_archived_choices() {
        let mut thread = thread();
        assert!(prepare_prefix(&thread, "test", "").is_err());
        thread.apply(&event("commands-thread", catalog())).unwrap();
        let before = serde_json::to_value(&thread).unwrap();
        for body in [
            "ordinary draft",
            "text",
            " ",
            "\n",
            " /test",
            "/test argument",
            "/test ",
            "/test\n",
            "/test\targ",
            "/test\u{2003}arg",
            "/test\0arg",
        ] {
            assert!(prepare_prefix(&thread, "test", body).is_err(), "{body:?}");
            assert_eq!(serde_json::to_value(&thread).unwrap(), before);
        }
        for status in ["archived", "notLoaded"] {
            thread.status.kind = status.into();
            assert!(prepare_prefix(&thread, "test", "").is_err());
        }
        thread.status.kind = "active".into();
        assert_eq!(prepare_prefix(&thread, "test", "").unwrap(), "/test");
        thread.apply(&event("commands-thread", json!([]))).unwrap();
        assert!(prepare_prefix(&thread, "test", "").is_err());
    }
}
