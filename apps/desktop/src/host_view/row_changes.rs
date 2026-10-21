//! Reconcile retained card entities by identity without cloning unchanged rows.
use crate::conversation_item::Row;
use std::collections::{BTreeMap, BTreeSet};

/// Whether an already-applied selected-session event can change Row inputs.
pub(super) fn affects_rows(event: &fleetpane_runtime::Event, applied: bool) -> bool {
    // These fields are outside Row::turn/Row::item. New content event types
    // remain conservative: the selected-session decoder controls `applied`.
    applied
        && !matches!(
            event
                .message
                .get("method")
                .and_then(serde_json::Value::as_str),
            Some(
                "thread/name/updated"
                    | "thread/status/changed"
                    | "thread/acpSessionInfo/updated"
                    | "thread/config/updated"
                    | "thread/acpModes/updated"
                    | "turn/plan/updated"
            )
        )
}

pub(super) struct Plan<'a> {
    pub retained: BTreeSet<&'a str>,
    pub updated: Vec<usize>,
}

pub(super) fn plan<'a>(before: &[Row], after: &'a [Row]) -> Plan<'a> {
    let old: BTreeMap<_, _> = before.iter().map(|row| (row.id.as_str(), row)).collect();
    Plan {
        // A session switch clears previous rows before rebuilding, but retained
        // entities can still exist. Always reconcile the actual cache to this set.
        retained: after.iter().map(|row| row.id.as_str()).collect(),
        updated: after
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                (old.get(row.id.as_str()).copied() != Some(row)).then_some(index)
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleetpane_session::content::Content;
    use serde_json::json;

    #[test]
    fn streamed_reasoning_appears_and_only_updates_its_retained_card() {
        let mut thread: fleetpane_session::Thread = serde_json::from_value(json!({
            "id":"s","cwd":"/synthetic","preview":"","status":{"type":"active"},
            "turns":[{"id":"t","status":"inProgress","items":[
                {"id":"r","type":"reasoning","summary":[],"content":[]},
                {"id":"a","type":"agentMessage","text":"Keep this card"}
            ]}]
        }))
        .unwrap();
        let hidden = Row::conversation(&thread);
        assert_eq!(hidden.len(), 2);
        let mut event = fleetpane_runtime::Event {
            sequence: 1,
            message: json!({"method":"item/reasoning/summaryTextDelta","params":{
                "threadId":"s","turnId":"t","itemId":"r","summaryIndex":0,"delta":"思考 "
            }}),
        };
        let applied = thread.apply(&event).unwrap();
        assert!(affects_rows(&event, applied));
        let visible = Row::conversation(&thread);
        assert_eq!(visible.len(), 3);
        assert_eq!(plan(&hidden, &visible).updated, [1]);
        let identity = visible[1].id.clone();
        event.message["params"]["delta"] = json!("continued");
        let applied = thread.apply(&event).unwrap();
        assert!(affects_rows(&event, applied));
        let updated = Row::conversation(&thread);
        assert_eq!(plan(&visible, &updated).updated, [1]);
        assert_eq!(updated[1].id, identity);
        assert!(
            matches!(&updated[1].content, Content::Reasoning(text) if text == "思考 continued")
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&updated[1].copy).unwrap()["summary"],
            json!(["思考 continued"])
        );
    }

    #[test]
    fn item_named_status_does_not_replace_the_turn_card() {
        let thread: fleetpane_session::Thread = serde_json::from_value(json!({
            "id":"s","cwd":"/synthetic","preview":"","status":{"type":"idle"},
            "turns":[{"id":"t","status":"completed","items":[{"id":"status","type":"agentMessage","text":"Exact status item"}]}]
        })).unwrap();
        let rows = Row::conversation(&thread);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            plan(&[], &rows).retained.len(),
            2,
            "message overwrites turn status entity"
        );
    }

    #[test]
    fn delimiters_in_provider_ids_do_not_alias_other_turn_items() {
        let mut thread: fleetpane_session::Thread = serde_json::from_value(json!({
            "id":"s","cwd":"/synthetic","preview":"","status":{"type":"idle"},
            "turns":[
                {"id":"a:b","status":"completed","items":[{"id":"c","type":"agentMessage","text":"First source"}]},
                {"id":"a","status":"completed","items":[{"id":"b:c","type":"agentMessage","text":"Second source"}]}
            ]
        })).unwrap();
        let before = Row::conversation(&thread);
        assert_eq!(
            plan(&[], &before).retained.len(),
            4,
            "different turn/item tuples share one entity"
        );
        thread.turns[1].items[0]
            .fields
            .insert("text".into(), json!("Second updated"));
        let after = Row::conversation(&thread);
        assert_eq!(plan(&before, &after).updated, [3]);
    }

    #[test]
    fn metadata_keeps_row_inputs_unchanged_and_does_not_rebuild_the_timeline() {
        let mut thread: fleetpane_session::Thread = serde_json::from_value(json!({
            "id":"s","cwd":"/synthetic","preview":"","status":{"type":"idle"},
            "turns":[{"id":"t","status":"completed","items":[{"id":"m","type":"agentMessage","text":"Exact **message** 中文"}]}]
        })).unwrap();
        let before_turn = Row::turn(&thread.id, &thread.turns[0]);
        let before_item = Row::item("s", "t", &thread.turns[0].items[0]);
        for (sequence, (method, params)) in [
            ("thread/name/updated", json!({"threadId":"s","threadName":"New name"})),
            ("thread/status/changed", json!({"threadId":"s","status":{"type":"active"}})),
            ("thread/acpSessionInfo/updated", json!({"threadId":"s","threadName":"Idle title","sessionInfo":{"updatedAt":"2026-10-21T00:00:00Z"}})),
            ("thread/config/updated", json!({"threadId":"s","configOptions":[]})),
            ("thread/acpModes/updated", json!({"threadId":"s","modes":{"currentModeId":"ask","availableModes":[]}})),
            ("turn/plan/updated", json!({"threadId":"s","turnId":"t","plan":[]})),
        ].into_iter().enumerate() {
            let event = fleetpane_runtime::Event { sequence: sequence as u64 + 1, message: json!({"method":method,"params":params}) };
            let applied = thread.apply(&event).unwrap();
            assert!(applied, "{method}");
            assert!(Row::turn(&thread.id, &thread.turns[0]) == before_turn, "{method}");
            assert!(Row::item("s", "t", &thread.turns[0].items[0]) == before_item, "{method}");
            assert!(!affects_rows(&event, applied), "metadata {method} rebuilt unchanged conversation rows");
        }
    }

    #[test]
    fn content_events_invalidate_rows_but_foreign_and_unapplied_events_do_not() {
        let mut thread: fleetpane_session::Thread = serde_json::from_value(json!({
            "id":"s","cwd":"/synthetic","preview":"","status":{"type":"active"},
            "turns":[{"id":"t","status":"inProgress","items":[{"id":"m","type":"agentMessage","text":"Before "}]}]
        })).unwrap();
        let event = fleetpane_runtime::Event {
            sequence: 1,
            message: json!({"method":"item/agentMessage/delta","params":{"threadId":"s","turnId":"t","itemId":"m","delta":"中文"}}),
        };
        let applied = thread.apply(&event).unwrap();
        assert!(affects_rows(&event, applied));
        assert_eq!(thread.turns[0].items[0].text().unwrap(), "Before 中文");
        let foreign = fleetpane_runtime::Event {
            sequence: 2,
            message: json!({"method":"thread/status/changed","params":{"threadId":"other","status":{"type":"active"}}}),
        };
        assert!(!affects_rows(&foreign, thread.apply(&foreign).unwrap()));
        // Newly supported non-metadata events conservatively invalidate; their
        // decoder still decides whether they were applied to this session.
        let future = fleetpane_runtime::Event {
            sequence: 3,
            message: json!({"method":"future/content"}),
        };
        assert!(affects_rows(&future, true));
        assert!(!affects_rows(&future, false));
    }

    fn row(id: usize) -> Row {
        Row {
            id: format!("thread:turn:{id}"),
            label: "Assistant".into(),
            content: Content::Markdown(format!("Synthetic message {id}")),
            copy: format!("Exact copy {id}"),
            error: None,
        }
    }

    #[test]
    fn streaming_tail_updates_one_card_without_touching_1200_retained_cards() {
        let mut before: Vec<_> = (0..=1200).map(row).collect();
        for tick in 0..200 {
            let mut after = before.clone();
            after[1200].content = Content::Markdown(format!("Streaming tick {tick}"));
            after[1200].copy = format!("Streaming tick {tick}");
            let changes = plan(&before, &after);
            assert_eq!(changes.retained.len(), 1201);
            assert_eq!(changes.updated, [1200]);
            before = after;
        }
        let changes = plan(&before, &before);
        assert!(changes.updated.is_empty());
    }

    #[test]
    fn pagination_reorder_edits_removal_and_thread_switch_use_identity() {
        let before: Vec<_> = (0..4).map(row).collect();
        let mut after = vec![row(9), row(3), row(1)];
        after[2].error = Some("Changed error".into());
        let changes = plan(&before, &after);
        assert_eq!(
            changes.retained,
            BTreeSet::from(["thread:turn:1", "thread:turn:3", "thread:turn:9"])
        );
        assert_eq!(changes.updated, [0, 2]);
        after[1].label = "New label".into();
        assert_eq!(plan(&before, &after).updated, [0, 1, 2]);
        after[1] = row(3);
        after[1].copy = "New exact copy".into();
        assert_eq!(plan(&before, &after).updated, [0, 1, 2]);
        for row in &mut after {
            row.id = format!("other:{}", row.id);
        }
        let changes = plan(&before, &after);
        assert!(changes.retained.iter().all(|id| id.starts_with("other:")));
        assert_eq!(changes.updated, [0, 1, 2]);
        let changes = plan(&after, &[]);
        assert!(changes.retained.is_empty());
        assert!(changes.updated.is_empty());
        let changes = plan(&[], &after);
        assert_eq!(changes.retained.len(), after.len());
        assert_eq!(changes.updated, [0, 1, 2]);
    }
}
