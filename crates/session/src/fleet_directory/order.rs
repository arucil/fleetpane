//! Ordering is over loaded metadata only, with exact source indices retained.
use super::Group;
use crate::Thread;
use chrono::{DateTime, SecondsFormat, Utc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Order {
    #[default]
    Activity,
    Host,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub group: usize,
    /// None denotes a Host summary, including offline and empty Hosts.
    pub thread: Option<usize>,
    pub updated: String,
}

fn update_time(thread: &Thread) -> (Option<DateTime<Utc>>, String) {
    if let Some(info) = &thread.acp_session_info {
        return match &info.updated_at {
            Some(raw) => match DateTime::parse_from_rfc3339(raw) {
                Ok(value) => (Some(value.to_utc()), format!("Updated {raw}")),
                Err(_) => (None, format!("Invalid update time: {raw}")),
            },
            None => (None, "Update time not supplied".into()),
        };
    }
    match thread.updated_at {
        Some(seconds) => match DateTime::from_timestamp(seconds, 0) {
            Some(value) => (
                Some(value),
                format!(
                    "Updated {}",
                    value.to_rfc3339_opts(SecondsFormat::Secs, true)
                ),
            ),
            None => (None, format!("Invalid update time: {seconds}")),
        },
        None => (None, "Update time not supplied".into()),
    }
}

/// Build once when metadata, filter or sort changes, rather than on each paint.
/// Stale rows stay visible after fresh rows, but cannot be opened by Group::source.
pub fn rows(groups: &[Group], filter: Option<usize>, order: Order) -> Vec<Row> {
    let mut summaries = Vec::new();
    let mut sessions = Vec::new();
    let mut grouped = Vec::new();
    for (index, group) in groups.iter().enumerate() {
        if filter.is_some_and(|selected| selected != index) {
            continue;
        }
        let summary = Row {
            group: index,
            thread: None,
            updated: String::new(),
        };
        summaries.push(summary.clone());
        if order == Order::Host {
            grouped.push(summary);
        }
        for (entry, thread) in group.directory.threads.iter().enumerate() {
            let (at, updated) = update_time(thread);
            let row = Row {
                group: index,
                thread: Some(entry),
                updated,
            };
            if order == Order::Host {
                grouped.push(row);
            } else {
                sessions.push((row, !group.can_open(), thread.status.kind != "active", at));
            }
        }
    }
    if order == Order::Host {
        return grouped;
    }
    sessions.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| b.3.cmp(&a.3))
            .then_with(|| {
                groups[a.0.group]
                    .endpoint
                    .socket
                    .cmp(&groups[b.0.group].endpoint.socket)
            })
            .then_with(|| {
                let left = &groups[a.0.group].directory.threads[a.0.thread.expect("session row")];
                let right = &groups[b.0.group].directory.threads[b.0.thread.expect("session row")];
                left.id.cmp(&right.id)
            })
    });
    let mut rows: Vec<_> = sessions.into_iter().map(|(row, _, _, _)| row).collect();
    rows.extend(summaries);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{directory::PageRequest, fleet_directory::Observation, hosts::Endpoint};
    use serde_json::json;
    use uuid::Uuid;

    fn group(name: &str, data: serde_json::Value) -> Group {
        let mut g = Group::new(Endpoint {
            name: name.into(),
            socket: format!("/synthetic/{name}.sock").into(),
        });
        let page = serde_json::from_value(json!({"data":data,"nextCursor":null})).unwrap();
        g.apply(
            &PageRequest::default(),
            Ok(Observation {
                host_id: Uuid::new_v4(),
                page,
            }),
        );
        g
    }
    fn thread(id: &str, active: bool, at: Option<i64>) -> serde_json::Value {
        json!({"id":id,"cwd":"/synthetic","preview":id,"status":{"type":if active {"active"} else {"idle"}},"updatedAt":at,"turns":[]})
    }
    fn ids(groups: &[Group], order: Order) -> Vec<(String, String)> {
        rows(groups, None, order)
            .into_iter()
            .filter_map(|r| {
                r.thread.map(|t| {
                    (
                        groups[r.group].endpoint.name.clone(),
                        groups[r.group].directory.threads[t].id.clone(),
                    )
                })
            })
            .collect()
    }
    #[test]
    fn global_activity_order_keeps_colliding_ids_and_does_not_invent_times() {
        let groups = vec![
            group(
                "a",
                json!([
                    thread("same", false, Some(900)),
                    thread("unknown", false, None)
                ]),
            ),
            group(
                "b",
                json!([
                    thread("same", true, Some(1)),
                    thread("recent", false, Some(1000))
                ]),
            ),
        ];
        assert_eq!(
            ids(&groups, Order::Activity),
            vec![
                ("b".into(), "same".into()),
                ("b".into(), "recent".into()),
                ("a".into(), "same".into()),
                ("a".into(), "unknown".into())
            ]
        );
        assert_eq!(ids(&groups, Order::Host)[0], ("a".into(), "same".into()));
        assert!(
            rows(&groups, Some(1), Order::Activity)
                .iter()
                .all(|r| r.group == 1)
        );
        assert!(
            rows(&groups, None, Order::Activity)[3]
                .updated
                .contains("not supplied")
        );
    }
    #[test]
    fn stale_active_rows_sort_last_and_reorder_never_changes_exact_sources() {
        let mut groups = vec![
            group("a", json!([thread("same", true, Some(9000))])),
            group("b", json!([thread("same", false, Some(1))])),
        ];
        groups[0].apply(
            &PageRequest::default(),
            Err(super::super::Error::Transport("offline".into())),
        );
        let ordered = ids(&groups, Order::Activity);
        assert_eq!(ordered[0], ("b".into(), "same".into()));
        assert!(groups[0].source("same").is_err());
        assert_eq!(
            groups[1].source("same").unwrap().host_id,
            groups[1].host_id.unwrap()
        );
        groups.reverse();
        assert_eq!(ids(&groups, Order::Activity), ordered);
    }
    #[test]
    fn timestamps_normalize_acp_offsets_keep_subseconds_and_report_invalid_data() {
        let mut acp = thread("acp", false, None);
        acp["acpSessionInfo"] = json!({"updatedAt":"1970-01-01T01:00:01.500+01:00"});
        let mut invalid = thread("invalid", false, None);
        invalid["acpSessionInfo"] = json!({"updatedAt":"opaque vendor clock"});
        let groups = vec![group(
            "a",
            json!([
                thread("codex", false, Some(1)),
                invalid,
                acp,
                thread("overflow", false, Some(i64::MAX))
            ]),
        )];
        assert_eq!(
            ids(&groups, Order::Activity)
                .iter()
                .map(|(_, id)| id.as_str())
                .collect::<Vec<_>>(),
            vec!["acp", "codex", "invalid", "overflow"]
        );
        let ordered = rows(&groups, None, Order::Activity);
        assert!(ordered[0].updated.contains("01:00:01.500+01:00"));
        assert!(ordered[2].updated.contains("Invalid update time"));
        assert!(ordered[3].updated.contains("Invalid update time"));
        let value: Thread = serde_json::from_value(thread("x", false, Some(1))).unwrap();
        assert_eq!(serde_json::to_value(value).unwrap()["updatedAt"], 1);
        let mut wrong = thread("x", false, None);
        wrong["updatedAt"] = json!("milliseconds?");
        assert!(serde_json::from_value::<Thread>(wrong).is_err());
    }
}
