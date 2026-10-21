//! Bounded provider-side title/workspace queries and opaque-cursor pagination.
use crate::Thread;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const PAGE_SIZE: usize = 100;
pub const MAX_THREADS: usize = 5_000;
const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub search: Option<String>,
    pub cwd: Option<String>,
    /// Provider archive membership, independent of runtime status.
    pub archived: bool,
}

impl Query {
    pub fn validate(&self) -> Result<(), Error> {
        if self
            .search
            .as_ref()
            .is_some_and(|value| value.trim().is_empty() || value.len() > 4096)
        {
            return Err(Error(
                "Title filter must contain text and fit within 4 KiB.".into(),
            ));
        }
        if self
            .cwd
            .as_ref()
            .is_some_and(|value| !std::path::Path::new(value).is_absolute())
        {
            return Err(Error(
                "Workspace filter must be an absolute directory.".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageRequest {
    pub query: Query,
    pub cursor: Option<String>,
}

impl PageRequest {
    pub fn params(&self) -> Result<serde_json::Value, Error> {
        self.query.validate()?;
        if self.cursor.as_ref().is_some_and(|value| value.is_empty()) {
            return Err(Error("Pagination cursor must not be empty.".into()));
        }
        // Filter on the provider's complete metadata index, never just loaded rows.
        Ok(
            serde_json::json!({"limit": PAGE_SIZE, "cursor": self.cursor,
            "searchTerm": self.query.search, "cwd": self.query.cwd,
            "sortKey": "updated_at", "sortDirection": "desc", "archived": self.query.archived}),
        )
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub data: Vec<Thread>,
    pub next_cursor: Option<String>,
}

#[derive(Default)]
pub struct Directory {
    pub threads: Vec<Thread>,
    pub query: Query,
    pub next_cursor: Option<String>,
    pub at_limit: bool,
    seen_cursors: BTreeSet<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("session directory: {0}")]
pub struct Error(pub String);

impl Directory {
    pub fn refresh(&self) -> PageRequest {
        PageRequest {
            query: self.query.clone(),
            cursor: None,
        }
    }

    pub fn next(&self) -> Option<PageRequest> {
        (!self.at_limit)
            .then(|| self.next_cursor.clone())
            .flatten()
            .map(|cursor| PageRequest {
                query: self.query.clone(),
                cursor: Some(cursor),
            })
    }

    /// Validate the entire update before replacing the visible result set.
    pub fn apply(&mut self, request: &PageRequest, page: Page) -> Result<(), Error> {
        request.params()?;
        let append = request.cursor.is_some();
        if append
            && (request.query != self.query || request.cursor != self.next_cursor || self.at_limit)
        {
            return Err(Error(
                "Page does not continue the current query; refresh the directory.".into(),
            ));
        }
        if page.data.len() > PAGE_SIZE || page.data.iter().any(|thread| !thread.turns.is_empty()) {
            return Err(Error(
                "Provider exceeded the metadata-only 100-session page contract.".into(),
            ));
        }
        let mut seen = if append {
            self.seen_cursors.clone()
        } else {
            BTreeSet::new()
        };
        if let Some(cursor) = &request.cursor {
            seen.insert(cursor.clone());
        }
        if let Some(cursor) = &page.next_cursor
            && (cursor.is_empty() || seen.contains(cursor))
        {
            return Err(Error(
                "Provider repeated an empty or previously visited cursor.".into(),
            ));
        }
        let mut page_ids = BTreeSet::new();
        if page
            .data
            .iter()
            .any(|thread| thread.id.is_empty() || !page_ids.insert(&thread.id))
        {
            return Err(Error(
                "Provider returned empty or duplicate thread IDs in a page.".into(),
            ));
        }
        let mut threads = if append {
            self.threads.clone()
        } else {
            Vec::new()
        };
        for thread in page.data {
            if let Some(old) = threads.iter_mut().find(|old| old.id == thread.id) {
                *old = thread; // An updated_at list can overlap while sessions change.
            } else {
                threads.push(thread);
            }
        }
        if threads.len() > MAX_THREADS
            || serde_json::to_vec(&threads)
                .map_err(|error| Error(error.to_string()))?
                .len()
                > MAX_BYTES
        {
            return Err(Error(
                "Directory exceeds 5,000 sessions / 8 MiB; narrow the filters.".into(),
            ));
        }
        self.threads = threads;
        self.query = request.query.clone();
        self.next_cursor = page.next_cursor;
        self.at_limit = self.threads.len() == MAX_THREADS;
        self.seen_cursors = seen;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn thread(id: &str) -> Thread {
        serde_json::from_value(json!({"id":id,"cwd":"/synthetic","preview":id,"name":null,"status":{"type":"idle"},"turns":[]})).unwrap()
    }
    fn page(ids: &[&str], next: Option<&str>) -> Page {
        Page {
            data: ids.iter().map(|id| thread(id)).collect(),
            next_cursor: next.map(str::to_owned),
        }
    }

    #[test]
    fn provider_query_keeps_filters_and_cursor_on_every_page() {
        let mut directory = Directory::default();
        let first = PageRequest {
            query: Query {
                archived: false,
                search: Some("中文".into()),
                cwd: Some(std::env::temp_dir().join("synthetic").display().to_string()),
            },
            cursor: None,
        };
        directory
            .apply(&first, page(&["a", "b"], Some("opaque:one")))
            .unwrap();
        let next = directory.next().unwrap();
        let params = next.params().unwrap();
        assert_eq!(params["cursor"], "opaque:one");
        assert_eq!(params["searchTerm"], "中文");
        assert_eq!(params["cwd"], first.query.cwd.as_deref().unwrap());
        directory.apply(&next, page(&["b", "c"], None)).unwrap();
        assert_eq!(
            directory
                .threads
                .iter()
                .map(|thread| thread.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        assert!(directory.next().is_none());
    }

    #[test]
    fn bad_pages_do_not_destroy_rows_or_advance_cursor() {
        let mut directory = Directory::default();
        directory
            .apply(&PageRequest::default(), page(&["a"], Some("one")))
            .unwrap();
        let next = directory.next().unwrap();
        assert!(directory.apply(&next, page(&["b"], Some("one"))).is_err());
        assert!(directory.apply(&next, page(&["b", "b"], None)).is_err());
        let wrong = PageRequest {
            query: Query {
                archived: false,
                search: Some("other".into()),
                cwd: None,
            },
            ..next.clone()
        };
        assert!(directory.apply(&wrong, page(&["b"], None)).is_err());
        assert_eq!(directory.threads[0].id, "a");
        assert_eq!(directory.next_cursor.as_deref(), Some("one"));
        directory.apply(&next, page(&["b"], Some("two"))).unwrap();
        assert!(
            directory
                .apply(&directory.next().unwrap(), page(&[], Some("one")))
                .is_err()
        );
        directory
            .apply(&PageRequest::default(), page(&[], None))
            .unwrap();
        assert!(directory.threads.is_empty());
    }

    #[test]
    fn directory_bounds_and_invalid_queries_are_explicit() {
        let mut directory = Directory::default();
        let mut request = PageRequest::default();
        for offset in (0..MAX_THREADS).step_by(PAGE_SIZE) {
            let data = (offset..offset + PAGE_SIZE)
                .map(|index| thread(&index.to_string()))
                .collect();
            directory
                .apply(
                    &request,
                    Page {
                        data,
                        next_cursor: Some((offset + PAGE_SIZE).to_string()),
                    },
                )
                .unwrap();
            if offset + PAGE_SIZE < MAX_THREADS {
                request = directory.next().unwrap();
            }
        }
        assert!(directory.at_limit && directory.next().is_none());
        assert_eq!(directory.threads.len(), MAX_THREADS);
        assert!(
            PageRequest {
                query: Query {
                    archived: false,
                    search: None,
                    cwd: Some("relative".into())
                },
                cursor: None
            }
            .params()
            .is_err()
        );
        let oversized = Page {
            data: (0..=PAGE_SIZE).map(|i| thread(&i.to_string())).collect(),
            next_cursor: None,
        };
        assert!(
            Directory::default()
                .apply(&PageRequest::default(), oversized)
                .is_err()
        );
    }
}

#[cfg(test)]
mod archive_scope_tests {
    use super::*;

    #[test]
    fn archive_membership_is_part_of_the_paging_scope() {
        let mut directory = Directory::default();
        let first = PageRequest::default();
        assert_eq!(first.params().unwrap()["archived"], false);
        directory
            .apply(
                &first,
                Page {
                    data: vec![],
                    next_cursor: Some("active-page".into()),
                },
            )
            .unwrap();
        let next = directory.next().unwrap();
        let mut wrong = next.clone();
        wrong.query.archived = true;
        assert!(
            directory
                .apply(
                    &wrong,
                    Page {
                        data: vec![],
                        next_cursor: None
                    }
                )
                .is_err()
        );
        assert_eq!(directory.next_cursor.as_deref(), Some("active-page"));
        assert!(!directory.query.archived);
        let archived = PageRequest {
            query: Query {
                archived: true,
                ..Query::default()
            },
            cursor: None,
        };
        assert_eq!(archived.params().unwrap()["archived"], true);
        directory
            .apply(
                &archived,
                Page {
                    data: vec![],
                    next_cursor: Some("archive-page".into()),
                },
            )
            .unwrap();
        assert!(directory.query.archived);
        assert_eq!(directory.next().unwrap().query, archived.query);
    }
}
