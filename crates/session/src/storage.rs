//! Private local controller state; awaited blocking jobs, bounded SQLite storage.
use crate::client::Error;
use rusqlite::{Connection as Database, OpenFlags, TransactionBehavior};
use std::{
    fs::OpenOptions,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    time::Duration,
};
pub(crate) type StorageError = Box<dyn std::error::Error + Send + Sync>;
#[derive(Clone, Debug)]
pub struct Store(PathBuf);
fn storage(error: impl std::fmt::Display) -> Error {
    Error::Input(format!("Local state: {error}"))
}

impl Store {
    pub(crate) fn root(&self) -> &std::path::Path {
        &self.0
    }

    /// Only the path lives in the UI. All filesystem/SQLite work is bounded and
    /// runs on Tokio's blocking pool; SQLite transactions serialize writers.
    pub async fn open(directory: PathBuf) -> Result<Self, Error> {
        if !directory.is_absolute() {
            return Err(storage("store directory must be absolute"));
        }
        let store = Self(directory);
        store.job(|_| Ok(())).await?;
        Ok(store)
    }

    pub(crate) async fn job<T: Send + 'static>(
        &self,
        action: impl FnOnce(&mut Database) -> Result<T, StorageError> + Send + 'static,
    ) -> Result<T, Error> {
        let directory = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let mut database = Self::database(directory)?;
            action(&mut database)
        })
        .await
        .map_err(storage)?
        .map_err(storage)
    }

    fn database(directory: PathBuf) -> Result<Database, StorageError> {
        fleetpane_runtime::unix::prepare_directory(&directory)?;
        let path = directory.join("creations.sqlite3");
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => {
                file.sync_all()?;
                std::fs::File::open(&directory)?.sync_all()?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        let metadata = std::fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
            return Err("creation database must be a private regular file (0600)".into());
        }
        let mut database = Database::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        database.busy_timeout(Duration::from_secs(2))?;
        database.execute_batch("PRAGMA foreign_keys=ON; PRAGMA page_size=4096; PRAGMA journal_mode=DELETE; PRAGMA synchronous=EXTRA; PRAGMA max_page_count=16384; PRAGMA cache_size=-1024;")?;
        let page_size: u32 = database.query_row("PRAGMA page_size", [], |row| row.get(0))?;
        if page_size != 4096 {
            return Err("creation database page size must be 4096".into());
        }
        let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: u32 = transaction.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        match version {
            0 => {
                let tables: u32 = transaction.query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table'",
                    [],
                    |row| row.get(0),
                )?;
                if tables != 0 {
                    return Err("unversioned creation database contains tables".into());
                }
                transaction.execute_batch("CREATE TABLE creations(position INTEGER PRIMARY KEY, id TEXT UNIQUE NOT NULL, route TEXT NOT NULL, cwd TEXT NOT NULL, target TEXT NOT NULL, attempted INTEGER NOT NULL CHECK(attempted IN (0,1)));")?;
            }
            1..=4 => {
                transaction.prepare(
                    "SELECT position,id,route,cwd,target,attempted FROM creations LIMIT 0",
                )?;
            }
            _ => return Err("unsupported creation database schema".into()),
        }
        if version < 2 {
            transaction.execute_batch("CREATE TABLE projects(id TEXT PRIMARY KEY, endpoint TEXT NOT NULL, identity TEXT NOT NULL, UNIQUE(endpoint,identity)); CREATE TABLE workspaces(position INTEGER PRIMARY KEY, id TEXT UNIQUE NOT NULL, project TEXT NOT NULL REFERENCES projects(id), endpoint TEXT NOT NULL, cwd TEXT NOT NULL, kind TEXT NOT NULL, identity TEXT NOT NULL, data TEXT NOT NULL, UNIQUE(endpoint,cwd,kind,identity,project)); PRAGMA user_version=2;")?;
        } else {
            transaction.prepare("SELECT id,endpoint,identity FROM projects LIMIT 0")?;
            transaction.prepare("SELECT position,id,project,endpoint,cwd,kind,identity,data FROM workspaces LIMIT 0")?;
        }
        if version < 3 {
            transaction.execute_batch("CREATE TABLE worktree_jobs(position INTEGER PRIMARY KEY, id TEXT UNIQUE NOT NULL, plan TEXT NOT NULL, state TEXT NOT NULL); PRAGMA user_version=3;")?;
        } else {
            transaction.prepare("SELECT position,id,plan,state FROM worktree_jobs LIMIT 0")?;
        }
        if version < 4 {
            transaction.execute_batch("CREATE TABLE drafts(key TEXT PRIMARY KEY, revision INTEGER NOT NULL, data TEXT NOT NULL); PRAGMA user_version=4;")?;
        } else {
            transaction.prepare("SELECT key,revision,data FROM drafts LIMIT 0")?;
        }
        transaction.commit()?;
        Ok(database)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hosts::Endpoint, routing::Target};
    use uuid::Uuid;

    #[tokio::test]
    async fn v1_migration_preserves_creation_bindings_and_attempts() {
        let directory = std::env::temp_dir().join(format!("fp-state-{}", Uuid::new_v4()));
        let store = Store::open(directory.clone()).await.unwrap();
        let id = Uuid::new_v4();
        let target = Target {
            endpoint: Endpoint {
                name: "Local".into(),
                socket: "/tmp/local/control.sock".into(),
            },
            host_id: Uuid::new_v4(),
            admission_generation: 2,
        };
        store
            .bind(id, "Codex".into(), "/tmp/work".into(), target.clone())
            .await
            .unwrap();
        store
            .job(move |db| {
                db.execute(
                    "UPDATE creations SET attempted=1 WHERE id=?1",
                    [id.to_string()],
                )?;
                db.execute_batch(
                    "DROP TABLE drafts; DROP TABLE worktree_jobs; DROP TABLE workspaces; DROP TABLE projects; PRAGMA user_version=1;",
                )?;
                Ok(())
            })
            .await
            .unwrap();
        let reopened = Store::open(directory.clone()).await.unwrap();
        let intent = reopened.get(id).await.unwrap().unwrap();
        assert!(intent.attempted);
        assert_eq!(intent.target.host_id, target.host_id);
        assert_eq!(intent.target.admission_generation, 2);
        assert!(reopened.workspaces(None).await.unwrap().is_empty());
        reopened
            .job(|db| {
                db.execute("DROP TABLE workspaces", [])?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(Store::open(directory.clone()).await.is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn v2_migration_preserves_workspace_identity() {
        let path = std::env::temp_dir().join(format!("fp-migrate-{}", Uuid::new_v4()));
        let store = Store::open(path.clone()).await.unwrap();
        let workspace = store
            .register_workspace(
                Endpoint {
                    name: "Local".into(),
                    socket: "/tmp/local/control.sock".into(),
                },
                path.clone(),
                crate::workspaces::Kind::Folder,
            )
            .await
            .unwrap();
        store
            .job(|db| {
                db.execute_batch(
                    "DROP TABLE drafts; DROP TABLE worktree_jobs; PRAGMA user_version=2;",
                )?;
                Ok(())
            })
            .await
            .unwrap();
        let reopened = Store::open(path.clone()).await.unwrap();
        assert_eq!(
            reopened.workspace(workspace.id).await.unwrap().id,
            workspace.id
        );
        assert_eq!(
            reopened.workspace(workspace.id).await.unwrap().project_id,
            workspace.project_id
        );
        assert!(reopened.worktree_jobs(None).await.unwrap().is_empty());
        reopened
            .job(|db| {
                db.execute("DROP TABLE worktree_jobs", [])?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(Store::open(path.clone()).await.is_err());
        std::fs::remove_dir_all(path).unwrap();
    }

    #[tokio::test]
    async fn missing_v1_creation_table_is_not_reinitialized() {
        let directory = std::env::temp_dir().join(format!("fp-state-{}", Uuid::new_v4()));
        let store = Store::open(directory.clone()).await.unwrap();
        store.job(|db| { db.execute_batch("DROP TABLE drafts; DROP TABLE worktree_jobs; DROP TABLE workspaces; DROP TABLE projects; DROP TABLE creations; PRAGMA user_version=1;")?;Ok(()) }).await.unwrap();
        assert!(Store::open(directory.clone()).await.is_err());
        let db = Database::open(directory.join("creations.sqlite3")).unwrap();
        let version: u32 = db
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
