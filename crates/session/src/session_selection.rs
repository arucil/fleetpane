//! Device-local last inspected session. Saving never contacts a Host.
use crate::{
    client::Error,
    hosts::Configuration,
    storage::{StorageError, Store},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const MAX_RECORD_BYTES: usize = 16 * 1024;
const MAX_ENDPOINTS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub host_id: Uuid,
    pub thread_id: String,
    pub cwd: String,
    pub archived: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Saved {
    pub revision: u64,
    pub target: Option<Target>,
}

impl Target {
    /// Check current provider membership explicitly, with bounded metadata pages.
    /// A missing row is never interpreted as an unarchived or resumable session.
    pub async fn observe(&self, socket: PathBuf) -> Result<(), Error> {
        use crate::{
            directory::{PageRequest, Query},
            fleet_directory,
            hosts::Endpoint,
        };
        self.validate()?;
        Configuration::single(socket.clone()).map_err(|error| Error::Input(error.to_string()))?;
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let before=crate::client::snapshot(&socket).await?;
            if before.host_id!=self.host_id {return Err(Error::Input("Saved session belongs to a replaced Host".into()));}
            let endpoint=Endpoint {name:"Saved session".into(),socket:socket.clone()};
            let mut request=PageRequest {query:Query {cwd:Some(self.cwd.clone()),archived:self.archived,search:None},cursor:None};
            let mut cursors=std::collections::BTreeSet::new();
            for _ in 0..8 {
                let observed=fleet_directory::read_page(&endpoint,Some(self.host_id),&request).await?;
                if let Some(thread)=observed.page.data.iter().find(|thread|thread.id==self.thread_id) {
                    self.check(observed.host_id,&thread.id,&thread.cwd)?;
                    let after=crate::client::snapshot(&socket).await?;
                    if before.host_id!=after.host_id || before.owner!=after.owner {
                        return Err(Error::Input("Host or controller changed while checking last session".into()));
                    }
                    return Ok(());
                }
                let Some(cursor)=observed.page.next_cursor else {
                    return Err(Error::Input("Last session is absent from its saved workspace/archive directory; choose a current session explicitly".into()));
                };
                if !cursors.insert(cursor.clone()) {return Err(Error::Protocol("Last-session directory repeated a cursor".into()));}
                request.cursor=Some(cursor);
            }
            Err(Error::Input("Last-session check reached 800 metadata entries; use All sessions to page and select explicitly".into()))
        }).await.map_err(|_|Error::Transport("Last-session check exceeded 10 seconds; choose or retry explicitly".into()))?
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.host_id.is_nil()
            || self.thread_id.is_empty()
            || self.thread_id.len() > 4096
            || self.thread_id.contains('\0')
            || !Path::new(&self.cwd).is_absolute()
            || self.cwd.len() > 4096
            || self.cwd.contains('\0')
        {
            return Err(Error::Input("Saved session requires an exact Host, session ID and absolute directory within 4 KiB each".into()));
        }
        Ok(())
    }

    /// Incarnations and directories are identity, never mutable display labels.
    pub fn check(&self, host_id: Uuid, thread_id: &str, cwd: &str) -> Result<(), Error> {
        self.validate()?;
        if self.host_id != host_id || self.thread_id != thread_id || self.cwd != cwd {
            return Err(Error::Input(
                "Saved session source changed; choose a current session explicitly".into(),
            ));
        }
        Ok(())
    }
}

fn socket_key(socket: &Path) -> Result<String, StorageError> {
    Configuration::single(socket.to_owned())?;
    let key = serde_json::to_string(socket)?;
    if key.len() > 4096 {
        return Err("saved session socket exceeds 4096 encoded bytes".into());
    }
    Ok(key)
}

fn read(db: &Connection, key: &str) -> Result<Saved, StorageError> {
    let row: Option<(i64, Option<String>)> = db.query_row(
        "SELECT revision,CASE WHEN length(CAST(data AS BLOB))<=16384 THEN data ELSE NULL END FROM session_selection WHERE socket=?1",
        [key], |row| Ok((row.get(0)?,row.get(1)?)),
    ).optional()?;
    let Some((revision, data)) = row else {
        return Ok(Saved::default());
    };
    let saved: Saved = serde_json::from_str(&data.ok_or("saved session exceeds 16 KiB")?)?;
    if revision <= 0 || saved.revision != u64::try_from(revision)? {
        return Err("invalid saved session revision".into());
    }
    saved
        .target
        .as_ref()
        .ok_or("saved session record has no target")?
        .validate()?;
    Ok(saved)
}

impl Store {
    pub async fn session_selection(&self, socket: PathBuf) -> Result<Saved, Error> {
        self.job(move |db| read(db, &socket_key(&socket)?)).await
    }

    pub async fn save_session_selection(
        &self,
        socket: PathBuf,
        expected_revision: u64,
        target: Target,
    ) -> Result<Saved, Error> {
        self.job(move |db| {
            target.validate()?;
            let key = socket_key(&socket)?;
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current = read(&tx, &key)?;
            if current.revision != expected_revision {
                return Err("last session changed in another window; reload before remembering the current session".into());
            }
            if current.revision == 0 {
                let count: i64 = tx.query_row("SELECT count(*) FROM session_selection", [], |r| r.get(0))?;
                if usize::try_from(count)? >= MAX_ENDPOINTS { return Err("last-session storage reached its 64-endpoint limit".into()); }
            }
            let revision = expected_revision.checked_add(1).and_then(|r|i64::try_from(r).ok()).ok_or("last-session revision exhausted")?;
            let saved = Saved {revision: u64::try_from(revision)?, target: Some(target)};
            let data = serde_json::to_string(&saved)?;
            if data.len() > MAX_RECORD_BYTES { return Err("saved session exceeds 16 KiB".into()); }
            tx.execute("INSERT INTO session_selection(socket,revision,data) VALUES(?1,?2,?3) ON CONFLICT(socket) DO UPDATE SET revision=excluded.revision,data=excluded.data",params![key,revision,data])?;
            tx.commit()?;
            Ok(saved)
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Target {
        Target {
            host_id: Uuid::new_v4(),
            thread_id: "same/会话:1".into(),
            cwd: "/example/目录 with spaces".into(),
            archived: true,
        }
    }

    #[tokio::test]
    async fn exact_sources_survive_reopen_and_are_independent_of_names_and_other_hosts() {
        let root = std::env::temp_dir().join(format!("fp-last-{}", Uuid::new_v4()));
        let store = Store::open(root.clone()).await.unwrap();
        let a = root.join("a.sock");
        let b = root.join("b.sock");
        let value = target();
        assert_eq!(
            store.session_selection(a.clone()).await.unwrap(),
            Saved::default()
        );
        let saved = store
            .save_session_selection(a.clone(), 0, value.clone())
            .await
            .unwrap();
        let reopened = Store::open(root.clone()).await.unwrap();
        assert_eq!(reopened.session_selection(a.clone()).await.unwrap(), saved);
        assert_eq!(
            reopened.session_selection(b.clone()).await.unwrap(),
            Saved::default()
        );
        assert!(
            value
                .check(Uuid::new_v4(), &value.thread_id, &value.cwd)
                .is_err()
        );
        assert!(
            value
                .check(value.host_id, &value.thread_id, "/another-directory")
                .is_err()
        );
        assert!(
            value
                .check(value.host_id, "another-session", &value.cwd)
                .is_err()
        );
        value
            .check(value.host_id, &value.thread_id, &value.cwd)
            .unwrap();
        let mut other = value.clone();
        other.host_id = Uuid::new_v4();
        reopened
            .save_session_selection(b.clone(), 0, other.clone())
            .await
            .unwrap();
        assert_eq!(reopened.session_selection(a).await.unwrap(), saved);
        assert_eq!(
            reopened.session_selection(b).await.unwrap().target,
            Some(other)
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn concurrent_writes_commit_failure_and_corruption_preserve_saved_selection() {
        let root = std::env::temp_dir().join(format!("fp-last-{}", Uuid::new_v4()));
        let store = Store::open(root.clone()).await.unwrap();
        let other = Store::open(root.clone()).await.unwrap();
        let socket = root.join("host.sock");
        let (a, b) = tokio::join!(
            store.save_session_selection(socket.clone(), 0, target()),
            other.save_session_selection(socket.clone(), 0, target())
        );
        assert_ne!(a.is_ok(), b.is_ok());
        let saved = store.session_selection(socket.clone()).await.unwrap();
        store.job(|db| {db.execute_batch("CREATE TRIGGER reject_session_selection BEFORE UPDATE ON session_selection BEGIN SELECT RAISE(ABORT,'injected last-session write failure'); END;")?;Ok(())}).await.unwrap();
        assert!(
            store
                .save_session_selection(socket.clone(), 1, target())
                .await
                .is_err()
        );
        assert_eq!(
            store.session_selection(socket.clone()).await.unwrap(),
            saved
        );
        store.job(|db| {db.execute_batch("DROP TRIGGER reject_session_selection; UPDATE session_selection SET data='corrupt';")?;Ok(())}).await.unwrap();
        assert!(store.session_selection(socket.clone()).await.is_err());
        assert!(
            store
                .save_session_selection(socket, 1, target())
                .await
                .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn schema_upgrade_and_limits_do_not_replace_existing_state() {
        let root = std::env::temp_dir().join(format!("fp-last-{}", Uuid::new_v4()));
        let store = Store::open(root.clone()).await.unwrap();
        store.job(|db| {db.execute_batch("INSERT INTO drafts VALUES('retained',7,'exact legacy draft'); DROP TABLE workspace_names; DROP TABLE pending_alert_cursors; DROP TABLE completion_alert_cursors; DROP TABLE notification_preferences; DROP TABLE workspace_layout; DROP TABLE session_selection; PRAGMA user_version=12;")?;Ok(())}).await.unwrap();
        let reopened = Store::open(root.clone()).await.unwrap();
        assert_eq!(
            reopened
                .session_selection(root.join("host.sock"))
                .await
                .unwrap(),
            Saved::default()
        );
        reopened
            .job(|db| {
                assert_eq!(
                    db.query_row("SELECT data FROM drafts", [], |r| r.get::<_, String>(0))?,
                    "exact legacy draft"
                );
                Ok(())
            })
            .await
            .unwrap();
        let mut invalid = target();
        invalid.cwd = "relative".into();
        assert!(
            reopened
                .save_session_selection(root.join("invalid.sock"), 0, invalid)
                .await
                .is_err()
        );
        for index in 0..MAX_ENDPOINTS {
            reopened
                .save_session_selection(root.join(format!("{index}.sock")), 0, target())
                .await
                .unwrap();
        }
        assert!(
            reopened
                .save_session_selection(root.join("extra.sock"), 0, target())
                .await
                .is_err()
        );
        reopened
            .save_session_selection(root.join("0.sock"), 1, target())
            .await
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
