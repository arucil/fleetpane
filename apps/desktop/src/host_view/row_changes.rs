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
                    | "thread/settings/updated"
                    | "turn/plan/updated"
            )
        )
}

/// Content targets are collected only after the session decoder accepts an event.
/// Structural/unknown events use full reconciliation; metadata has no targets.
#[derive(Default)]
pub(super) enum Invalidation {
    #[default]
    None,
    Items(BTreeSet<(String, String)>),
    All,
}

impl Invalidation {
    pub fn record(&mut self, event: &fleetpane_runtime::Event, applied: bool) {
        if !affects_rows(event, applied) || matches!(self, Self::All) {
            return;
        }
        let method = event
            .message
            .get("method")
            .and_then(serde_json::Value::as_str);
        let item_path = match method {
            Some("item/started" | "item/completed") => "/params/item/id",
            Some(
                "item/agentMessage/delta"
                | "item/reasoning/summaryPartAdded"
                | "item/reasoning/summaryTextDelta"
                | "item/reasoning/textDelta",
            ) => "/params/itemId",
            _ => {
                *self = Self::All;
                return;
            }
        };
        let (Some(turn), Some(item)) = (
            event
                .message
                .pointer("/params/turnId")
                .and_then(serde_json::Value::as_str),
            event
                .message
                .pointer(item_path)
                .and_then(serde_json::Value::as_str),
        ) else {
            *self = Self::All;
            return;
        };
        if matches!(self, Self::None) {
            *self = Self::Items(BTreeSet::new());
        }
        if let Self::Items(items) = self {
            items.insert((turn.to_owned(), item.to_owned()));
        }
    }

    /// None asks the caller to reconcile structural changes. The sparse path
    /// never constructs unrelated content, raw Copy payloads or Markdown.
    pub fn project(
        &self,
        thread: &fleetpane_session::Thread,
        rows: &[Row],
        index: &BTreeMap<String, usize>,
    ) -> Option<Vec<(usize, Row)>> {
        self.project_with(thread, rows, index, Row::item)
    }

    fn project_with(
        &self,
        thread: &fleetpane_session::Thread,
        rows: &[Row],
        index: &BTreeMap<String, usize>,
        mut project: impl FnMut(&str, &str, &fleetpane_session::Item) -> Option<Row>,
    ) -> Option<Vec<(usize, Row)>> {
        let targets = match self {
            Self::None => return Some(Vec::new()),
            Self::All => return None,
            Self::Items(items) => items,
        };
        let mut updates = Vec::with_capacity(targets.len());
        for (turn_id, item_id) in targets {
            let item = thread
                .turns
                .iter()
                .find(|turn| &turn.id == turn_id)?
                .items
                .iter()
                .find(|item| &item.id == item_id)?;
            let key = Row::item_key(&thread.id, turn_id, item_id);
            let position = index.get(&key).copied();
            match (position, project(&thread.id, turn_id, item)) {
                (Some(position), Some(row)) => {
                    if rows.get(position)? != &row {
                        updates.push((position, row));
                    }
                }
                (None, None) => {}
                // A newly visible/hidden row changes layout and the identity index.
                _ => return None,
            }
        }
        Some(updates)
    }
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
            ("thread/settings/updated", json!({"threadId":"s","threadSettings":{"model":"confirmed","modelProvider":"synthetic","effort":null}})),
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

    fn source(items: Vec<serde_json::Value>) -> fleetpane_session::Thread {
        serde_json::from_value(json!({"id":"s:中文","cwd":"/synthetic","preview":"",
            "status":{"type":"active"},"turns":[{"id":"t:中文","status":"inProgress","items":items}]})).unwrap()
    }

    fn index(rows: &[Row]) -> BTreeMap<String, usize> {
        rows.iter()
            .enumerate()
            .map(|(i, row)| (row.id.clone(), i))
            .collect()
    }

    fn event(method: &str, fields: serde_json::Value) -> fleetpane_runtime::Event {
        let mut params = json!({"threadId":"s:中文","turnId":"t:中文"});
        params
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        fleetpane_runtime::Event {
            sequence: 1,
            message: json!({"method":method,"params":params}),
        }
    }

    fn assert_projection(
        thread: &mut fleetpane_session::Thread,
        rows: &mut Vec<Row>,
        event: &fleetpane_runtime::Event,
    ) -> bool {
        let mut dirty = Invalidation::default();
        dirty.record(event, thread.apply(event).unwrap());
        let sparse = dirty.project(thread, rows, &index(rows));
        let incremental = sparse.is_some();
        match sparse {
            Some(updates) => {
                for (i, row) in updates {
                    rows[i] = row;
                }
            }
            None => *rows = Row::conversation(thread),
        }
        assert!(*rows == Row::conversation(thread));
        incremental
    }

    #[test]
    fn streaming_projects_only_the_target_once_per_poll_without_serializing_399_other_items() {
        let mut thread = source((0..400).map(|i| json!({"id":format!("m:{i}"),"type":"agentMessage","text":format!("Retained **中文** {i}")})).collect());
        let mut rows = Row::conversation(&thread);
        let indices = index(&rows);
        let retained: Vec<_> = rows[..400].iter().map(|row| row.copy.as_ptr()).collect();
        for tick in 0..40 {
            let mut dirty = Invalidation::default();
            for part in [" 中文", " **tail**"] {
                let update = event(
                    "item/agentMessage/delta",
                    json!({"itemId":"m:399","delta":part}),
                );
                dirty.record(&update, thread.apply(&update).unwrap());
            }
            let mut projected = 0;
            let updates = dirty
                .project_with(&thread, &rows, &indices, |s, t, i| {
                    projected += 1;
                    Row::item(s, t, i)
                })
                .unwrap();
            assert_eq!(
                projected, 1,
                "tick {tick}: unrelated source items reprojected"
            );
            assert_eq!(updates.len(), 1);
            assert_eq!(updates[0].0, 400);
            for (i, row) in updates {
                rows[i] = row;
            }
            assert!(rows == Row::conversation(&thread));
            assert_eq!(
                rows[..400]
                    .iter()
                    .map(|row| row.copy.as_ptr())
                    .collect::<Vec<_>>(),
                retained
            );
        }
    }

    #[test]
    fn visibility_lifecycle_and_failed_tool_updates_match_full_projection() {
        let mut thread = source(vec![
            json!({"id":"r","type":"reasoning","summary":[],"content":[]}),
            json!({"id":"tool","type":"acpToolCall","call":{"title":"Read settings","status":"pending","rawOutput":null}}),
        ]);
        let mut rows = Row::conversation(&thread);
        // Empty reasoning has no row; becoming visible and becoming hidden
        // must reconcile layout, not silently discard the change.
        assert!(assert_projection(
            &mut thread,
            &mut rows,
            &event(
                "item/reasoning/summaryPartAdded",
                json!({"itemId":"r","summaryIndex":0})
            )
        ));
        assert!(!assert_projection(
            &mut thread,
            &mut rows,
            &event(
                "item/reasoning/summaryTextDelta",
                json!({"itemId":"r","summaryIndex":0,"delta":"Thinking 中文"})
            )
        ));
        assert!(assert_projection(
            &mut thread,
            &mut rows,
            &event(
                "item/reasoning/textDelta",
                json!({"itemId":"r","contentIndex":0,"delta":"detail"})
            )
        ));
        assert!(assert_projection(
            &mut thread,
            &mut rows,
            &event(
                "item/completed",
                json!({"item":{"id":"tool","type":"acpToolCall","call":{"title":"Read failed","status":"failed","rawOutput":false}}})
            )
        ));
        let copied: serde_json::Value = serde_json::from_str(&rows.last().unwrap().copy).unwrap();
        assert_eq!(copied["call"]["rawOutput"], false);
        assert!(!assert_projection(
            &mut thread,
            &mut rows,
            &event(
                "item/completed",
                json!({"item":{"id":"r","type":"reasoning","summary":[],"content":[]}})
            )
        ));
        assert!(!assert_projection(
            &mut thread,
            &mut rows,
            &event(
                "item/started",
                json!({"item":{"id":"new","type":"agentMessage","text":"New visible row"}})
            )
        ));
        // Malformed known content remains an inspectable error card.
        assert!(assert_projection(
            &mut thread,
            &mut rows,
            &event(
                "item/completed",
                json!({"item":{"id":"tool","type":"commandExecution","command":4}})
            )
        ));
        assert!(rows[1].error.is_some());
    }

    #[test]
    fn metadata_foreign_pages_structural_and_future_events_keep_their_boundaries() {
        let mut thread = source(vec![
            json!({"id":"m","type":"agentMessage","text":"Retained"}),
        ]);
        let rows = Row::conversation(&thread);
        let indices = index(&rows);
        let mut dirty = Invalidation::default();
        let metadata = event("thread/name/updated", json!({"threadName":"New title"}));
        dirty.record(&metadata, thread.apply(&metadata).unwrap());
        let mut foreign = event(
            "item/agentMessage/delta",
            json!({"itemId":"m","delta":"Ignored"}),
        );
        foreign.message["params"]["threadId"] = json!("other");
        dirty.record(&foreign, thread.apply(&foreign).unwrap());
        // History can reject an otherwise valid event behind a page watermark.
        dirty.record(
            &event("item/completed", json!({"item":{"id":"not-loaded"}})),
            false,
        );
        assert!(
            dirty
                .project_with(&thread, &rows, &indices, |_, _, _| panic!(
                    "metadata projected content"
                ))
                .unwrap()
                .is_empty()
        );
        let completion = event(
            "turn/completed",
            json!({"turn":{"id":"t:中文","status":"completed","items":[{"id":"m","type":"agentMessage","text":"Final"}]}}),
        );
        dirty.record(&completion, thread.apply(&completion).unwrap());
        assert!(dirty.project(&thread, &rows, &indices).is_none());
        let mut future = Invalidation::default();
        future.record(&event("future/content", json!({})), true);
        assert!(future.project(&thread, &rows, &indices).is_none());
    }

    #[test]
    fn item_targets_keep_opaque_turn_and_item_boundaries_and_stable_order() {
        let mut thread: fleetpane_session::Thread = serde_json::from_value(json!({
            "id":"s:中文","cwd":"/synthetic","preview":"","status":{"type":"active"},"turns":[
                {"id":"a:b","status":"inProgress","items":[{"id":"c","type":"agentMessage","text":"First"}]},
                {"id":"a","status":"inProgress","items":[{"id":"b:c","type":"agentMessage","text":"Second"}]}
            ]})).unwrap();
        let mut rows = Row::conversation(&thread);
        let mut dirty = Invalidation::default();
        for (turn, item, delta) in [("a:b", "c", " 中文"), ("a", "b:c", " exact")] {
            let e = event(
                "item/agentMessage/delta",
                json!({"turnId":turn,"itemId":item,"delta":delta}),
            );
            dirty.record(&e, thread.apply(&e).unwrap());
        }
        let updates = dirty.project(&thread, &rows, &index(&rows)).unwrap();
        assert_eq!(updates.len(), 2);
        for (i, row) in updates {
            rows[i] = row;
        }
        assert!(rows == Row::conversation(&thread));
        assert_eq!(rows[1].copy, "First 中文");
        assert_eq!(rows[3].copy, "Second exact");
    }

    fn row(id: usize) -> Row {
        Row {
            id: format!("thread:turn:{id}"),
            label: "Assistant".into(),
            content: Content::Markdown(format!("Synthetic message {id}")),
            copy: format!("Exact copy {id}"),
            error: None,
            answer_turn: None,
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
