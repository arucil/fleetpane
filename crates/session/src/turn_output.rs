//! Explicit text export of one observed turn, without fetching more history.
use crate::{Error, Thread};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, PartialEq, Eq)]
pub struct Answer {
    pub text: String,
    pub text_items: usize,
    pub non_text_parts: usize,
}

/// Only assistant text is exported. Loaded item order and whitespace are kept;
/// distinct messages are separated by two newlines. Tools, plans, thoughts and
/// user input never enter the answer. The caller must describe the loaded scope.
pub fn loaded_answer(thread: &Thread, turn_id: &str) -> Result<Answer, Error> {
    let turn = thread
        .turns
        .iter()
        .find(|turn| turn.id == turn_id)
        .ok_or_else(|| Error::Projection("Requested turn is no longer loaded".into()))?;
    let mut messages = Vec::new();
    let mut non_text_parts = 0;
    for item in &turn.items {
        match item.kind.as_str() {
            "agentMessage" => messages.push(item.text()?),
            "acpMessage" => {
                #[derive(Deserialize)]
                struct Message {
                    content: Vec<Value>,
                }
                #[derive(Deserialize)]
                struct Text {
                    text: String,
                }
                let message: Message = serde_json::from_value(serde_json::to_value(&item.fields)?)?;
                let mut parts = Vec::new();
                for part in message.content {
                    if part.get("type").and_then(Value::as_str) == Some("text") {
                        parts.push(serde_json::from_value::<Text>(part)?.text);
                    } else {
                        non_text_parts += 1;
                    }
                }
                if !parts.is_empty() {
                    messages.push(parts.join("\n"));
                }
            }
            _ => {}
        }
    }
    if messages.iter().all(String::is_empty) {
        return Err(Error::Projection(
            "This loaded turn has no assistant text to copy".into(),
        ));
    }
    let text_items = messages.len();
    let text = messages.join("\n\n");
    Ok(Answer {
        text,
        text_items,
        non_text_parts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn answer_preserves_order_whitespace_and_exact_turn_without_leaking_other_kinds() {
        let thread: Thread = serde_json::from_value(json!({
            "id":"session","cwd":"/synthetic","preview":"","status":{"type":"active"},
            "turns":[{"id":"older","status":"completed","items":[
                {"id":"same-id","type":"agentMessage","text":"older answer"}
            ]},{"id":"selected:turn","status":"inProgress","items":[
                {"id":"u","type":"userMessage","content":[{"type":"text","text":"private user input"}]},
                {"id":"same-id","type":"agentMessage","text":"  中文\r\n"},
                {"id":"tool","type":"commandExecution","aggregatedOutput":"secret tool output"},
                {"id":"plan","type":"plan","text":"private plan"},
                {"id":"thought","type":"reasoning","content":["private thought"]},
                {"id":"acp","type":"acpMessage","content":[
                    {"type":"text","text":"**Markdown**  "},
                    {"type":"image","data":"binary","mimeType":"image/png"},
                    {"type":"text","text":"尾部\n"},
                    {"type":"future-part","value":"opaque"}
                ]},
                {"id":"final","type":"agentMessage","text":"end  "}
            ]}]
        })).unwrap();
        let before = serde_json::to_value(&thread).unwrap();
        assert_eq!(
            loaded_answer(&thread, "selected:turn").unwrap(),
            Answer {
                text: "  中文\r\n\n\n**Markdown**  \n尾部\n\n\nend  ".into(),
                text_items: 3,
                non_text_parts: 2,
            }
        );
        assert_eq!(
            loaded_answer(&thread, "older").unwrap().text,
            "older answer"
        );
        assert!(loaded_answer(&thread, "absent").is_err());
        assert_eq!(serde_json::to_value(thread).unwrap(), before);
    }

    #[test]
    fn malformed_assistant_is_error_and_non_text_only_does_not_replace_clipboard() {
        let mut thread: Thread = serde_json::from_value(json!({
            "id":"s","cwd":"/synthetic","preview":"","status":{"type":"idle"},
            "turns":[{"id":"t","status":"failed","items":[
                {"id":"a","type":"acpMessage","content":[{"type":"image","data":"x"}]}
            ]}]
        }))
        .unwrap();
        assert!(
            loaded_answer(&thread, "t")
                .unwrap_err()
                .to_string()
                .contains("no assistant text")
        );
        thread.turns[0].items[0]
            .fields
            .insert("content".into(), json!([{"type":"text","text":17}]));
        assert!(loaded_answer(&thread, "t").is_err());
        thread.turns[0].items[0].kind = "agentMessage".into();
        assert!(loaded_answer(&thread, "t").is_err());
        thread.turns[0].items[0]
            .fields
            .insert("text".into(), json!(""));
        let mut second = thread.turns[0].items[0].clone();
        second.id = "b".into();
        thread.turns[0].items.push(second);
        assert!(loaded_answer(&thread, "t").is_err());
    }
}
