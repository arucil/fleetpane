//! Demand-driven history with per-snapshot event boundaries.
use crate::{Error, Item, Thread, Turn};
use fleetpane_runtime::Event;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const PAGE_ITEMS: usize = 16;
const MAX_CURSOR_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Turns {
        thread_id: String,
        cursor: Option<String>,
    },
    Items {
        thread_id: String,
        turn_id: String,
        cursor: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread() -> Thread {
        serde_json::from_value(json!({"id":"t","cwd":"/synthetic","preview":"","historyMode":"paginated","status":{"type":"idle"},"turns":[]})).unwrap()
    }

    fn item(id: &str, text: &str) -> Value {
        json!({"id":id,"type":"agentMessage","text":text})
    }
    fn turn(id: &str) -> Value {
        json!({"id":id,"status":"completed","items":[],"error":null})
    }
    fn event(sequence: u64, method: &str, params: Value) -> Event {
        Event {
            sequence,
            message: json!({"method":method,"params":params}),
        }
    }
    fn apply(history: &mut History, thread: &mut Thread, result: Value, sequence: u64) {
        let request = history.next().unwrap();
        history.apply(thread, &request, result, sequence).unwrap();
    }

    #[test]
    fn older_pages_prepend_in_order_and_failed_pages_leave_continuation_usable() {
        let mut thread = thread();
        let mut history = History::new(thread.id.clone());
        apply(
            &mut history,
            &mut thread,
            json!({"data":[turn("recent")],"nextCursor":"older-turn"}),
            2,
        );
        apply(
            &mut history,
            &mut thread,
            json!({"data":[{"turnId":"recent","item":item("b","B")}],"nextCursor":"older-item"}),
            3,
        );
        let before = serde_json::to_value(&thread).unwrap();
        let request = history.next().unwrap();
        assert!(
            history
                .apply(
                    &mut thread,
                    &request,
                    json!({"data":[{"turnId":"wrong","item":item("a","A")}],"nextCursor":null}),
                    4
                )
                .is_err()
        );
        assert_eq!(serde_json::to_value(&thread).unwrap(), before);
        assert_eq!(history.next(), Some(request));
        apply(
            &mut history,
            &mut thread,
            json!({"data":[{"turnId":"recent","item":item("a","A")}],"nextCursor":null}),
            5,
        );
        apply(
            &mut history,
            &mut thread,
            json!({"data":[turn("old")],"nextCursor":null}),
            6,
        );
        // Opaque item cursors may be identical in different turn scopes.
        apply(
            &mut history,
            &mut thread,
            json!({"data":[{"turnId":"old","item":item("d","D")}],"nextCursor":"older-item"}),
            7,
        );
        apply(
            &mut history,
            &mut thread,
            json!({"data":[{"turnId":"old","item":item("c","C")}],"nextCursor":null}),
            8,
        );
        assert!(history.next().is_none());
        assert_eq!(
            thread
                .turns
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            ["old", "recent"]
        );
        assert_eq!(
            thread.turns[1]
                .items
                .iter()
                .map(|i| i.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }

    #[test]
    fn page_watermarks_prevent_duplicate_deltas_but_preserve_new_turns_and_later_output() {
        let mut thread = thread();
        let mut history = History::new(thread.id.clone());
        apply(
            &mut history,
            &mut thread,
            json!({"data":[turn("recent")],"nextCursor":null}),
            3,
        );
        apply(
            &mut history,
            &mut thread,
            json!({"data":[{"turnId":"recent","item":item("a","Hello")}],"nextCursor":"older"}),
            10,
        );
        for e in [
            event(
                4,
                "item/started",
                json!({"threadId":"t","turnId":"recent","item":item("a","")}),
            ),
            event(
                5,
                "item/agentMessage/delta",
                json!({"threadId":"t","turnId":"recent","itemId":"a","delta":"Hello"}),
            ),
            event(
                6,
                "turn/started",
                json!({"threadId":"t","turn":turn("new")}),
            ),
            event(
                7,
                "item/started",
                json!({"threadId":"t","turnId":"new","item":item("new-msg","New")}),
            ),
        ] {
            history.apply_event(&mut thread, &e).unwrap();
        }
        assert_eq!(thread.turns[0].items[0].text().unwrap(), "Hello");
        assert_eq!(thread.turns[1].items[0].text().unwrap(), "New");
        // Loading an older item must not suppress an unobserved update to a recent item.
        apply(
            &mut history,
            &mut thread,
            json!({"data":[{"turnId":"recent","item":item("older","Before")}],"nextCursor":null}),
            20,
        );
        history
            .apply_event(
                &mut thread,
                &event(
                    15,
                    "item/agentMessage/delta",
                    json!({"threadId":"t","turnId":"recent","itemId":"a","delta":" world"}),
                ),
            )
            .unwrap();
        assert_eq!(thread.turns[0].items[1].text().unwrap(), "Hello world");
        // Completion's embedded items must obey the same per-item snapshot boundary.
        history.apply_event(&mut thread,&event(9,"turn/completed",json!({"threadId":"t","turn":{"id":"recent","status":"completed","items":[item("a","stale"),item("older","stale")],"error":null}}))).unwrap();
        assert_eq!(thread.turns[0].items[1].text().unwrap(), "Hello world");
        assert_eq!(thread.turns[0].items[0].text().unwrap(), "Before");
    }

    #[test]
    fn replay_does_not_append_omitted_older_items_after_the_recent_page() {
        let mut thread = thread();
        let mut history = History::new(thread.id.clone());
        apply(
            &mut history,
            &mut thread,
            json!({"data":[turn("recent")],"nextCursor":null}),
            3,
        );
        apply(
            &mut history,
            &mut thread,
            json!({"data":[{"turnId":"recent","item":item("last","Last")}],"nextCursor":"older"}),
            10,
        );
        history
            .apply_event(
                &mut thread,
                &event(
                    4,
                    "item/started",
                    json!({"threadId":"t","turnId":"recent","item":item("omitted", "Earlier")}),
                ),
            )
            .unwrap();
        history.apply_event(&mut thread, &event(9, "turn/completed", json!({"threadId":"t","turn":{"id":"recent","status":"completed","items":[item("omitted","Earlier"),item("last","Last")],"error":null}}))).unwrap();
        assert_eq!(
            thread.turns[0]
                .items
                .iter()
                .map(|i| i.id.as_str())
                .collect::<Vec<_>>(),
            ["last"]
        );
        apply(
            &mut history,
            &mut thread,
            json!({"data":[{"turnId":"recent","item":item("omitted","Earlier")}],"nextCursor":null}),
            12,
        );
        assert_eq!(
            thread.turns[0]
                .items
                .iter()
                .map(|i| i.id.as_str())
                .collect::<Vec<_>>(),
            ["omitted", "last"]
        );
    }

    #[test]
    fn acp_item_replacements_obey_page_watermarks_without_reverting_later_content() {
        let mut thread = thread();
        thread.history_mode = Some("acp-observed".into());
        thread.history_paging = true;
        assert!(thread.supports_history_pages());
        let mut history = History::new(thread.id.clone());
        let message = |text| json!({"id":"message","type":"acpMessage","content":[{"type":"text","text":text}]});
        apply(
            &mut history,
            &mut thread,
            json!({"data":[turn("recent")],"nextCursor":null}),
            2,
        );
        apply(
            &mut history,
            &mut thread,
            json!({"data":[{"turnId":"recent","item":message("Alpha Beta")}],"nextCursor":"older"}),
            10,
        );
        for (sequence, text) in [(9, "Alpha"), (12, "Alpha Beta Gamma")] {
            history
                .apply_event(
                    &mut thread,
                    &event(
                        sequence,
                        "item/completed",
                        json!({"threadId":"t","turnId":"recent","item":message(text)}),
                    ),
                )
                .unwrap();
        }
        apply(
            &mut history,
            &mut thread,
            json!({"data":[{"turnId":"recent","item":item("older","before")}],"nextCursor":null}),
            20,
        );
        history.apply_event(&mut thread,&event(15,"item/completed",json!({"threadId":"t","turnId":"recent","item":message("Alpha Beta Gamma Delta")}))).unwrap();
        assert_eq!(thread.turns[0].items.len(), 2);
        assert_eq!(
            thread.turns[0].items[1].text().unwrap(),
            "Alpha Beta Gamma Delta"
        );
    }

    #[test]
    fn repeated_cursors_duplicate_items_and_oversize_pages_are_atomic_errors() {
        let mut thread = thread();
        let mut history = History::new(thread.id.clone());
        apply(
            &mut history,
            &mut thread,
            json!({"data":[turn("recent")],"nextCursor":null}),
            2,
        );
        apply(
            &mut history,
            &mut thread,
            json!({"data":[{"turnId":"recent","item":item("a","A")}],"nextCursor":"repeat"}),
            3,
        );
        let request = history.next().unwrap();
        for page in [
            json!({"data":[{"turnId":"recent","item":item("b","B")}],"nextCursor":"repeat"}),
            json!({"data":[{"turnId":"recent","item":item("a","duplicate")}],"nextCursor":null}),
            json!({"data":[],"nextCursor":"empty"}),
            json!({"data":(0..PAGE_ITEMS+1).map(|i|json!({"turnId":"recent","item":item(&format!("item-{i}"),"text")})).collect::<Vec<_>>(),"nextCursor":null}),
        ] {
            let before = serde_json::to_value(&thread).unwrap();
            assert!(history.apply(&mut thread, &request, page, 4).is_err());
            assert_eq!(serde_json::to_value(&thread).unwrap(), before);
            assert_eq!(history.next(), Some(request.clone()));
        }
    }

    #[test]
    fn reaching_the_retained_history_bound_keeps_existing_data_and_stops_loading() {
        let mut thread = thread();
        thread.turns.push(serde_json::from_value(json!({"id":"loaded","status":"completed","items":(0..crate::MAX_ITEMS).map(|i|item(&format!("i-{i}"),"text")).collect::<Vec<_>>()})).unwrap());
        let mut history = History::new(thread.id.clone());
        apply(
            &mut history,
            &mut thread,
            json!({"data":[turn("old")],"nextCursor":null}),
            1,
        );
        let before = serde_json::to_value(&thread).unwrap();
        let request = history.next().unwrap();
        assert!(history.apply(&mut thread,&request,json!({"data":[{"turnId":"old","item":item("extra","text")}],"nextCursor":null}),2).is_err());
        assert!(history.at_limit && history.next().is_none());
        assert_eq!(serde_json::to_value(&thread).unwrap(), before);
    }
}

impl Request {
    pub fn rpc(&self) -> Result<(&'static str, Value), Error> {
        let (thread_id, cursor) = match self {
            Self::Turns { thread_id, cursor }
            | Self::Items {
                thread_id, cursor, ..
            } => (thread_id, cursor),
        };
        if thread_id.is_empty()
            || cursor
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.len() > MAX_CURSOR_BYTES)
        {
            return Err(Error::Projection(
                "invalid history identity or cursor".into(),
            ));
        }
        Ok(match self {
            Self::Turns { thread_id, cursor } => (
                "thread/turns/list",
                json!({"threadId":thread_id,"cursor":cursor,"limit":1,"sortDirection":"desc","itemsView":"notLoaded"}),
            ),
            Self::Items {
                thread_id,
                turn_id,
                cursor,
            } => {
                if turn_id.is_empty() {
                    return Err(Error::Projection("history turn ID is empty".into()));
                }
                (
                    "thread/items/list",
                    json!({"threadId":thread_id,"turnId":turn_id,"cursor":cursor,"limit":PAGE_ITEMS,"sortDirection":"desc"}),
                )
            }
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page<T> {
    data: Vec<T>,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    turn_id: String,
    item: Item,
}

#[derive(Clone)]
pub struct History {
    next: Option<Request>,
    older_turn: Option<String>,
    seen_turns: BTreeSet<String>,
    seen_cursors: BTreeSet<(String, String)>,
    turn_boundaries: BTreeMap<String, u64>,
    tail_boundaries: BTreeMap<String, u64>,
    item_boundaries: BTreeMap<(String, String), u64>,
    pub at_limit: bool,
}

impl History {
    pub fn new(thread_id: String) -> Self {
        Self {
            next: Some(Request::Turns {
                thread_id,
                cursor: None,
            }),
            older_turn: None,
            seen_turns: BTreeSet::new(),
            seen_cursors: BTreeSet::new(),
            turn_boundaries: BTreeMap::new(),
            tail_boundaries: BTreeMap::new(),
            item_boundaries: BTreeMap::new(),
            at_limit: false,
        }
    }

    pub fn next(&self) -> Option<Request> {
        if self.at_limit {
            None
        } else {
            self.next.clone()
        }
    }

    /// Validate a whole page before changing either history or its continuation.
    pub fn apply(
        &mut self,
        thread: &mut Thread,
        request: &Request,
        result: Value,
        sequence: u64,
    ) -> Result<(), Error> {
        if self.next.as_ref() != Some(request) {
            return Err(Error::Projection(
                "history reply does not match its continuation".into(),
            ));
        }
        let mut history = self.clone();
        let mut candidate = thread.clone();
        history.apply_page(&mut candidate, request, result, sequence)?;
        if let Err(error) = candidate.validate() {
            self.at_limit = true;
            return Err(error);
        }
        *self = history;
        *thread = candidate;
        Ok(())
    }

    fn continuation(&mut self, scope: &str, cursor: &Option<String>) -> Result<(), Error> {
        if let Some(cursor) = cursor
            && (cursor.is_empty()
                || cursor.len() > MAX_CURSOR_BYTES
                || !self.seen_cursors.insert((scope.into(), cursor.clone())))
        {
            return Err(Error::Projection(
                "history cursor is empty, oversized or repeated".into(),
            ));
        }
        Ok(())
    }

    fn apply_page(
        &mut self,
        thread: &mut Thread,
        request: &Request,
        result: Value,
        sequence: u64,
    ) -> Result<(), Error> {
        match request {
            Request::Turns { thread_id, .. } => {
                let mut page: Page<Turn> = serde_json::from_value(result)?;
                if thread_id != &thread.id
                    || page.data.len() > 1
                    || (page.data.is_empty() && page.next_cursor.is_some())
                {
                    return Err(Error::Projection("invalid history turn page".into()));
                }
                self.continuation("turns", &page.next_cursor)?;
                self.older_turn = page.next_cursor;
                if let Some(mut turn) = page.data.pop() {
                    if turn.id.is_empty()
                        || !turn.items.is_empty()
                        || !self.seen_turns.insert(turn.id.clone())
                    {
                        return Err(Error::Projection(
                            "history turn page must contain one new metadata-only turn".into(),
                        ));
                    }
                    let turn_id = turn.id.clone();
                    self.turn_boundaries.insert(turn_id.clone(), sequence);
                    if let Some(old) = thread.turns.iter_mut().find(|t| t.id == turn_id) {
                        turn.items = old.items.clone();
                        *old = turn;
                    } else {
                        thread.turns.insert(0, turn);
                    }
                    self.next = Some(Request::Items {
                        thread_id: thread_id.clone(),
                        turn_id,
                        cursor: None,
                    });
                } else {
                    self.next = None;
                }
            }
            Request::Items {
                thread_id,
                turn_id,
                cursor,
            } => {
                let page: Page<Entry> = serde_json::from_value(result)?;
                if thread_id != &thread.id
                    || page.data.len() > PAGE_ITEMS
                    || (page.data.is_empty() && page.next_cursor.is_some())
                {
                    return Err(Error::Projection("invalid history item page".into()));
                }
                self.continuation(&format!("items:{turn_id}"), &page.next_cursor)?;
                let turn = thread
                    .turns
                    .iter_mut()
                    .find(|t| &t.id == turn_id)
                    .ok_or_else(|| {
                        Error::Projection("history items have no loaded turn metadata".into())
                    })?;
                let mut ids = BTreeSet::new();
                let mut items = Vec::new();
                if cursor.is_none() {
                    // The first item page defines this turn's visible tail. Older
                    // events in the read window belong behind its continuation,
                    // not after the recent items already in the snapshot.
                    self.tail_boundaries.insert(turn_id.clone(), sequence);
                }
                for entry in page.data.into_iter().rev() {
                    let key = (turn_id.clone(), entry.item.id.clone());
                    if &entry.turn_id != turn_id
                        || entry.item.id.is_empty()
                        || !ids.insert(entry.item.id.clone())
                        || self.item_boundaries.contains_key(&key)
                    {
                        return Err(Error::Projection(
                            "history page has a wrong turn or duplicate item".into(),
                        ));
                    }
                    self.item_boundaries.insert(key, sequence);
                    items.push(entry.item);
                }
                turn.items.retain(|item| !ids.contains(&item.id));
                items.append(&mut turn.items);
                turn.items = items;
                self.next = page
                    .next_cursor
                    .map(|cursor| Request::Items {
                        thread_id: thread_id.clone(),
                        turn_id: turn_id.clone(),
                        cursor: Some(cursor),
                    })
                    .or_else(|| {
                        self.older_turn.clone().map(|cursor| Request::Turns {
                            thread_id: thread_id.clone(),
                            cursor: Some(cursor),
                        })
                    });
            }
        }
        Ok(())
    }

    /// Replay events after the metadata read, skipping only fields covered by a
    /// later page snapshot. Events from a new turn during hydration still apply.
    pub fn apply_event(&self, thread: &mut Thread, event: &Event) -> Result<bool, Error> {
        if event
            .message
            .pointer("/params/threadId")
            .and_then(Value::as_str)
            .is_some_and(|id| id != thread.id)
        {
            return Ok(false);
        }
        let method = event.message.get("method").and_then(Value::as_str);
        if matches!(method, Some("turn/started" | "turn/completed")) {
            if let Some(turn_id) = event
                .message
                .pointer("/params/turn/id")
                .and_then(Value::as_str)
            {
                if self
                    .turn_boundaries
                    .get(turn_id)
                    .is_some_and(|seq| event.sequence <= *seq)
                {
                    return Ok(false);
                }
                let mut filtered = event.clone();
                if let Some(items) = filtered
                    .message
                    .pointer_mut("/params/turn/items")
                    .and_then(Value::as_array_mut)
                {
                    items.retain(|item| {
                        !self.covered(
                            turn_id,
                            item.get("id").and_then(Value::as_str),
                            event.sequence,
                        )
                    });
                }
                return thread.apply(&filtered);
            }
        } else if matches!(
            method,
            Some("item/started" | "item/completed" | "item/agentMessage/delta")
        ) && let Some(turn_id) = event
            .message
            .pointer("/params/turnId")
            .and_then(Value::as_str)
        {
            let item_id = if method == Some("item/agentMessage/delta") {
                event.message.pointer("/params/itemId")
            } else {
                event.message.pointer("/params/item/id")
            };
            if self.covered(turn_id, item_id.and_then(Value::as_str), event.sequence) {
                return Ok(false);
            }
        }
        thread.apply(event)
    }

    fn covered(&self, turn_id: &str, item_id: Option<&str>, sequence: u64) -> bool {
        item_id.is_some_and(|id| {
            self.tail_boundaries
                .get(turn_id)
                .is_some_and(|seq| sequence <= *seq)
                || self
                    .item_boundaries
                    .get(&(turn_id.into(), id.into()))
                    .is_some_and(|seq| sequence <= *seq)
        })
    }
}
