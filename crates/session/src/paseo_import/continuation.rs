//! One explicitly dispatched Codex fork per imported archive. Never resumes the
//! source provider identity or retries an ambiguous dispatch.
use super::{history, invalid};
use crate::{
    client::{self, Command, Completed, Connection, Error},
    creations::Resolution,
    routing::Target,
    storage::{StorageError, Store},
};
use fleetpane_runtime::{Operation, Outcome, Request, Response};
use rusqlite::{Connection as Database, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Intent {
    pub id: Uuid,
    pub session_id: Uuid,
    pub workspace_id: Uuid,
    pub thread_id: String,
    pub cwd: String,
    pub path: PathBuf,
    pub target: Target,
    pub attempted: bool,
}
impl Intent {
    pub fn command(&self) -> Command {
        Command::Fork {
            thread_id: self.thread_id.clone(),
            path: self.path.clone(),
            cwd: self.cwd.clone(),
        }
    }

    pub async fn resolve(&self) -> Result<Resolution, Error> {
        if !self.attempted {
            return Ok(Resolution::Prepared);
        }
        match fleetpane_runtime::unix::request(
            &self.target.endpoint.socket,
            &Request::Operation {
                operation_id: self.id,
            },
        )
        .await
        .map_err(|e| Error::Transport(e.to_string()))?
        {
            Response::Operation(operation) => self.resolve_operation(operation),
            Response::Error { code, message } => Err(Error::Rejected { code, message }),
            _ => Err(Error::Protocol("expected saved fork receipt".into())),
        }
    }

    fn resolve_operation(&self, operation: Operation) -> Result<Resolution, Error> {
        let command = self.command();
        let (method, params) = command.rpc()?;
        if operation.id != self.id || operation.method != method || operation.params != params {
            return Err(Error::Protocol(
                "fork receipt does not match its saved binding".into(),
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
                let result = message
                    .get("result")
                    .ok_or_else(|| Error::Protocol("fork receipt lacks result".into()))?;
                command.validate_completed(&Completed {
                    result: result.clone(),
                    sequence: 0,
                })?;
                let thread: crate::Thread = serde_json::from_value(result["thread"].clone())
                    .map_err(|e| Error::Protocol(e.to_string()))?;
                Ok(Resolution::Created {
                    thread_id: thread.id,
                })
            }
        }
    }
}

fn read(db: &Database, session: Uuid) -> Result<Option<Intent>, StorageError> {
    let row: Option<(String, bool)> = db
        .query_row(
            "SELECT data,attempted FROM paseo_continuations WHERE session=?1",
            [session.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    row.map(|(data, attempted)| {
        let mut intent: Intent = serde_json::from_str(&data)?;
        if intent.session_id != session {
            return Err("fork binding session mismatch".into());
        }
        intent.attempted = attempted;
        Ok(intent)
    })
    .transpose()
}

impl Store {
    pub async fn paseo_continuation(&self, session: Uuid) -> Result<Option<Intent>, Error> {
        self.job(move |db| read(db, session)).await
    }

    async fn fork_export(&self, session_id: Uuid) -> Result<PathBuf, Error> {
        let session = self.paseo_session(session_id).await?;
        let root = self.root().to_owned();
        tokio::task::spawn_blocking(move || history::export_codex(&root, &session))
            .await
            .map_err(invalid)?
    }

    /// Preparation observes the target; it never claims control or starts a model.
    /// A saved binding is returned before probing an offline/replaced Host.
    pub async fn prepare_paseo_continuation(&self, session_id: Uuid) -> Result<Intent, Error> {
        if let Some(intent) = self.paseo_continuation(session_id).await? {
            return Ok(intent);
        }
        let intent = self.fresh_paseo_continuation(session_id).await?;
        self.save_paseo_continuation(intent, None).await
    }

    /// Explicitly replace an unattempted binding. Expected UUID prevents a stale
    /// controller from replacing a newer preparation. This never dispatches.
    pub async fn reprepare_paseo_continuation(
        &self,
        session_id: Uuid,
        expected: Uuid,
    ) -> Result<Intent, Error> {
        let saved = self
            .paseo_continuation(session_id)
            .await?
            .ok_or_else(|| invalid("prepare a history fork first"))?;
        if saved.id != expected || saved.attempted {
            return Err(invalid(
                "fork binding changed or was already attempted; check saved status",
            ));
        }
        let intent = self.fresh_paseo_continuation(session_id).await?;
        self.save_paseo_continuation(intent, Some(expected)).await
    }

    async fn fresh_paseo_continuation(&self, session_id: Uuid) -> Result<Intent, Error> {
        let session = self.paseo_session(session_id).await?;
        let handle = session
            .agent
            .persistence
            .as_ref()
            .ok_or_else(|| invalid("session lacks a durable provider handle"))?;
        if session.agent.provider != "codex" || handle.provider != "codex" {
            return Err(invalid(
                "Only Codex supports independent history forks; DeepSeek ACP continuation requires source-owner coordination",
            ));
        }
        // Codex requires a real thread UUID even when the snapshot path is supplied.
        Uuid::parse_str(&handle.session_id).map_err(invalid)?;
        let parent = self.paseo_workspace(session.parent).await?;
        let workspace_id = parent
            .workspace_id
            .ok_or_else(|| invalid("imported directory is not registered"))?;
        let workspace = self.workspace(workspace_id).await?;
        workspace.inspect().await?;
        let original = crate::workspaces::directory(session.agent.cwd.clone()).await?;
        if original != workspace.registration.directory
            || workspace.registration.endpoint.socket != parent.endpoint.socket
        {
            return Err(invalid(
                "source session directory differs from the registered workspace",
            ));
        }
        let path = self.fork_export(session_id).await?;
        let state = client::snapshot(&parent.endpoint.socket).await?;
        if state.provider_info["fleetpaneProvider"] != "codex" {
            return Err(invalid(
                "target Host must identify its Codex provider; upgrade an older Host explicitly",
            ));
        }
        let target = Target {
            endpoint: parent.endpoint,
            host_id: state.host_id,
            admission_generation: state.admission.generation,
        };
        target.check(&state).map_err(invalid)?;
        let intent = Intent {
            id: Uuid::new_v4(),
            session_id,
            workspace_id,
            thread_id: handle.session_id.clone(),
            cwd: original
                .path
                .to_str()
                .ok_or_else(|| invalid("non-UTF-8 directory"))?
                .into(),
            path,
            target,
            attempted: false,
        };
        Ok(intent)
    }

    pub(super) async fn save_paseo_continuation(
        &self,
        intent: Intent,
        expected: Option<Uuid>,
    ) -> Result<Intent, Error> {
        self.job(move |db| {
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let session_id = intent.session_id;
            match expected {
                Some(expected) => {
                    if tx.execute(
                        "UPDATE paseo_continuations SET id=?1,data=?2 WHERE session=?3 AND id=?4 AND attempted=0",
                        params![intent.id.to_string(), serde_json::to_string(&intent)?, session_id.to_string(), expected.to_string()],
                    )? != 1 {
                        return Err("fork binding changed or was already attempted; check saved status".into());
                    }
                }
                None => {
                    if let Some(saved) = read(&tx, session_id)? {
                        return Ok(saved);
                    }
                    tx.execute(
                        "INSERT INTO paseo_continuations(session,id,data,attempted) VALUES(?1,?2,?3,0)",
                        params![session_id.to_string(), intent.id.to_string(), serde_json::to_string(&intent)?],
                    )?;
                }
            }
            tx.commit()?;
            Ok(intent)
        }).await
    }

    pub(super) async fn attempt_paseo_continuation(
        &self,
        session_id: Uuid,
        id: Uuid,
    ) -> Result<(), Error> {
        self.job(move |db| {
            if db.execute("UPDATE paseo_continuations SET attempted=1 WHERE session=?1 AND id=?2 AND attempted=0", params![session_id.to_string(),id.to_string()])? != 1 {
                return Err("fork already attempted; check its saved receipt instead of resending".into());
            }
            Ok(())
        }).await
    }

    pub async fn dispatch_paseo_continuation(
        &self,
        session_id: Uuid,
        expected: Uuid,
        connection: &Connection,
    ) -> Result<Completed, Error> {
        let intent = self
            .paseo_continuation(session_id)
            .await?
            .ok_or_else(|| invalid("prepare a history fork first"))?;
        if intent.id != expected {
            return Err(invalid(
                "fork binding changed; reopen its saved preparation",
            ));
        }
        if intent.attempted {
            return Err(invalid(
                "fork already attempted; check its saved receipt instead of resending",
            ));
        }
        if connection.socket != intent.target.endpoint.socket
            || connection.lease.host_id != intent.target.host_id
        {
            return Err(invalid("controller is not connected to the bound Host"));
        }
        self.workspace(intent.workspace_id).await?.inspect().await?;
        if self.fork_export(session_id).await? != intent.path {
            return Err(invalid("saved snapshot path changed"));
        }
        // Export can stream a large archive; observe ownership/admission after
        // that work, immediately before persisting the one dispatch attempt.
        let state = client::snapshot(&connection.socket).await?;
        intent.target.check(&state).map_err(invalid)?;
        if state.owner.as_ref() != Some(&connection.lease)
            || state.provider_info["fleetpaneProvider"] != "codex"
        {
            return Err(invalid("controller no longer owns the bound Codex Host"));
        }
        self.attempt_paseo_continuation(session_id, intent.id)
            .await?;
        connection.execute(&intent.command(), intent.id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fork_replies_must_create_an_independent_idle_session() {
        let command = Command::Fork {
            thread_id: "source".into(),
            path: "/tmp/archive.jsonl".into(),
            cwd: "/tmp/work".into(),
        };
        let valid = json!({"thread":{"id":"new","cwd":"/tmp/work","preview":"","name":null,"status":{"type":"idle"},"turns":[]}});
        assert!(
            command
                .validate_completed(&Completed {
                    result: valid.clone(),
                    sequence: 0
                })
                .is_ok()
        );
        for (key, value) in [
            ("id", json!("source")),
            ("id", json!("")),
            ("cwd", json!("/tmp/other")),
            ("status", json!({"type":"active"})),
            ("turns", json!([{"id":"t","status":"completed","items":[]}])),
        ] {
            let mut result = valid.clone();
            result["thread"][key] = value;
            assert!(
                command
                    .validate_completed(&Completed {
                        result,
                        sequence: 0
                    })
                    .is_err()
            );
        }
        let (_, params) = command.rpc().unwrap();
        assert_eq!(params["excludeTurns"], true);
        assert_eq!(params["deferGoalContinuation"], true);
    }
}
