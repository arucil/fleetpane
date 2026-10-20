//! Local creation intents. Bind before dispatch, then reconcile only the bound
//! endpoint/UUID. A crash around dispatch never authorizes a second submission.
use crate::{
    Thread,
    client::{Command, Completed, Connection, Error},
    hosts::Configuration,
    routing::{self, Target},
};
use fleetpane_runtime::{Operation, Outcome, Request, Response};
use rusqlite::{Connection as Database, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::StorageError;
pub use crate::storage::Store;
const PAGE_SIZE: u32 = 32;

#[derive(Clone, Debug, Serialize)]
pub struct Intent {
    pub position: u64,
    pub id: Uuid,
    pub route: String,
    pub cwd: String,
    pub target: Target,
    pub attempted: bool,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Resolution {
    Prepared,
    Pending,
    Created { thread_id: String },
    Failed { error: serde_json::Value },
    Unknown { reason: String },
}

fn storage(error: impl std::fmt::Display) -> Error {
    Error::Input(format!("Creation records: {error}"))
}

impl Intent {
    pub fn command(&self) -> Command {
        Command::Create {
            cwd: self.cwd.clone(),
        }
    }

    fn matches(&self, route: &str, cwd: &str) -> Result<(), Error> {
        if self.route != route || self.cwd != cwd {
            return Err(storage("UUID already bound to a different creation"));
        }
        Ok(())
    }

    /// Read-only, including after Host restart. A missing receipt is ambiguous,
    /// not evidence that the provider did nothing. No claim or resend occurs.
    pub async fn resolve(&self) -> Result<Resolution, Error> {
        if !self.attempted {
            return Ok(Resolution::Prepared);
        }
        let response = fleetpane_runtime::unix::request(
            &self.target.endpoint.socket,
            &Request::Operation {
                operation_id: self.id,
            },
        )
        .await
        .map_err(|error| Error::Transport(error.to_string()))?;
        match response {
            Response::Operation(operation) => self.resolve_operation(operation),
            Response::Error { code, message } => Err(Error::Rejected { code, message }),
            _ => Err(Error::Protocol("expected saved creation receipt".into())),
        }
    }

    fn resolve_operation(&self, operation: Operation) -> Result<Resolution, Error> {
        let (method, params) = self.command().rpc()?;
        if operation.id != self.id || operation.method != method || operation.params != params {
            return Err(Error::Protocol(
                "saved creation receipt does not match its binding".into(),
            ));
        }
        match operation.outcome {
            Outcome::Accepted => Ok(Resolution::Pending),
            Outcome::Unknown(reason) => Ok(Resolution::Unknown { reason }),
            Outcome::Finished(message) => {
                if let Some(error) = message.get("error") {
                    return Ok(Resolution::Failed {
                        error: error.clone(),
                    });
                }
                #[derive(Deserialize)]
                struct Reply {
                    thread: Thread,
                }
                let value = message
                    .get("result")
                    .ok_or_else(|| Error::Protocol("creation receipt has no result".into()))?;
                let reply: Reply = serde_json::from_value(value.clone())
                    .map_err(|error| Error::Protocol(error.to_string()))?;
                reply
                    .thread
                    .validate()
                    .map_err(|error| Error::Protocol(error.to_string()))?;
                Ok(Resolution::Created {
                    thread_id: reply.thread.id,
                })
            }
        }
    }
}

impl Store {
    fn read(database: &Database, id: Uuid) -> Result<Option<Intent>, StorageError> {
        let row = database
            .query_row(
                "SELECT position,id,route,cwd,target,attempted FROM creations WHERE id=?1",
                [id.to_string()],
                Self::row,
            )
            .optional()?;
        row.map(Self::decode).transpose()
    }

    fn row(
        row: &rusqlite::Row<'_>,
    ) -> rusqlite::Result<(i64, String, String, String, String, bool)> {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
        ))
    }

    fn decode(row: (i64, String, String, String, String, bool)) -> Result<Intent, StorageError> {
        Ok(Intent {
            position: row.0.try_into()?,
            id: row.1.parse()?,
            route: row.2,
            cwd: row.3,
            target: serde_json::from_str(&row.4)?,
            attempted: row.5,
        })
    }

    pub async fn get(&self, id: Uuid) -> Result<Option<Intent>, Error> {
        self.job(move |database| Self::read(database, id)).await
    }

    pub async fn page(&self, before: Option<u64>) -> Result<Vec<Intent>, Error> {
        let before = i64::try_from(before.unwrap_or(i64::MAX as u64)).map_err(storage)?;
        self.job(move |database| {
            let mut statement = database.prepare("SELECT position,id,route,cwd,target,attempted FROM creations WHERE position < ?1 ORDER BY position DESC LIMIT ?2")?;
            let rows = statement.query_map(params![before, PAGE_SIZE], Self::row)?;
            rows.map(|row| Self::decode(row?)).collect()
        }).await
    }

    /// Reusing a UUID checks the existing intent before probing configuration.
    /// Changed priority, missing configuration or drained Hosts cannot rebind it.
    pub async fn plan(
        &self,
        configuration: &Configuration,
        route: &str,
        cwd: &str,
        id: Uuid,
    ) -> Result<Intent, Error> {
        if let Some(intent) = self.get(id).await? {
            intent.matches(route, cwd)?;
            return Ok(intent);
        }
        let selection = routing::select(configuration, route)
            .await
            .map_err(storage)?;
        let target = selection
            .target
            .ok_or_else(|| storage("no ready Host in this group"))?;
        self.bind(id, route.to_owned(), cwd.to_owned(), target)
            .await
    }

    pub async fn bind(
        &self,
        id: Uuid,
        route: String,
        cwd: String,
        target: Target,
    ) -> Result<Intent, Error> {
        Command::Create { cwd: cwd.clone() }.rpc()?;
        if route.is_empty()
            || route.len() > 64
            || cwd.len() > 4096
            || !target.endpoint.socket.is_absolute()
            || target.endpoint.socket.as_os_str().len() > 4096
        {
            return Err(storage("invalid creation binding"));
        }
        self.job(move |database| {
            let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if let Some(intent) = Self::read(&transaction, id)? {
                intent.matches(&route, &cwd)?;
                return Ok(intent);
            }
            transaction.execute(
                "INSERT INTO creations(id,route,cwd,target,attempted) VALUES(?1,?2,?3,?4,0)",
                params![id.to_string(), route, cwd, serde_json::to_string(&target)?],
            )?;
            let intent = Self::read(&transaction, id)?.ok_or("committed creation missing")?;
            transaction.commit()?;
            Ok(intent)
        })
        .await
    }

    async fn mark_attempted(&self, id: Uuid) -> Result<(), Error> {
        self.job(move |database| {
            if database.execute(
                "UPDATE creations SET attempted=1 WHERE id=?1 AND attempted=0",
                [id.to_string()],
            )? != 1
            {
                return Err(
                    "creation was already attempted; check its saved status instead of resending"
                        .into(),
                );
            }
            Ok(())
        })
        .await
    }

    /// Persists the one allowed dispatch before touching the provider. Every
    /// subsequent call is refused, including after an error or controller crash.
    pub async fn dispatch(&self, id: Uuid, connection: &Connection) -> Result<Completed, Error> {
        let intent = self
            .get(id)
            .await?
            .ok_or_else(|| storage("creation UUID not found"))?;
        if intent.attempted {
            return Err(storage(
                "creation was already attempted; check its saved status instead of resending",
            ));
        }
        if connection.socket != intent.target.endpoint.socket
            || connection.lease.host_id != intent.target.host_id
        {
            return Err(storage("controller is not connected to the bound Host"));
        }
        let state = crate::client::snapshot(&connection.socket).await?;
        intent.target.check(&state).map_err(storage)?;
        if state.owner.as_ref() != Some(&connection.lease) {
            return Err(storage("controller no longer owns the bound Host"));
        }
        self.mark_attempted(id).await?;
        connection.execute(&intent.command(), id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hosts::Endpoint;
    use std::{os::unix::fs::PermissionsExt, path::PathBuf};

    fn target(name: &str) -> Target {
        Target {
            endpoint: Endpoint {
                name: name.into(),
                socket: PathBuf::from(format!("/tmp/{name}/control.sock")),
            },
            host_id: Uuid::new_v4(),
            admission_generation: 0,
        }
    }

    #[tokio::test]
    async fn binding_survives_reopen_and_never_changes_hosts() {
        let directory = std::env::temp_dir().join(format!("fp-creations-{}", Uuid::new_v4()));
        let store = Store::open(directory.clone()).await.unwrap();
        let id = Uuid::new_v4();
        let first = store
            .bind(id, "Codex".into(), "/tmp/work".into(), target("first"))
            .await
            .unwrap();
        let second = store
            .bind(id, "Codex".into(), "/tmp/work".into(), target("second"))
            .await
            .unwrap();
        assert_eq!(first.target.host_id, second.target.host_id);
        assert!(
            store
                .bind(id, "ACP".into(), "/tmp/work".into(), target("second"))
                .await
                .is_err()
        );
        let (a, b) = tokio::join!(store.mark_attempted(id), store.mark_attempted(id));
        assert_ne!(a.is_ok(), b.is_ok());
        let reopened = Store::open(directory.clone()).await.unwrap();
        assert!(reopened.get(id).await.unwrap().unwrap().attempted);
        assert!(reopened.mark_attempted(id).await.is_err());
        assert_eq!(reopened.page(None).await.unwrap().len(), 1);
        // Planning an existing UUID never consults this invalid configuration.
        let invalid = Configuration {
            version: 99,
            hosts: vec![],
            routes: vec![],
        };
        assert_eq!(
            reopened
                .plan(&invalid, "Codex", "/tmp/work", id)
                .await
                .unwrap()
                .target
                .host_id,
            first.target.host_id
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn pagination_and_corrupt_schema_are_explicit() {
        let directory = std::env::temp_dir().join(format!("fp-creations-{}", Uuid::new_v4()));
        let store = Store::open(directory.clone()).await.unwrap();
        let path = directory.join("creations.sqlite3");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o077,
            0
        );
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(Store::open(directory.clone()).await.is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        for _ in 0..35 {
            store
                .bind(
                    Uuid::new_v4(),
                    "Codex".into(),
                    "/tmp/work".into(),
                    target("first"),
                )
                .await
                .unwrap();
        }
        let first = store.page(None).await.unwrap();
        assert_eq!(first.len(), 32);
        let next = store
            .page(Some(first.last().unwrap().position))
            .await
            .unwrap();
        assert_eq!(next.len(), 3);
        Database::open(directory.join("creations.sqlite3"))
            .unwrap()
            .execute("DROP TABLE creations", [])
            .unwrap();
        assert!(Store::open(directory.clone()).await.is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn mismatched_and_unknown_receipts_cannot_be_opened() {
        let intent = Intent {
            position: 1,
            id: Uuid::new_v4(),
            route: "Codex".into(),
            cwd: "/tmp/work".into(),
            target: target("first"),
            attempted: true,
        };
        let (method, params) = intent.command().rpc().unwrap();
        let operation = Operation {
            id: intent.id,
            method: method.into(),
            params,
            outcome: Outcome::Unknown("provider exited".into()),
            completed_sequence: None,
        };
        assert!(matches!(
            intent.resolve_operation(operation.clone()).unwrap(),
            Resolution::Unknown { .. }
        ));
        assert!(
            intent
                .resolve_operation(Operation {
                    id: Uuid::new_v4(),
                    ..operation
                })
                .is_err()
        );
    }

    #[tokio::test]
    async fn wrong_host_is_rejected_before_dispatch_or_attempt_marker() {
        let directory = std::env::temp_dir().join(format!("fp-creations-{}", Uuid::new_v4()));
        let store = Store::open(directory.clone()).await.unwrap();
        let id = Uuid::new_v4();
        store
            .bind(id, "Codex".into(), "/tmp/work".into(), target("first"))
            .await
            .unwrap();
        let wrong = Connection {
            socket: "/tmp/second/control.sock".into(),
            lease: fleetpane_runtime::Lease {
                host_id: Uuid::new_v4(),
                controller_id: Uuid::new_v4(),
                generation: 1,
            },
        };
        assert!(
            matches!(store.dispatch(id, &wrong).await, Err(Error::Input(message)) if message.contains("bound Host"))
        );
        assert!(!store.get(id).await.unwrap().unwrap().attempted);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn historical_finished_receipts_recover_identity_without_event_replay() {
        let intent = Intent {
            position: 1,
            id: Uuid::new_v4(),
            route: "Codex".into(),
            cwd: "/tmp/work".into(),
            target: target("first"),
            attempted: true,
        };
        let (method, params) = intent.command().rpc().unwrap();
        let mut operation = Operation {
            id: intent.id,
            method: method.into(),
            params,
            outcome: Outcome::Finished(
                serde_json::json!({"result":{"thread":{"id":"saved-thread","cwd":"/tmp/work","preview":"","name":null,"historyMode":"paginated","status":{"type":"idle"},"turns":[]}}}),
            ),
            completed_sequence: None,
        };
        assert!(
            matches!(intent.resolve_operation(operation.clone()).unwrap(), Resolution::Created { thread_id } if thread_id == "saved-thread")
        );
        operation.outcome =
            Outcome::Finished(serde_json::json!({"error":{"code":1,"message":"rejected"}}));
        assert!(matches!(
            intent.resolve_operation(operation.clone()).unwrap(),
            Resolution::Failed { .. }
        ));
        operation.outcome = Outcome::Finished(serde_json::json!({"result":{}}));
        assert!(intent.resolve_operation(operation).is_err());
    }
}
