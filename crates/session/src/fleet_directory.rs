//! On-demand metadata pages across configured Hosts. Never acquires a lease.
use crate::{
    client::{self, Command, Connection, Error},
    directory::{Directory, Page, PageRequest, Query},
    hosts::{Configuration, Endpoint},
};
use serde::Serialize;
use std::time::Duration;
use uuid::Uuid;

pub mod order;

#[derive(Serialize)]
pub struct Observation {
    pub host_id: Uuid,
    pub page: Page,
}

#[derive(Serialize)]
pub struct Report {
    pub endpoint: Endpoint,
    pub observation: Option<Observation>,
    pub error: Option<String>,
}

/// An observed source includes provider archive membership, independently of
/// runtime status. Opening history must retain that membership until restored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub host_id: Uuid,
    pub thread_id: String,
    pub archived: bool,
}

/// A failed refresh retains rows only for the same query and marks them stale.
pub struct Group {
    pub endpoint: Endpoint,
    pub host_id: Option<Uuid>,
    pub directory: Directory,
    pub error: Option<String>,
}

impl Group {
    pub fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            host_id: None,
            directory: Directory::default(),
            error: None,
        }
    }

    pub fn apply(&mut self, request: &PageRequest, result: Result<Observation, Error>) {
        let applied = result.and_then(|observation| {
            if request.cursor.is_some() && self.host_id != Some(observation.host_id) {
                return Err(Error::Input(
                    "Host changed; refresh this directory before paging".into(),
                ));
            }
            self.directory
                .apply(request, observation.page)
                .map_err(|error| Error::Protocol(error.to_string()))?;
            self.host_id = Some(observation.host_id);
            Ok(())
        });
        self.error = applied.err().map(|error| error.to_string());
        if self.error.is_some() && self.directory.query != request.query {
            self.directory = Directory::default();
            self.directory.query = request.query.clone();
            self.host_id = None;
        }
    }

    pub fn can_open(&self) -> bool {
        self.host_id.is_some() && self.error.is_none()
    }

    pub fn source(&self, id: &str) -> Result<Source, Error> {
        if !self.can_open() || !self.directory.threads.iter().any(|thread| thread.id == id) {
            return Err(Error::Input(
                "Session is not in the current observed directory; refresh before opening".into(),
            ));
        }
        Ok(Source {
            host_id: self.host_id.expect("fresh observed directory has a Host"),
            thread_id: id.to_owned(),
            archived: self.directory.query.archived,
        })
    }
}

/// Each Host is independently bounded; a slow/offline peer cannot lose other results.
pub async fn collect(configuration: Configuration, query: Query) -> Result<Vec<Report>, Error> {
    configuration
        .validate()
        .map_err(|e| Error::Input(e.to_string()))?;
    query.validate().map_err(|e| Error::Input(e.to_string()))?;
    let mut tasks = tokio::task::JoinSet::new();
    let count = configuration.hosts.len();
    for (index, endpoint) in configuration.hosts.into_iter().enumerate() {
        let request = PageRequest {
            query: query.clone(),
            cursor: None,
        };
        tasks.spawn(async move {
            let result = read_page(&endpoint, None, &request).await;
            let (observation, error) = match result {
                Ok(observation) => (Some(observation), None),
                Err(error) => (None, Some(error.to_string())),
            };
            (
                index,
                Report {
                    endpoint,
                    observation,
                    error,
                },
            )
        });
    }
    let mut reports = Vec::with_capacity(count);
    while let Some(result) = tasks.join_next().await {
        reports.push(result.map_err(|error| Error::Transport(error.to_string()))?);
    }
    reports.sort_by_key(|(index, _)| *index);
    Ok(reports.into_iter().map(|(_, report)| report).collect())
}

/// Borrow the current controller's query fence; ownership changes are explicit errors.
pub async fn read_page(
    endpoint: &Endpoint,
    expected_host: Option<Uuid>,
    request: &PageRequest,
) -> Result<Observation, Error> {
    request
        .params()
        .map_err(|error| Error::Input(error.to_string()))?;
    tokio::time::timeout(Duration::from_secs(10), async {
        let before = client::snapshot(&endpoint.socket).await?;
        if expected_host.is_some_and(|id| id != before.host_id) {
            return Err(Error::Input(
                "Host changed; refresh the session directory".into(),
            ));
        }
        if let Some(fault) = before.fault {
            return Err(Error::Input(fault));
        }
        let lease = before.owner.ok_or_else(|| {
            Error::Input(
                "No controller lease is available; take control explicitly in this Host first"
                    .into(),
            )
        })?;
        let connection = Connection {
            socket: endpoint.socket.clone(),
            lease: lease.clone(),
        };
        let completed = connection
            .execute(&Command::List(request.clone()), Uuid::new_v4())
            .await?;
        let page: Page =
            serde_json::from_value(completed.result).map_err(|e| Error::Protocol(e.to_string()))?;
        if request.cursor.is_some() && request.cursor == page.next_cursor {
            return Err(Error::Protocol(
                "Provider repeated the current directory cursor".into(),
            ));
        }
        // Reuse the existing metadata, size, identity and cursor validation without
        // needing the caller's previous pages. Group::apply checks continuation too.
        Directory::default()
            .apply(
                &PageRequest {
                    query: request.query.clone(),
                    cursor: None,
                },
                Page {
                    data: page.data.clone(),
                    next_cursor: page.next_cursor.clone(),
                },
            )
            .map_err(|e| Error::Protocol(e.to_string()))?;
        let after = client::snapshot(&endpoint.socket).await?;
        if let Some(fault) = after.fault {
            return Err(Error::Input(fault));
        }
        if after.host_id != before.host_id || after.owner.as_ref() != Some(&lease) {
            return Err(Error::Input(
                "Host or controller changed during directory observation; refresh explicitly"
                    .into(),
            ));
        }
        Ok(Observation {
            host_id: before.host_id,
            page,
        })
    })
    .await
    .map_err(|_| {
        Error::Transport(
            "Session directory read exceeded 10 seconds; no control was acquired".into(),
        )
    })?
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleetpane_runtime::{Lease, Request, Response, Snapshot};
    use serde_json::json;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    fn state(owner: Option<Lease>, host_id: Uuid) -> Snapshot {
        serde_json::from_value(json!({"protocol":fleetpane_runtime::PROTOCOL_VERSION,"host_id":host_id,"host_pid":1,"provider_pid":2,"provider_info":{},"owner":owner,"sequence":1,"operation_ids":[],"journal_total":0,"admission":{"generation":0,"mode":"ready"},"activity":{"turns":[],"in_flight_operations":0,"unknown_operations":0},"pending_requests":[],"fault":null})).unwrap()
    }

    async fn scripted(
        responses: Vec<Response>,
    ) -> (
        Endpoint,
        tokio::task::JoinHandle<Vec<Request>>,
        std::path::PathBuf,
    ) {
        let root = std::path::PathBuf::from("/tmp").join(format!("fp-dir-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let endpoint = Endpoint {
            name: "Scripted".into(),
            socket: root.join("c.sock"),
        };
        let listener = tokio::net::UnixListener::bind(&endpoint.socket).unwrap();
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for response in responses {
                let (stream, _) = listener.accept().await.unwrap();
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                requests.push(serde_json::from_str(&line).unwrap());
                let mut frame = serde_json::to_vec(&response).unwrap();
                frame.push(b'\n');
                reader.get_mut().write_all(&frame).await.unwrap();
            }
            requests
        });
        (endpoint, task, root)
    }

    #[tokio::test]
    async fn observation_refuses_controller_changes_without_claiming_or_mutating() {
        let host = Uuid::new_v4();
        let owner = Lease {
            host_id: host,
            controller_id: Uuid::new_v4(),
            generation: 1,
        };
        let replaced = Lease {
            controller_id: Uuid::new_v4(),
            generation: 2,
            ..owner.clone()
        };
        let (endpoint, task, root) = scripted(vec![
            Response::Snapshot(state(Some(owner.clone()), host)),
            Response::Query {
                message: json!({"result":{"data":[],"nextCursor":null}}),
                sequence: 2,
            },
            Response::Snapshot(state(Some(replaced), host)),
        ])
        .await;
        let error = read_page(&endpoint, Some(host), &PageRequest::default())
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("controller changed"));
        let requests = task.await.unwrap();
        assert_eq!(requests.len(), 3);
        assert!(
            matches!(&requests[1], Request::Query {lease, method, params} if lease==&owner && method=="thread/list" && params["limit"]==100)
        );
        assert!(
            matches!(requests[0], Request::Snapshot) && matches!(requests[2], Request::Snapshot)
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn unowned_host_and_history_bearing_directory_are_explicit_errors() {
        let host = Uuid::new_v4();
        let (endpoint, task, root) = scripted(vec![Response::Snapshot(state(None, host))]).await;
        assert!(
            read_page(&endpoint, None, &PageRequest::default())
                .await
                .err()
                .unwrap()
                .to_string()
                .contains("No controller lease")
        );
        assert!(matches!(&task.await.unwrap()[..], [Request::Snapshot]));
        std::fs::remove_dir_all(root).unwrap();
        let owner = Lease {
            host_id: host,
            controller_id: Uuid::new_v4(),
            generation: 1,
        };
        let (endpoint,task,root) = scripted(vec![
            Response::Snapshot(state(Some(owner),host)),
            Response::Query {message:json!({"result":{"data":[{"id":"a","cwd":"/synthetic","preview":"a","turns":[{"id":"secret-history","status":"completed","items":[]}],"status":{"type":"idle"}}],"nextCursor":null}}),sequence:2},
        ]).await;
        assert!(
            read_page(&endpoint, None, &PageRequest::default())
                .await
                .is_err()
        );
        assert!(matches!(
            &task.await.unwrap()[..],
            [Request::Snapshot, Request::Query { .. }]
        ));
        std::fs::remove_dir_all(root).unwrap();
    }
    fn group() -> Group {
        Group::new(Endpoint {
            name: "Test".into(),
            socket: std::env::temp_dir().join("fleet/control.sock"),
        })
    }
    fn observation(host: Uuid, id: &str, next: Option<&str>) -> Observation {
        Observation { host_id: host, page: Page { data: vec![serde_json::from_value(json!({"id":id,"cwd":"/synthetic","preview":id,"status":{"type":"idle"},"turns":[]})).unwrap()], next_cursor: next.map(str::to_owned) } }
    }
    #[test]
    fn colliding_ids_remain_separate_and_failed_refresh_retains_stale_rows() {
        let mut a = group();
        let mut b = group();
        let host = Uuid::new_v4();
        a.apply(&PageRequest::default(), Ok(observation(host, "same", None)));
        b.apply(
            &PageRequest::default(),
            Ok(observation(Uuid::new_v4(), "same", None)),
        );
        assert!(a.can_open() && b.can_open());
        assert_ne!(a.host_id, b.host_id);
        a.apply(
            &PageRequest::default(),
            Err(Error::Transport("offline".into())),
        );
        assert_eq!(a.directory.threads[0].id, "same");
        assert!(!a.can_open());
        assert!(b.can_open());
        let filtered = PageRequest {
            query: Query {
                search: Some("missing".into()),
                ..Query::default()
            },
            cursor: None,
        };
        a.apply(&filtered, Err(Error::Transport("offline".into())));
        assert!(a.directory.threads.is_empty());
        assert_eq!(a.directory.query, filtered.query);
    }
    #[test]
    fn replaced_host_and_invalid_pages_do_not_advance_or_enable_stale_rows() {
        let mut g = group();
        let host = Uuid::new_v4();
        g.apply(
            &PageRequest::default(),
            Ok(observation(host, "first", Some("next"))),
        );
        let next = g.directory.next().unwrap();
        g.apply(&next, Ok(observation(Uuid::new_v4(), "wrong", None)));
        assert_eq!(g.directory.threads.len(), 1);
        assert!(!g.can_open());
        g.apply(&next, Ok(observation(host, "second", Some("next"))));
        assert_eq!(g.directory.next_cursor.as_deref(), Some("next"));
        assert!(!g.can_open());
        g.apply(&next, Ok(observation(host, "second", None)));
        assert_eq!(g.directory.threads.len(), 2);
        assert!(g.can_open());
    }

    #[test]
    fn source_preserves_archive_membership_even_with_idle_runtime_status() {
        let mut g = group();
        let host = Uuid::new_v4();
        let request = PageRequest {
            query: Query {
                archived: true,
                cwd: Some("/synthetic".into()),
                ..Query::default()
            },
            cursor: None,
        };
        g.apply(&request, Ok(observation(host, "saved", None)));
        assert_eq!(
            g.source("saved").unwrap(),
            Source {
                host_id: host,
                thread_id: "saved".into(),
                archived: true
            }
        );
        assert!(g.source("absent").is_err());
        g.apply(&request, Err(Error::Transport("offline".into())));
        assert!(g.source("saved").is_err());
        g.apply(
            &PageRequest::default(),
            Ok(observation(host, "saved", None)),
        );
        assert!(!g.source("saved").unwrap().archived);
    }

    #[test]
    fn switching_archive_query_never_reuses_rows_or_cursors_on_failure() {
        let mut g = group();
        g.apply(
            &PageRequest::default(),
            Ok(observation(Uuid::new_v4(), "active", Some("active-page"))),
        );
        let archived = PageRequest {
            query: Query {
                archived: true,
                ..Query::default()
            },
            cursor: None,
        };
        g.apply(&archived, Err(Error::Transport("unavailable".into())));
        assert!(g.directory.threads.is_empty());
        assert!(g.directory.next().is_none());
        assert!(!g.can_open());
        assert_eq!(g.directory.query, archived.query);
    }
}
