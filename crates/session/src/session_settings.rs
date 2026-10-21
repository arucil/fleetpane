//! Provider-reported session settings are distinct from per-turn model execution.
use crate::{Error, Thread};
use serde::Deserialize;
use serde_json::Value;

fn validate_text(value: &str, maximum: usize, field: &str) -> Result<(), Error> {
    if value.trim().is_empty() || value.len() > maximum {
        return Err(Error::Projection(format!(
            "provider session {field} must contain text and fit within {maximum} bytes"
        )));
    }
    Ok(())
}

impl Thread {
    pub(crate) fn validate_session_settings(&self) -> Result<(), Error> {
        for (value, maximum, field) in [
            (self.model.as_deref(), 1024, "model"),
            (self.model_provider.as_deref(), 512, "model provider"),
            (self.reasoning_effort.as_deref(), 128, "reasoning effort"),
        ] {
            if let Some(value) = value {
                validate_text(value, maximum, field)?;
            }
        }
        Ok(())
    }

    pub(crate) fn apply_session_settings(&mut self, params: &Value) -> Result<(), Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Settings {
            model: String,
            model_provider: String,
            effort: Option<String>,
        }
        let settings: Settings =
            serde_json::from_value(params.get("threadSettings").cloned().ok_or_else(|| {
                Error::Projection("provider settings update lacks threadSettings".into())
            })?)?;
        validate_text(&settings.model, 1024, "model")?;
        validate_text(&settings.model_provider, 512, "model provider")?;
        if let Some(effort) = &settings.effort {
            validate_text(effort, 128, "reasoning effort")?;
        }
        self.model = Some(settings.model);
        self.model_provider = Some(settings.model_provider);
        self.reasoning_effort = settings.effort;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::Thread;
    use fleetpane_runtime::Event;
    use serde_json::{Value, json};

    fn thread() -> Thread {
        serde_json::from_value(json!({"id":"settings-thread","cwd":"/synthetic","preview":"","name":null,"status":{"type":"idle"},"turns":[],"model":"provider-confirmed","modelProvider":"private-fixture","reasoningEffort":"future-effort"})).unwrap()
    }

    fn event(thread_id: &str, settings: Value) -> Event {
        Event {
            sequence: 7,
            message: json!({"method":"thread/settings/updated","params":{"threadId":thread_id,"threadSettings":settings}}),
        }
    }

    #[test]
    fn provider_settings_survive_metadata_roundtrip_without_using_next_message_choices() {
        let thread = thread();
        thread.validate().unwrap();
        let saved = serde_json::to_value(&thread).unwrap();
        assert_eq!(saved["model"], "provider-confirmed");
        assert_eq!(saved["modelProvider"], "private-fixture");
        assert_eq!(saved["reasoningEffort"], "future-effort");
    }

    #[test]
    fn settings_events_route_atomically_and_null_effort_clears_previous_setting() {
        let mut thread = thread();
        let settings =
            json!({"model":"new-confirmed","modelProvider":"another-provider","effort":null});
        let before = serde_json::to_value(&thread).unwrap();
        assert!(
            !thread
                .apply(&event("another-thread", settings.clone()))
                .unwrap()
        );
        assert_eq!(serde_json::to_value(&thread).unwrap(), before);
        assert!(thread.apply(&event("settings-thread", settings)).unwrap());
        let saved = serde_json::to_value(&thread).unwrap();
        assert_eq!(saved["model"], "new-confirmed");
        assert_eq!(saved["modelProvider"], "another-provider");
        assert!(saved.get("reasoningEffort").is_none_or(Value::is_null));
        for invalid in [
            json!({"model":false,"modelProvider":"valid"}),
            json!({"model":"","modelProvider":"valid"}),
            json!({"model":"valid"}),
            json!({"model":"valid","modelProvider":"valid","effort":42}),
            json!({"model":"valid","modelProvider":"valid","effort":"x".repeat(129)}),
        ] {
            assert!(thread.apply(&event("settings-thread", invalid)).is_err());
            assert_eq!(serde_json::to_value(&thread).unwrap(), saved);
        }
    }
}
