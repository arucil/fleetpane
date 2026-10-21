//! Private local controller state; awaited blocking jobs, bounded SQLite storage.
use crate::client::Error;
use rusqlite::{Connection as Database, OpenFlags, TransactionBehavior};
use std::{
    collections::HashMap,
    fs::OpenOptions,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex, Weak},
    time::Duration,
};
use tokio::sync::Mutex as JobQueue;
pub(crate) type StorageError = Box<dyn std::error::Error + Send + Sync>;
static QUEUES: LazyLock<Mutex<HashMap<PathBuf, Weak<JobQueue<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
#[derive(Clone, Debug)]
pub struct Store {
    directory: PathBuf,
    queue: Arc<JobQueue<()>>,
}
fn storage(error: impl std::fmt::Display) -> Error {
    Error::Input(format!("Local state: {error}"))
}

impl Store {
    pub(crate) fn root(&self) -> &std::path::Path {
        &self.directory
    }

    /// Filesystem/SQLite work runs on Tokio's blocking pool. Handles for the same
    /// canonical directory share a FIFO queue before opening their connections.
    /// SQLite transactions still enforce cross-process atomicity.
    pub async fn open(directory: PathBuf) -> Result<Self, Error> {
        if !directory.is_absolute() {
            return Err(storage("store directory must be absolute"));
        }
        let path = directory.clone();
        let queue = tokio::task::spawn_blocking(move || Self::queue(path))
            .await
            .map_err(storage)?
            .map_err(storage)?;
        let store = Self { directory, queue };
        store.job(|_| Ok(())).await?;
        Ok(store)
    }

    pub(crate) async fn job<T: Send + 'static>(
        &self,
        action: impl FnOnce(&mut Database) -> Result<T, StorageError> + Send + 'static,
    ) -> Result<T, Error> {
        let directory = self.directory.clone();
        let guard = self.queue.clone().lock_owned().await;
        tokio::task::spawn_blocking(move || {
            // A blocking job outlives cancellation of its async caller. Keep its
            // queue slot until the connection and action have both finished.
            let _guard = guard;
            let mut database = Self::database(directory)?;
            action(&mut database)
        })
        .await
        .map_err(storage)?
        .map_err(storage)
    }

    fn queue(directory: PathBuf) -> Result<Arc<JobQueue<()>>, StorageError> {
        fleetpane_runtime::unix::prepare_directory(&directory)?;
        let directory = std::fs::canonicalize(directory)?;
        let mut queues = QUEUES.lock().map_err(|_| "store queue registry poisoned")?;
        queues.retain(|_, queue| queue.strong_count() > 0);
        if let Some(queue) = queues.get(&directory).and_then(Weak::upgrade) {
            return Ok(queue);
        }
        let queue = Arc::new(JobQueue::new(()));
        queues.insert(directory, Arc::downgrade(&queue));
        Ok(queue)
    }

    fn database(directory: PathBuf) -> Result<Database, StorageError> {
        fleetpane_runtime::unix::prepare_directory(&directory)?;
        // SQLite NOFOLLOW rejects symlinks in every filename component. macOS
        // system roots such as /var are aliases; resolve ancestors only after
        // checking that the store directory itself is a private non-symlink.
        let directory = std::fs::canonicalize(directory)?;
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
            1..=11 => {
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
        if version < 5 {
            transaction.execute_batch("CREATE TABLE workspace_pins(position INTEGER PRIMARY KEY, workspace TEXT UNIQUE NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE); PRAGMA user_version=5;")?;
        } else {
            transaction.prepare("SELECT position,workspace FROM workspace_pins LIMIT 0")?;
        }
        if version < 6 {
            transaction.execute_batch("CREATE TABLE appearance(singleton INTEGER PRIMARY KEY CHECK(singleton=1), revision INTEGER NOT NULL CHECK(revision>0), data TEXT NOT NULL); PRAGMA user_version=6;")?;
        } else {
            transaction.prepare("SELECT singleton,revision,data FROM appearance LIMIT 0")?;
        }
        if version < 7 {
            transaction.execute_batch("CREATE TABLE recent_directories(position INTEGER PRIMARY KEY, socket TEXT NOT NULL, cwd TEXT NOT NULL, UNIQUE(socket,cwd)); PRAGMA user_version=7;")?;
        } else {
            transaction.prepare("SELECT position,socket,cwd FROM recent_directories LIMIT 0")?;
        }
        if version < 8 {
            transaction.execute_batch("CREATE TABLE paseo_workspaces(id TEXT PRIMARY KEY, source TEXT NOT NULL, source_id TEXT NOT NULL, endpoint TEXT NOT NULL, workspace TEXT REFERENCES workspaces(id), data TEXT NOT NULL, UNIQUE(source,source_id,endpoint)); CREATE TABLE paseo_sessions(position INTEGER PRIMARY KEY, id TEXT UNIQUE NOT NULL, parent TEXT NOT NULL REFERENCES paseo_workspaces(id), source_id TEXT NOT NULL, data TEXT NOT NULL, UNIQUE(parent,source_id)); PRAGMA user_version=8;")?;
        } else {
            transaction.prepare(
                "SELECT id,source,source_id,endpoint,workspace,data FROM paseo_workspaces LIMIT 0",
            )?;
            transaction
                .prepare("SELECT position,id,parent,source_id,data FROM paseo_sessions LIMIT 0")?;
        }
        if version < 9 {
            transaction.execute_batch("CREATE TABLE paseo_continuations(session TEXT PRIMARY KEY REFERENCES paseo_sessions(id), id TEXT UNIQUE NOT NULL, data TEXT NOT NULL, attempted INTEGER NOT NULL CHECK(attempted IN (0,1))); PRAGMA user_version=9;")?;
        } else {
            transaction
                .prepare("SELECT session,id,data,attempted FROM paseo_continuations LIMIT 0")?;
        }
        if version < 10 {
            transaction.execute_batch("CREATE TABLE paseo_batches(position INTEGER PRIMARY KEY, id TEXT UNIQUE NOT NULL, plan TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('running','completed','stopped','failed')), progress TEXT NOT NULL, error TEXT); CREATE TABLE paseo_batch_results(batch TEXT NOT NULL REFERENCES paseo_batches(id), ordinal INTEGER NOT NULL CHECK(ordinal>=0), data TEXT NOT NULL, PRIMARY KEY(batch,ordinal)); PRAGMA user_version=10;")?;
        } else {
            transaction.prepare(
                "SELECT position,id,plan,state,progress,error FROM paseo_batches LIMIT 0",
            )?;
            transaction.prepare("SELECT batch,ordinal,data FROM paseo_batch_results LIMIT 0")?;
        }
        if version < 11 {
            transaction.execute_batch("CREATE TABLE host_selection(singleton INTEGER PRIMARY KEY CHECK(singleton=1), revision INTEGER NOT NULL CHECK(revision>0), socket TEXT NOT NULL); PRAGMA user_version=11;")?;
        } else {
            transaction.prepare("SELECT singleton,revision,socket FROM host_selection LIMIT 0")?;
        }
        transaction.commit()?;
        Ok(database)
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn queued_cancellation_skips_action_and_failure_releases_next_job_in_order() {
        use std::{future::Future, task::Poll};
        let root = std::env::temp_dir().join(format!("fp-queue-{}", uuid::Uuid::new_v4()));
        let store = Store::open(root.clone()).await.unwrap();
        let second = Store::open(root.clone()).await.unwrap();
        let order = Arc::new(Mutex::new(Vec::new()));
        let guard = store.queue.clone().lock_owned().await;
        let events = order.clone();
        let mut first = Box::pin(store.job(move |_| {
            events.lock().unwrap().push(1);
            Err::<(), _>("injected boundary failure".into())
        }));
        std::future::poll_fn(|cx| {
            assert!(first.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        let events = order.clone();
        let mut cancelled = Box::pin(second.job(move |_| {
            events.lock().unwrap().push(2);
            Ok(())
        }));
        std::future::poll_fn(|cx| {
            assert!(cancelled.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        let events = order.clone();
        let mut last = Box::pin(second.job(move |_| {
            events.lock().unwrap().push(3);
            Ok(())
        }));
        std::future::poll_fn(|cx| {
            assert!(last.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        drop(cancelled);
        drop(guard);
        let (failed, succeeded) = tokio::join!(first, last);
        assert!(
            failed
                .unwrap_err()
                .to_string()
                .contains("injected boundary failure")
        );
        succeeded.unwrap();
        assert_eq!(*order.lock().unwrap(), vec![1, 3]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn cancelled_job_keeps_alias_opens_queued_without_blocking_other_stores() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!("fp-queue-{}", uuid::Uuid::new_v4()));
        let real = root.join("real");
        std::fs::create_dir_all(&real).unwrap();
        symlink(&real, root.join("alias")).unwrap();
        let store = Store::open(real.join("state")).await.unwrap();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let (finished, done) = tokio::sync::oneshot::channel();
        let held = store.clone();
        let caller = tokio::spawn(async move {
            held.job(move |db| {
                let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
                started.send(()).unwrap();
                wait.recv_timeout(Duration::from_secs(15))?;
                transaction.commit()?;
                finished.send(()).unwrap();
                Ok(())
            })
            .await
        });
        ready.await.unwrap();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        drop(store);
        // A busy store must not consume a global queue or block independent stores.
        let independent = tokio::time::timeout(
            Duration::from_secs(5),
            Store::open(root.join("independent")),
        )
        .await;
        // This exceeds SQLite's existing busy timeout. An alias open must wait
        // before opening a connection, even after its predecessor's caller died.
        let alias = tokio::time::timeout(
            Duration::from_millis(2300),
            Store::open(root.join("alias/state")),
        )
        .await;
        release.send(()).unwrap();
        done.await.unwrap();
        assert!(independent.unwrap().is_ok());
        assert!(
            alias.is_err(),
            "alias open must remain queued, got {alias:?}"
        );
        let store = Store::open(real.join("state")).await.unwrap();
        assert!(store.drafts(None).await.unwrap().is_empty());
        let alias = Store::open(root.join("alias/state")).await.unwrap();
        assert!(alias.drafts(None).await.unwrap().is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn system_directory_alias_is_supported_but_leaf_symlinks_are_refused() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!("fp-alias-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let real = root.join("real");
        std::fs::create_dir(&real).unwrap();
        let alias = root.join("alias");
        symlink(&real, &alias).unwrap();
        let store = Store::open(alias.join("state")).await.unwrap();
        assert!(store.drafts(None).await.unwrap().is_empty());
        symlink(real.join("state"), alias.join("leaf-link")).unwrap();
        assert!(Store::open(alias.join("leaf-link")).await.is_err());
        let db = real.join("state/creations.sqlite3");
        std::fs::rename(&db, real.join("saved.sqlite3")).unwrap();
        symlink(real.join("saved.sqlite3"), &db).unwrap();
        assert!(Store::open(alias.join("state")).await.is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

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
                    "DROP TABLE host_selection; DROP TABLE paseo_batch_results; DROP TABLE paseo_batches; DROP TABLE paseo_continuations; DROP TABLE paseo_sessions; DROP TABLE paseo_workspaces; DROP TABLE recent_directories; DROP TABLE appearance; DROP TABLE workspace_pins; DROP TABLE drafts; DROP TABLE worktree_jobs; DROP TABLE workspaces; DROP TABLE projects; PRAGMA user_version=1;",
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
                    "DROP TABLE host_selection; DROP TABLE paseo_batch_results; DROP TABLE paseo_batches; DROP TABLE paseo_continuations; DROP TABLE paseo_sessions; DROP TABLE paseo_workspaces; DROP TABLE recent_directories; DROP TABLE appearance; DROP TABLE workspace_pins; DROP TABLE drafts; DROP TABLE worktree_jobs; PRAGMA user_version=2;",
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
        store.job(|db| { db.execute_batch("DROP TABLE host_selection; DROP TABLE paseo_batch_results; DROP TABLE paseo_batches; DROP TABLE paseo_continuations; DROP TABLE paseo_sessions; DROP TABLE paseo_workspaces; DROP TABLE recent_directories; DROP TABLE appearance; DROP TABLE workspace_pins; DROP TABLE drafts; DROP TABLE worktree_jobs; DROP TABLE workspaces; DROP TABLE projects; DROP TABLE creations; PRAGMA user_version=1;")?;Ok(()) }).await.unwrap();
        assert!(Store::open(directory.clone()).await.is_err());
        let db = Database::open(directory.join("creations.sqlite3")).unwrap();
        let version: u32 = db
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
