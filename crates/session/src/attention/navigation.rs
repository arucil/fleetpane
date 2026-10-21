//! A selection belongs to an exact observed request, never to its row number.
use super::{Item, Kind};
use serde_json::Value;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Default)]
pub struct Filter {
    pub socket: Option<PathBuf>,
    pub kind: Option<Kind>,
}

impl Filter {
    pub fn matches(&self, item: &Item) -> bool {
        self.socket
            .as_ref()
            .is_none_or(|socket| socket == &item.endpoint.socket)
            && self.kind.is_none_or(|kind| kind == item.kind)
    }
}

#[derive(Clone, Copy)]
pub enum Movement {
    Next,
    Previous,
    First,
    Last,
}

struct Identity {
    socket: PathBuf,
    host_id: Uuid,
    request: Value,
}

/// Refresh/filter changes can clear selection but cannot choose another request.
#[derive(Default)]
pub struct Cursor {
    selected: Option<Identity>,
}

impl Cursor {
    pub fn index(&self, items: &[Item], filter: &Filter) -> Option<usize> {
        let selected = self.selected.as_ref()?;
        items.iter().position(|item| {
            filter.matches(item)
                && item.thread_id.is_some()
                && item.endpoint.socket == selected.socket
                && item.host_id == selected.host_id
                && item.request == selected.request
        })
    }

    /// Returns true when a previously selected request became unavailable.
    pub fn reconcile(&mut self, items: &[Item], filter: &Filter) -> bool {
        if self.selected.is_some() && self.index(items, filter).is_none() {
            self.selected = None;
            return true;
        }
        false
    }

    pub fn select(&mut self, item: &Item) {
        self.selected = item.thread_id.as_ref().map(|_| Identity {
            socket: item.endpoint.socket.clone(),
            host_id: item.host_id,
            request: item.request.clone(),
        });
    }

    /// Explicit movement skips requests with no source session and stops at each end.
    pub fn navigate(&mut self, items: &[Item], filter: &Filter, movement: Movement) {
        let indices: Vec<_> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| filter.matches(item) && item.thread_id.is_some())
            .map(|(index, _)| index)
            .collect();
        let current = self
            .index(items, filter)
            .and_then(|selected| indices.iter().position(|index| *index == selected));
        let target = match movement {
            Movement::Next => current.map_or(0, |index| (index + 1).min(indices.len() - 1)),
            Movement::Previous => current.map_or(indices.len().saturating_sub(1), |index| {
                index.saturating_sub(1)
            }),
            Movement::First => 0,
            Movement::Last => indices.len().saturating_sub(1),
        };
        if let Some(index) = indices.get(target) {
            self.select(&items[*index]);
        } else {
            self.selected = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(host: &str, request: Value) -> Item {
        Item::from_request(
            super::super::Endpoint {
                name: host.into(),
                socket: format!("/tmp/{host}.sock").into(),
            },
            Uuid::new_v4(),
            request,
        )
        .unwrap()
    }

    fn rows() -> Vec<Item> {
        let request = json!({"id":10,"method":"session/request_permission","params":{"threadId":"same","toolCall":{"title":"Run 中文"}}});
        vec![
            item("a", request.clone()),
            item("b", request),
            item(
                "b",
                json!({"id":"10","method":"future/request","params":{}}),
            ),
            item(
                "a",
                json!({"id":11,"method":"item/tool/requestUserInput","params":{"threadId":"other"}}),
            ),
        ]
    }

    #[test]
    fn navigation_skips_unroutable_rows_and_handles_empty_and_end_boundaries() {
        let rows = rows();
        let filter = Filter::default();
        let mut cursor = Cursor::default();
        for (movement, expected) in [
            (Movement::Next, 0),
            (Movement::Next, 1),
            (Movement::Next, 3),
            (Movement::Next, 3),
            (Movement::Previous, 1),
            (Movement::First, 0),
            (Movement::Previous, 0),
            (Movement::Last, 3),
        ] {
            cursor.navigate(&rows, &filter, movement);
            assert_eq!(cursor.index(&rows, &filter), Some(expected));
        }
        for movement in [
            Movement::Next,
            Movement::Previous,
            Movement::First,
            Movement::Last,
        ] {
            cursor.navigate(&[], &filter, movement);
            assert_eq!(cursor.index(&[], &filter), None);
        }
        cursor.navigate(&rows[2..3], &filter, Movement::Next);
        assert_eq!(cursor.index(&rows[2..3], &filter), None);
    }

    #[test]
    fn refresh_keeps_exact_source_across_reorder_but_clears_changed_or_resolved_request() {
        let mut rows = rows();
        let filter = Filter::default();
        let mut cursor = Cursor::default();
        cursor.select(&rows[1]);
        rows.swap(0, 1);
        rows[0].endpoint.name = "Renamed Host".into();
        assert!(!cursor.reconcile(&rows, &filter));
        assert_eq!(cursor.index(&rows, &filter), Some(0));
        rows.remove(0);
        assert!(cursor.reconcile(&rows, &filter));
        assert_eq!(
            cursor.index(&rows, &filter),
            None,
            "same ID on other Host must not replace selection"
        );
        cursor.select(&rows[0]);
        rows[0].request["params"]["toolCall"]["title"] = json!("changed scope");
        assert!(cursor.reconcile(&rows, &filter));
        cursor.select(&rows[0]);
        rows[0].host_id = Uuid::new_v4();
        assert!(cursor.reconcile(&rows, &filter));
    }

    #[test]
    fn host_and_kind_filters_clear_hidden_selection_without_retargeting() {
        let rows = rows();
        let filter = Filter {
            socket: Some(rows[1].endpoint.socket.clone()),
            kind: Some(Kind::Approval),
        };
        let mut cursor = Cursor::default();
        cursor.select(&rows[0]);
        assert!(cursor.reconcile(&rows, &filter));
        cursor.navigate(&rows, &filter, Movement::Next);
        assert_eq!(cursor.index(&rows, &filter), Some(1));
        let questions = Filter {
            kind: Some(Kind::Question),
            ..Filter::default()
        };
        assert!(cursor.reconcile(&rows, &questions));
        cursor.navigate(&rows, &questions, Movement::Last);
        assert_eq!(cursor.index(&rows, &questions), Some(3));
    }
}
