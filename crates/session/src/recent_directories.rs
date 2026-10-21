//! Device-local directory suggestions, scoped to the exact Host endpoint.
use crate::{
    client::Error,
    storage::{StorageError, Store},
};
use rusqlite::{Connection, TransactionBehavior, params};
use std::path::{Path, PathBuf};

pub const MAX_RECENT_DIRECTORIES: usize = 3;
const MAX_TOTAL_DIRECTORIES: usize = 96;

fn path_text(path: &Path) -> Result<String, StorageError> {
    let text = path
        .to_str()
        .ok_or("recent directory paths must be UTF-8")?;
    if !path.is_absolute() || text.len() > 4096 || text.contains('\0') {
        return Err(
            "recent directory and Host socket must be absolute paths within 4 KiB, without NUL"
                .into(),
        );
    }
    Ok(text.to_owned())
}

fn read(database: &Connection, socket: &str) -> Result<Vec<PathBuf>, StorageError> {
    let mut statement = database.prepare(
        "SELECT cwd FROM recent_directories WHERE socket=?1 ORDER BY position DESC LIMIT ?2",
    )?;
    let paths = statement
        .query_map(
            params![socket, i64::try_from(MAX_RECENT_DIRECTORIES + 1)?],
            |row| row.get::<_, String>(0),
        )?
        .map(|row| {
            let path = PathBuf::from(row?);
            path_text(&path)?;
            Ok(path)
        })
        .collect::<Result<Vec<_>, StorageError>>()?;
    if paths.len() > MAX_RECENT_DIRECTORIES {
        return Err("recent directory records exceed the supported limit".into());
    }
    Ok(paths)
}

impl Store {
    pub async fn recent_directories(&self, socket: PathBuf) -> Result<Vec<PathBuf>, Error> {
        let socket = path_text(&socket).map_err(|e| Error::Input(e.to_string()))?;
        self.job(move |database| read(database, &socket)).await
    }

    /// A suggestion records use, not directory identity or permission to create.
    /// Callers surface save failures independently of a successful session creation.
    pub async fn remember_directory(
        &self,
        socket: PathBuf,
        cwd: PathBuf,
    ) -> Result<Vec<PathBuf>, Error> {
        let socket = path_text(&socket).map_err(|e| Error::Input(e.to_string()))?;
        let cwd = path_text(&cwd).map_err(|e| Error::Input(e.to_string()))?;
        self.job(move |database| {
            let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
            // Validate existing records before replacing anything; corruption is visible.
            read(&transaction, &socket)?;
            transaction.execute("INSERT INTO recent_directories(position,socket,cwd) VALUES((SELECT COALESCE(MAX(position),0)+1 FROM recent_directories),?1,?2) ON CONFLICT(socket,cwd) DO UPDATE SET position=excluded.position", params![socket,cwd])?;
            transaction.execute("DELETE FROM recent_directories WHERE socket=?1 AND position NOT IN (SELECT position FROM recent_directories WHERE socket=?1 ORDER BY position DESC LIMIT ?2)", params![socket,i64::try_from(MAX_RECENT_DIRECTORIES)?])?;
            transaction.execute("DELETE FROM recent_directories WHERE position NOT IN (SELECT position FROM recent_directories ORDER BY position DESC LIMIT ?1)", [i64::try_from(MAX_TOTAL_DIRECTORIES)?])?;
            let paths = read(&transaction, &socket)?;
            transaction.commit()?;
            Ok(paths)
        }).await
    }

    /// Forget suggestions for this endpoint; sessions and saved workspaces are untouched.
    pub async fn clear_recent_directories(&self, socket: PathBuf) -> Result<(), Error> {
        let socket = path_text(&socket).map_err(|e| Error::Input(e.to_string()))?;
        self.job(move |database| {
            database.execute("DELETE FROM recent_directories WHERE socket=?1", [socket])?;
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::{Preferences, Theme};

    async fn fixture() -> (PathBuf, Store) {
        let root = std::env::temp_dir().join(format!("fp-recent-{}", uuid::Uuid::new_v4()));
        let store = Store::open(root.clone()).await.unwrap();
        (root, store)
    }

    #[tokio::test]
    async fn recent_order_is_exact_deduplicated_persistent_and_host_scoped() {
        let (root, store) = fixture().await;
        let socket = PathBuf::from("/tmp/host-a.sock");
        let other = PathBuf::from("/tmp/host-b.sock");
        let exact = PathBuf::from("/tmp/中文 directory\nwith trailing space ");
        for path in ["/tmp/one", "/tmp/two", "/tmp/three", "/tmp/four"] {
            store
                .remember_directory(socket.clone(), PathBuf::from(path))
                .await
                .unwrap();
        }
        store
            .remember_directory(socket.clone(), PathBuf::from("/tmp/two"))
            .await
            .unwrap();
        assert_eq!(
            store.recent_directories(socket.clone()).await.unwrap(),
            ["/tmp/two", "/tmp/four", "/tmp/three"].map(PathBuf::from)
        );
        store
            .remember_directory(other.clone(), exact.clone())
            .await
            .unwrap();
        let reopened = Store::open(root.clone()).await.unwrap();
        assert_eq!(
            reopened.recent_directories(other.clone()).await.unwrap(),
            vec![exact]
        );
        reopened
            .clear_recent_directories(socket.clone())
            .await
            .unwrap();
        assert!(store.recent_directories(socket).await.unwrap().is_empty());
        assert_eq!(store.recent_directories(other).await.unwrap().len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn concurrent_writes_and_total_retention_are_bounded() {
        let (root, store) = fixture().await;
        let mut jobs = Vec::new();
        for n in 0..16 {
            let store = store.clone();
            let directory = store.root().to_path_buf();
            jobs.push(tokio::spawn(async move {
                let store = if n % 2 == 0 {
                    Store::open(directory).await?
                } else {
                    store
                };
                store
                    .remember_directory("/tmp/same.sock".into(), PathBuf::from(format!("/tmp/{n}")))
                    .await
            }));
        }
        for job in jobs {
            job.await.unwrap().unwrap();
        }
        let paths = store
            .recent_directories("/tmp/same.sock".into())
            .await
            .unwrap();
        assert_eq!(paths.len(), 3);
        assert!(
            paths
                .iter()
                .all(|p| p.to_str().unwrap().starts_with("/tmp/"))
        );
        for n in 0..100 {
            store
                .remember_directory(
                    PathBuf::from(format!("/tmp/host-{n}.sock")),
                    "/tmp/project".into(),
                )
                .await
                .unwrap();
        }
        store
            .job(|db| {
                assert_eq!(
                    db.query_row("SELECT count(*) FROM recent_directories", [], |r| r
                        .get::<_, i64>(0))?,
                    96
                );
                Ok(())
            })
            .await
            .unwrap();
        assert!(
            store
                .recent_directories("/tmp/same.sock".into())
                .await
                .unwrap()
                .is_empty()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn schema_six_upgrade_preserves_preferences_and_corrupt_rows_are_visible() {
        let (root, store) = fixture().await;
        store
            .save_appearance(0, Preferences { theme: Theme::Dark })
            .await
            .unwrap();
        store
            .job(|db| {
                db.execute_batch("DROP TABLE paseo_batch_results; DROP TABLE paseo_batches; DROP TABLE paseo_continuations; DROP TABLE paseo_sessions; DROP TABLE paseo_workspaces; DROP TABLE recent_directories; PRAGMA user_version=6;")?;
                Ok(())
            })
            .await
            .unwrap();
        let store = Store::open(root.clone()).await.unwrap();
        assert_eq!(store.appearance().await.unwrap().value.theme, Theme::Dark);
        assert!(
            store
                .recent_directories("/tmp/host.sock".into())
                .await
                .unwrap()
                .is_empty()
        );
        store.job(|db|{db.execute("INSERT INTO recent_directories(socket,cwd) VALUES('/tmp/host.sock','relative')",[])?;Ok(())}).await.unwrap();
        assert!(
            store
                .recent_directories("/tmp/host.sock".into())
                .await
                .is_err()
        );
        assert!(
            store
                .remember_directory("/tmp/host.sock".into(), "/tmp/new".into())
                .await
                .is_err()
        );
        assert!(
            store
                .remember_directory("relative".into(), "/tmp/new".into())
                .await
                .is_err()
        );
        assert!(
            store
                .remember_directory("/tmp/host.sock".into(), "/tmp/nu\0l".into())
                .await
                .is_err()
        );
        store
            .job(|db| {
                assert_eq!(
                    db.query_row("SELECT cwd FROM recent_directories", [], |r| r
                        .get::<_, String>(0))?,
                    "relative"
                );
                Ok(())
            })
            .await
            .unwrap();
        store
            .clear_recent_directories("/tmp/host.sock".into())
            .await
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
