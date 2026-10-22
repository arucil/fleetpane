//! Navigation over observed user prompts; no history fetch or provider action.
use crate::{Error, Thread};
use serde::Deserialize;
use serde_json::Value;

const PREVIEW_CHARACTERS: usize = 90;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    pub turn_id: String,
    pub item_id: String,
    pub preview: String,
}

/// Loaded order is authoritative. Image-only prompts retain their own identity;
/// tools, assistant messages and opaque future items are never inferred as users.
pub fn loaded(thread: &Thread) -> Result<Vec<Prompt>, Error> {
    let mut prompts = Vec::new();
    for turn in &thread.turns {
        for item in turn.items.iter().filter(|item| item.kind == "userMessage") {
            #[derive(Deserialize)]
            struct Message {
                content: Vec<Value>,
            }
            #[derive(Deserialize)]
            struct Text {
                text: String,
            }
            let message: Message = serde_json::from_value(serde_json::to_value(&item.fields)?)?;
            let mut text = Vec::new();
            let mut other = 0;
            for part in message.content {
                if part.get("type").and_then(Value::as_str) == Some("text") {
                    text.push(serde_json::from_value::<Text>(part)?.text);
                } else {
                    other += 1;
                }
            }
            // Collapse whitespace for the menu only. Stored source is untouched.
            let text = text
                .join("\n")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let mut characters = text.chars();
            let mut preview: String = characters.by_ref().take(PREVIEW_CHARACTERS).collect();
            if characters.next().is_some() {
                preview.push('…');
            }
            if preview.is_empty() {
                preview = format!("No text · {other} non-text parts");
            }
            prompts.push(Prompt {
                turn_id: turn.id.clone(),
                item_id: item.id.clone(),
                preview,
            });
        }
    }
    Ok(prompts)
}

/// A menu belongs to one exact session and one loaded user item, even when IDs
/// collide across turns or the selected session changes while it is open.
pub fn validate(thread: &Thread, session_id: &str, prompt: &Prompt) -> Result<(), Error> {
    if thread.id != session_id
        || !thread.turns.iter().any(|turn| {
            turn.id == prompt.turn_id
                && turn
                    .items
                    .iter()
                    .any(|item| item.id == prompt.item_id && item.kind == "userMessage")
        })
    {
        return Err(Error::Projection(
            "The selected prompt is no longer in this loaded session".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ordered_prompts_keep_unicode_media_and_turn_identity_without_mutation() {
        let thread: Thread = serde_json::from_value(json!({
            "id":"session","cwd":"/fixture","preview":"","status":{"type":"idle"},
            "turns":[{"id":"older","status":"completed","items":[
                {"id":"same","type":"userMessage","content":[{"type":"text","text":" 中文\r\n **source**  "}]},
                {"id":"agent","type":"acpMessage","content":[{"type":"text","text":"not a prompt"}]},
                {"id":"tool","type":"acpToolCall"}
            ]},{"id":"newer","status":"inProgress","items":[
                {"id":"same","type":"userMessage","content":[{"type":"image","data":"opaque"}]},
                {"id":"long","type":"userMessage","content":[{"type":"text","text":"汉".repeat(200)}]},
                {"id":"empty","type":"userMessage","content":[]},
                {"id":"future","type":"future-user","content":[]}
            ]}]
        })).unwrap();
        let before = serde_json::to_value(&thread).unwrap();
        let prompts = loaded(&thread).unwrap();
        assert_eq!(prompts.len(), 4);
        assert_eq!(prompts[0].preview, "中文 **source**");
        assert_eq!(prompts[1].preview, "No text · 1 non-text parts");
        assert_eq!(prompts[2].preview, format!("{}…", "汉".repeat(90)));
        assert_eq!(prompts[3].preview, "No text · 0 non-text parts");
        assert_eq!(prompts[0].item_id, prompts[1].item_id);
        assert_ne!(prompts[0].turn_id, prompts[1].turn_id);
        assert!(validate(&thread, "session", &prompts[0]).is_ok());
        assert!(validate(&thread, "other-session", &prompts[0]).is_err());
        let mut stale = prompts[0].clone();
        stale.turn_id = "absent".into();
        assert!(validate(&thread, "session", &stale).is_err());
        stale.turn_id = "older".into();
        stale.item_id = "agent".into();
        assert!(validate(&thread, "session", &stale).is_err());
        assert_eq!(serde_json::to_value(thread).unwrap(), before);
    }

    #[test]
    fn malformed_known_prompt_is_reported_instead_of_silently_omitted() {
        for content in [json!(17), json!([{"type":"text","text":false}])] {
            let thread: Thread = serde_json::from_value(json!({
                "id":"s","cwd":"/fixture","preview":"","status":{"type":"idle"},
                "turns":[{"id":"t","status":"completed","items":[
                    {"id":"u","type":"userMessage","content":content}
                ]}]
            }))
            .unwrap();
            assert!(loaded(&thread).is_err());
        }
    }
}
