//! Codex reasoning arrays are indexed independently and retain exact text.

use crate::{Error, MAX_CONVERSATION_BYTES, Thread};
use serde::Deserialize;
use serde_json::Value;

pub(super) fn is_delta(method: &str) -> bool {
    matches!(
        method,
        "item/reasoning/summaryPartAdded"
            | "item/reasoning/summaryTextDelta"
            | "item/reasoning/textDelta"
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Target {
    turn_id: String,
    item_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SummaryIndex {
    summary_index: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContentIndex {
    content_index: usize,
}

#[derive(Deserialize)]
struct Delta {
    delta: String,
}

pub(super) fn apply(thread: &mut Thread, method: &str, params: Value) -> Result<(), Error> {
    let target: Target = serde_json::from_value(params.clone())?;
    let (field, index) = if method == "item/reasoning/textDelta" {
        (
            "content",
            serde_json::from_value::<ContentIndex>(params.clone())?.content_index,
        )
    } else {
        (
            "summary",
            serde_json::from_value::<SummaryIndex>(params.clone())?.summary_index,
        )
    };
    let delta = if method == "item/reasoning/summaryPartAdded" {
        None
    } else {
        Some(serde_json::from_value::<Delta>(params)?.delta)
    };
    // Even empty JSON strings need at least three bytes per indexed slot.
    // Bound sparse indices by the existing serialized conversation budget.
    if index >= MAX_CONVERSATION_BYTES / 3 {
        return Err(Error::Projection(
            "reasoning index exceeds the conversation byte limit".into(),
        ));
    }
    let bytes = serde_json::to_vec(thread)?.len();
    let item = thread
        .turns
        .iter_mut()
        .find(|turn| turn.id == target.turn_id)
        .and_then(|turn| turn.items.iter_mut().find(|item| item.id == target.item_id))
        .ok_or_else(|| {
            Error::Projection("reasoning delta has no item; reload conversation".into())
        })?;
    if item.kind != "reasoning" {
        return Err(Error::Projection(
            "reasoning delta does not target a reasoning item".into(),
        ));
    }
    let previous = item
        .fields
        .get(field)
        .ok_or_else(|| Error::Projection(format!("reasoning item has no {field} array")))?;
    let mut parts: Vec<String> = serde_json::from_value(previous.clone())?;
    if index >= parts.len() {
        parts.resize(index + 1, String::new());
    }
    if let Some(delta) = delta {
        parts[index].push_str(&delta);
    }
    let next = serde_json::to_value(parts)?;
    let next_bytes = bytes - serde_json::to_vec(previous)?.len() + serde_json::to_vec(&next)?.len();
    if next_bytes > MAX_CONVERSATION_BYTES {
        return Err(Error::Projection(
            "reasoning delta exceeds the 2 MiB conversation limit; reload conversation".into(),
        ));
    }
    item.fields.insert(field.into(), next);
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{Thread, content::Content};
    use fleetpane_runtime::Event;
    use serde_json::{Value, json};

    fn thread() -> Thread {
        serde_json::from_value(json!({
            "id":"s","cwd":"/synthetic","preview":"","status":{"type":"active"},
            "turns":[{"id":"t","status":"inProgress","items":[
                {"id":"r","type":"reasoning","summary":[],"content":[]},
                {"id":"a","type":"agentMessage","text":"Unchanged"}
            ]}]
        }))
        .unwrap()
    }

    fn event(method: &str, extra: Value) -> Event {
        let mut params = json!({"threadId":"s","turnId":"t","itemId":"r"});
        params
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        Event {
            sequence: 1,
            message: json!({"method":method,"params":params}),
        }
    }

    #[test]
    fn reasoning_stream_keeps_independent_parts_exact_text_and_item_identity() {
        let mut thread = thread();
        for (method, extra) in [
            ("item/reasoning/summaryPartAdded", json!({"summaryIndex":0})),
            (
                "item/reasoning/summaryTextDelta",
                json!({"summaryIndex":0,"delta":"  思考\n"}),
            ),
            ("item/reasoning/summaryPartAdded", json!({"summaryIndex":0})),
            (
                "item/reasoning/textDelta",
                json!({"contentIndex":0,"delta":"Detail "}),
            ),
            (
                "item/reasoning/summaryTextDelta",
                json!({"summaryIndex":0,"delta":"**exact**  "}),
            ),
            ("item/reasoning/summaryPartAdded", json!({"summaryIndex":1})),
            (
                "item/reasoning/summaryTextDelta",
                json!({"summaryIndex":1,"delta":"Second"}),
            ),
            (
                "item/reasoning/textDelta",
                json!({"contentIndex":1,"delta":"中文"}),
            ),
            (
                "item/reasoning/textDelta",
                json!({"contentIndex":0,"delta":"continued"}),
            ),
        ] {
            assert!(
                thread.apply(&event(method, extra)).unwrap(),
                "ignored {method}"
            );
        }
        let items = &thread.turns[0].items;
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "r");
        assert_eq!(
            items[0].fields["summary"],
            json!(["  思考\n**exact**  ", "Second"])
        );
        assert_eq!(
            items[0].fields["content"],
            json!(["Detail continued", "中文"])
        );
        assert_eq!(items[1].text().unwrap(), "Unchanged");
        assert!(
            matches!(items[0].content().unwrap(), Some(Content::Reasoning(text))
            if text == "  思考\n**exact**  \n\nSecond\n\nDetail continued\n\n中文")
        );
        let copy: Value = serde_json::from_str(&items[0].text().unwrap()).unwrap();
        assert_eq!(copy["summary"], items[0].fields["summary"]);
        // Final item snapshots replace streamed arrays, rather than append twice.
        let final_item = serde_json::to_value(&items[0]).unwrap();
        assert!(
            thread
                .apply(&event("item/completed", json!({"item":final_item})))
                .unwrap()
        );
        assert_eq!(thread.turns[0].items[0].fields["summary"], copy["summary"]);
    }

    #[test]
    fn reasoning_invalid_targets_indices_and_payloads_fail_without_altering_items() {
        let mut thread = thread();
        let before = serde_json::to_value(&thread).unwrap();
        for extra in [
            json!({"summaryIndex":-1,"delta":"x"}),
            json!({"summaryIndex":i64::MAX,"delta":"x"}),
            json!({"summaryIndex":0,"delta":"x".repeat(crate::MAX_CONVERSATION_BYTES)}),
            json!({"summaryIndex":0,"delta":4}),
            json!({"summaryIndex":0}),
            json!({"summaryIndex":0,"delta":"x","itemId":"a"}),
            json!({"summaryIndex":0,"delta":"x","itemId":"missing"}),
            json!({"summaryIndex":0,"delta":"x","turnId":"missing"}),
        ] {
            assert!(
                thread
                    .apply(&event("item/reasoning/summaryTextDelta", extra))
                    .is_err()
            );
            assert_eq!(serde_json::to_value(&thread).unwrap(), before);
        }
        let foreign = event(
            "item/reasoning/textDelta",
            json!({"threadId":"other","contentIndex":0,"delta":"foreign"}),
        );
        assert!(!thread.apply(&foreign).unwrap());
        assert_eq!(serde_json::to_value(&thread).unwrap(), before);
        thread.turns[0].items[0]
            .fields
            .insert("summary".into(), json!([4]));
        let before = serde_json::to_value(&thread).unwrap();
        assert!(
            thread
                .apply(&event(
                    "item/reasoning/summaryPartAdded",
                    json!({"summaryIndex":0})
                ))
                .is_err()
        );
        assert_eq!(serde_json::to_value(&thread).unwrap(), before);
    }

    #[test]
    fn reasoning_indexed_slots_do_not_compact_or_discard_empty_parts() {
        let mut thread = thread();
        assert!(
            thread
                .apply(&event(
                    "item/reasoning/textDelta",
                    json!({"contentIndex":2,"delta":"Last"})
                ))
                .unwrap()
        );
        assert!(
            thread
                .apply(&event(
                    "item/reasoning/textDelta",
                    json!({"contentIndex":0,"delta":"First"})
                ))
                .unwrap()
        );
        assert_eq!(
            thread.turns[0].items[0].fields["content"],
            json!(["First", "", "Last"])
        );
    }
}
