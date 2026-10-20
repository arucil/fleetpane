//! Private, revisioned composer drafts. Persist dispatch identity before sending.
use crate::{
    MAX_PROMPT_BYTES,
    client::{Command, Completed, Connection, Error},
    storage::Store,
};
use fleetpane_runtime::{Outcome, Request, Response};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Key {
    pub socket: PathBuf,
    pub host_id: Uuid,
    pub thread_id: String,
    pub cwd: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleetpane_runtime::Operation;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::UnixListener,
    };

    fn root() -> PathBuf {
        std::env::temp_dir().join(format!("fp-draft-{}", Uuid::new_v4()))
    }
    fn key(root: &std::path::Path) -> Key {
        Key {
            socket: root.join("host.sock"),
            host_id: Uuid::new_v4(),
            thread_id: "thread-1".into(),
            cwd: "/example/project".into(),
        }
    }
    fn command(draft: &Draft) -> Command {
        Command::Send {
            thread_id: draft.key.thread_id.clone(),
            text: draft.text.clone(),
            options: Default::default(),
        }
    }

    async fn reply(socket: &std::path::Path, response: Response) -> tokio::task::JoinHandle<()> {
        let listener = UnixListener::bind(socket).unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut write) = stream.into_split();
            let mut line = String::new();
            BufReader::new(read).read_line(&mut line).await.unwrap();
            assert!(matches!(
                serde_json::from_str::<Request>(&line).unwrap(),
                Request::Operation { .. }
            ));
            let mut bytes = serde_json::to_vec(&response).unwrap();
            bytes.push(b'\n');
            write.write_all(&bytes).await.unwrap();
        })
    }

    #[tokio::test]
    async fn persistence_identity_conflicts_and_tombstones() {
        let root = root();
        let store = Store::open(root.clone()).await.unwrap();
        let key = key(&root);
        let draft = store.draft(key.clone()).await.unwrap();
        let a = "中文\n  preserve whitespace \n".to_owned();
        let saved = store.edit_draft(&draft, a.clone()).await.unwrap();
        let reopened = Store::open(root.clone()).await.unwrap();
        assert_eq!(reopened.draft(key.clone()).await.unwrap().text, a);
        let (first, second) = tokio::join!(
            store.edit_draft(&saved, "first".into()),
            reopened.edit_draft(&saved, "second".into())
        );
        assert_ne!(first.is_ok(), second.is_ok());
        let current = store.draft(key.clone()).await.unwrap();
        let cleared = store.edit_draft(&current, String::new()).await.unwrap();
        assert!(store.edit_draft(&current, "stale".into()).await.is_err());
        assert_eq!(
            store.draft(key.clone()).await.unwrap().revision,
            cleared.revision
        );
        for changed in [
            Key {
                host_id: Uuid::new_v4(),
                ..key.clone()
            },
            Key {
                thread_id: "other".into(),
                ..key.clone()
            },
            Key {
                socket: root.join("other.sock"),
                ..key.clone()
            },
            Key {
                cwd: "/other".into(),
                ..key.clone()
            },
        ] {
            assert!(store.draft(changed).await.unwrap().text.is_empty());
        }
        assert!(
            store
                .edit_draft(&cleared, "x".repeat(MAX_PROMPT_BYTES + 1))
                .await
                .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn dispatch_binding_and_receipt_recovery_do_not_resend() {
        let root = root();
        let store = Store::open(root.clone()).await.unwrap();
        let draft = store.draft(key(&root)).await.unwrap();
        let draft = store
            .edit_draft(&draft, "send exactly once".into())
            .await
            .unwrap();
        let id = Uuid::new_v4();
        let command = command(&draft);
        let attempted = store.begin_draft_send(&draft, id, &command).await.unwrap();
        assert!(store.begin_draft_send(&draft, id, &command).await.is_err());
        assert!(
            store
                .edit_draft(&attempted, "replacement".into())
                .await
                .is_err()
        );
        let attempt = attempted.attempt.as_ref().unwrap();
        for outcome in [Outcome::Accepted, Outcome::Unknown("Host restarted".into())] {
            let response = Response::Operation(Operation {
                id,
                method: attempt.method.clone(),
                params: attempt.params.clone(),
                outcome,
                completed_sequence: None,
            });
            let server = reply(&draft.key.socket, response).await;
            let checked = store.check_draft(&attempted).await.unwrap();
            assert_eq!(checked.draft.revision, attempted.revision);
            assert!(checked.draft.attempt.is_some());
            server.await.unwrap();
            std::fs::remove_file(&draft.key.socket).unwrap();
        }
        let wrong = Response::Operation(Operation {
            id,
            method: "thread/start".into(),
            params: attempt.params.clone(),
            outcome: Outcome::Accepted,
            completed_sequence: None,
        });
        let server = reply(&draft.key.socket, wrong).await;
        assert!(store.check_draft(&attempted).await.is_err());
        server.await.unwrap();
        std::fs::remove_file(&draft.key.socket).unwrap();
        let success = Response::Operation(Operation {
            id,
            method: attempt.method.clone(),
            params: attempt.params.clone(),
            outcome: Outcome::Finished(serde_json::json!({"result":{"turn":{"id":"turn-1"}}})),
            completed_sequence: None,
        });
        let server = reply(&draft.key.socket, success).await;
        let checked = store.check_draft(&attempted).await.unwrap();
        assert!(matches!(checked.resolution, Status::Sent { .. }));
        assert!(checked.draft.text.is_empty() && checked.draft.attempt.is_none());
        server.await.unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn v3_migration_preserves_existing_tables_and_refuses_missing_v4_drafts() {
        let root = root();
        let store = Store::open(root.clone()).await.unwrap();
        store.job(|db| {
            db.execute_batch("INSERT INTO worktree_jobs(id,plan,state) VALUES('receipt','unchanged-plan','unchanged-state'); DROP TABLE drafts; PRAGMA user_version=3;")?;
            Ok(())
        }).await.unwrap();
        let reopened = Store::open(root.clone()).await.unwrap();
        reopened
            .job(|db| {
                assert_eq!(
                    db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?,
                    4
                );
                assert_eq!(
                    db.query_row(
                        "SELECT state FROM worktree_jobs WHERE id='receipt'",
                        [],
                        |row| row.get::<_, String>(0)
                    )?,
                    "unchanged-state"
                );
                db.execute("DROP TABLE drafts", [])?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(Store::open(root.clone()).await.is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    pub id: Uuid,
    pub method: String,
    pub params: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub key: Key,
    pub revision: i64,
    pub text: String,
    pub attempt: Option<Attempt>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Status {
    Editing,
    Pending,
    Sent { turn_id: String },
    Rejected { error: Value },
    Unknown { reason: String },
}

#[derive(Debug, Serialize)]
pub struct Saved {
    pub position: i64,
    #[serde(flatten)]
    pub draft: Draft,
}

#[derive(Debug, Serialize)]
pub struct Checked {
    pub draft: Draft,
    pub resolution: Status,
}

fn invalid(error: impl std::fmt::Display) -> Error {
    Error::Input(format!("Draft: {error}"))
}

impl Key {
    fn encoded(&self) -> Result<String, Error> {
        if !self.socket.is_absolute()
            || self.socket.as_os_str().len() > 4096
            || self.thread_id.is_empty()
            || self.thread_id.len() > 4096
            || !std::path::Path::new(&self.cwd).is_absolute()
            || self.cwd.len() > 4096
            || self.thread_id.contains('\0')
            || self.cwd.contains('\0')
        {
            return Err(invalid("invalid Host/session/directory identity"));
        }
        serde_json::to_string(self).map_err(invalid)
    }
}

impl Store {
    pub async fn draft(&self, key: Key) -> Result<Draft, Error> {
        let encoded = key.encoded()?;
        self.job(move |db| {
            let row: Option<(i64, String)> = db
                .query_row(
                    "SELECT revision,data FROM drafts WHERE key=?",
                    [encoded],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            if let Some((revision, data)) = row {
                let draft: Draft = serde_json::from_str(&data)?;
                if draft.key != key
                    || draft.revision != revision
                    || draft.text.len() > MAX_PROMPT_BYTES
                {
                    return Err("draft identity/revision/size is inconsistent".into());
                }
                Ok(draft)
            } else {
                Ok(Draft {
                    key,
                    revision: 0,
                    text: String::new(),
                    attempt: None,
                })
            }
        })
        .await
    }

    async fn replace_draft(
        &self,
        old: &Draft,
        text: String,
        attempt: Option<Attempt>,
    ) -> Result<Draft, Error> {
        if text.len() > MAX_PROMPT_BYTES {
            return Err(invalid(
                "text exceeds the 64 KiB draft limit; copy or shorten it before saving",
            ));
        }
        if old.revision < 0 {
            return Err(invalid("invalid revision"));
        }
        let expected = serde_json::to_string(old).map_err(invalid)?;
        let encoded = old.key.encoded()?;
        let revision = old.revision;
        let next = Draft {
            key: old.key.clone(),
            revision: revision
                .checked_add(1)
                .ok_or_else(|| invalid("revision exhausted"))?,
            text,
            attempt,
        };
        let data = serde_json::to_string(&next).map_err(invalid)?;
        self.job(move |db| {
            let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let changed=if revision==0 {
                tx.execute("INSERT OR IGNORE INTO drafts(key,revision,data) VALUES(?,?,?)",params![encoded,next.revision,data])?
            } else {
                tx.execute("UPDATE drafts SET revision=?,data=? WHERE key=? AND revision=? AND data=?",params![next.revision,data,encoded,revision,expected])?
            };
            if changed!=1 { return Err("draft changed in another window; copy your local text, then reload the saved draft".into()); }
            tx.commit()?;
            Ok(next)
        }).await
    }

    pub async fn edit_draft(&self, draft: &Draft, text: String) -> Result<Draft, Error> {
        if draft.attempt.is_some() {
            return Err(invalid(
                "send outcome needs checking before editing this draft",
            ));
        }
        self.replace_draft(draft, text, None).await
    }

    pub async fn begin_draft_send(
        &self,
        draft: &Draft,
        id: Uuid,
        command: &Command,
    ) -> Result<Draft, Error> {
        match command {
            Command::Send {
                thread_id, text, ..
            } if thread_id == &draft.key.thread_id
                && text == &draft.text
                && draft.attempt.is_none() => {}
            _ => return Err(invalid("send does not match this saved draft")),
        }
        let (method, params) = command.rpc()?;
        self.replace_draft(
            draft,
            draft.text.clone(),
            Some(Attempt {
                id,
                method: method.into(),
                params,
            }),
        )
        .await
    }

    /// Read only the original receipt. Missing/unknown is never permission to resend.
    pub async fn check_draft(&self, draft: &Draft) -> Result<Checked, Error> {
        let Some(attempt) = &draft.attempt else {
            return Ok(Checked {
                draft: draft.clone(),
                resolution: Status::Editing,
            });
        };
        let reply = fleetpane_runtime::unix::request(
            &draft.key.socket,
            &Request::Operation {
                operation_id: attempt.id,
            },
        )
        .await
        .map_err(|error| Error::Transport(error.to_string()))?;
        let resolution = match reply {
            Response::Operation(operation) => {
                if operation.id != attempt.id
                    || operation.method != attempt.method
                    || operation.params != attempt.params
                {
                    return Err(invalid("receipt does not match the saved send identity"));
                }
                match operation.outcome {
                    Outcome::Accepted => Status::Pending,
                    Outcome::Unknown(reason) => Status::Unknown { reason },
                    Outcome::Finished(message) => {
                        if let Some(error) = message.get("error") {
                            Status::Rejected {
                                error: error.clone(),
                            }
                        } else {
                            let turn_id = message
                                .pointer("/result/turn/id")
                                .and_then(Value::as_str)
                                .filter(|id| !id.is_empty())
                                .ok_or_else(|| invalid("send receipt lacks a turn ID"))?
                                .to_owned();
                            Status::Sent { turn_id }
                        }
                    }
                }
            }
            Response::Error { code, message } => Status::Unknown {
                reason: format!("{code}: {message}"),
            },
            _ => return Err(invalid("expected a send receipt")),
        };
        let updated = match resolution {
            Status::Sent { .. } => self.replace_draft(draft, String::new(), None).await?,
            Status::Rejected { .. } => self.replace_draft(draft, draft.text.clone(), None).await?,
            _ => draft.clone(),
        };
        Ok(Checked {
            draft: updated,
            resolution,
        })
    }

    pub async fn send_draft(
        &self,
        draft: &Draft,
        connection: &Connection,
        id: Uuid,
        command: &Command,
    ) -> Result<Completed, Error> {
        if connection.socket != draft.key.socket || connection.lease.host_id != draft.key.host_id {
            return Err(invalid(
                "saved draft belongs to a different Host incarnation",
            ));
        }
        let attempted = self.begin_draft_send(draft, id, command).await?;
        let result = connection.execute(command, id).await;
        match &result {
            Ok(_) | Err(Error::Provider(_)) => {
                self.check_draft(&attempted)
                    .await
                    .map_err(|error| Error::Uncertain {
                        id,
                        reason: format!("send returned but local draft settlement failed: {error}"),
                    })?;
            }
            Err(Error::Rejected { .. }) => {
                self.replace_draft(&attempted, attempted.text.clone(), None)
                    .await?;
            }
            _ => {}
        }
        result
    }

    /// Bounded metadata/text recovery for earlier Host incarnations; never sends.
    pub async fn drafts(&self, before: Option<i64>) -> Result<Vec<Saved>, Error> {
        self.job(move |db| {
            let mut statement = db.prepare(
                "SELECT rowid,data FROM drafts WHERE rowid < ? ORDER BY rowid DESC LIMIT 32",
            )?;
            let rows = statement.query_map([before.unwrap_or(i64::MAX)], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.map(|row| {
                let (position, data) = row?;
                Ok(Saved {
                    position,
                    draft: serde_json::from_str(&data)?,
                })
            })
            .collect()
        })
        .await
    }
}
