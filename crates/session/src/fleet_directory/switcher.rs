//! A bounded, frozen palette model retains complete source identity.
use super::{Group, Source, order};
use crate::hosts::Endpoint;

pub const MAX_ENTRIES: usize = 200;

#[derive(Clone, Debug)]
pub struct Entry {
    pub endpoint: Endpoint,
    pub source: Source,
    pub title: String,
    pub cwd: String,
    pub status: String,
}

#[derive(Default)]
pub struct Catalog {
    pub entries: Vec<Entry>,
    pub omitted: usize,
    pub more_hosts: usize,
    pub unavailable: Vec<String>,
}

/// Called after an explicit metadata read, never from painting or typing.
pub fn catalog(groups: &[Group]) -> Catalog {
    let mut catalog = Catalog::default();
    for group in groups {
        if let Some(error) = &group.error {
            catalog
                .unavailable
                .push(format!("{}: {error}", group.endpoint.name));
        }
        if group.directory.next_cursor.is_some() || group.directory.at_limit {
            catalog.more_hosts += 1;
        }
    }
    for row in order::rows(groups, None, order::Order::Activity) {
        let Some(index) = row.thread else { continue };
        let group = &groups[row.group];
        let thread = &group.directory.threads[index];
        // Stale observations remain available in All sessions, never as
        // actionable palette targets after a failed metadata read.
        if let Ok(source) = group.source(&thread.id) {
            if catalog.entries.len() == MAX_ENTRIES {
                catalog.omitted += 1;
                continue;
            }
            catalog.entries.push(Entry {
                endpoint: group.endpoint.clone(),
                source,
                title: thread.title().to_owned(),
                cwd: thread.cwd.clone(),
                status: thread.status.kind.clone(),
            });
        }
    }
    catalog
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{directory::Page, directory::PageRequest, fleet_directory::Observation};
    use serde_json::json;
    use uuid::Uuid;

    fn group(name: &str, count: usize) -> Group {
        let mut group = Group::new(Endpoint {
            name: name.into(),
            socket: format!("/tmp/{name}.sock").into(),
        });
        group.apply(
            &PageRequest::default(),
            Ok(Observation {
                host_id: Uuid::new_v4(),
                page: Page {
                    data: (0..count)
                        .map(|i| {
                            serde_json::from_value(json!({
                                "id":format!("same-{i}"),"cwd":format!("/tmp/{name}"),
                                "name":format!("Exact 中文 {i}"),"preview":"", "updatedAt":i,
                                "status":{"type":"idle"},"turns":[]
                            }))
                            .unwrap()
                        })
                        .collect(),
                    next_cursor: None,
                },
            }),
        );
        group
    }

    #[test]
    fn colliding_ids_keep_exact_endpoint_host_and_archive_identity() {
        let mut groups = vec![group("a", 1), group("b", 1)];
        groups[1].directory.query.archived = true;
        let catalog = catalog(&groups);
        assert_eq!(catalog.entries.len(), 2);
        let a = &catalog.entries[0];
        let b = &catalog.entries[1];
        assert_eq!(a.source.thread_id, b.source.thread_id);
        assert_ne!(a.source.host_id, b.source.host_id);
        assert_ne!(a.endpoint.socket, b.endpoint.socket);
        assert_eq!(a.title, "Exact 中文 0");
        assert_eq!(a.cwd, "/tmp/a");
        assert!(!a.source.archived);
        assert!(b.source.archived);
        assert_eq!(groups[0].directory.threads.len(), 1);
        assert!(groups[0].directory.threads[0].turns.is_empty());
    }

    #[test]
    fn cap_and_missing_pages_are_visible_and_stale_rows_cannot_be_opened() {
        let mut groups = vec![
            group("a", 100),
            group("b", 100),
            group("c", 1),
            group("offline", 1),
        ];
        groups[0].directory.next_cursor = Some("more".into());
        groups[3].error = Some("Disconnected".into());
        let catalog = catalog(&groups);
        assert_eq!(catalog.entries.len(), MAX_ENTRIES);
        assert_eq!(catalog.omitted, 1);
        assert_eq!(catalog.more_hosts, 1);
        assert_eq!(catalog.unavailable, ["offline: Disconnected"]);
        assert!(
            catalog
                .entries
                .iter()
                .all(|entry| entry.cwd != "/tmp/offline")
        );
        assert_eq!(catalog.entries[0].source.thread_id, "same-99");
    }
}
