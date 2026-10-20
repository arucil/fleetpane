//! Durable mutation receipts. One bounded blocking task at a time from the Host loop.

use crate::{
    Operation, OperationEntry as Entry, OperationPage as Page, Outcome,
    lifecycle::{Admission, Mode},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use uuid::Uuid;

type Error = Box<dyn std::error::Error + Send + Sync>;
const SCHEMA: u32 = 2;
const MAX_PAGES: u32 = 65_536; // 256 MiB at the fixed 4096-byte page size.
const PAGE_SIZE: usize = 32;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    journal: Uuid,
    before: u64,
}

struct Database {
    connection: Connection,
    _lock: File,
    id: Uuid,
    host: Uuid,
}

/// The Host awaits each task; neither an unbounded worker queue nor filesystem
/// work on the Tokio/GPUI thread is introduced.
pub struct Journal(Arc<Mutex<Database>>);

fn private_file(path: &Path) -> Result<File, Error> {
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = std::fs::symlink_metadata(path)?;
            if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
                return Err("journal files must be private regular files (0600)".into());
            }
            OpenOptions::new().read(true).write(true).open(path)?
        }
        Err(error) => return Err(error.into()),
    };
    Ok(file)
}

impl Database {
    fn admission(&self) -> Result<Admission, Error> {
        Self::read_admission(&self.connection)
    }

    fn read_admission(connection: &Connection) -> Result<Admission, Error> {
        let (generation, mode): (i64, String) = connection.query_row(
            "SELECT generation,mode FROM admission WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let mode = match mode.as_str() {
            "ready" => Mode::Ready,
            "draining" => Mode::Draining,
            _ => return Err("invalid durable admission mode".into()),
        };
        Ok(Admission {
            generation: generation.try_into()?,
            mode,
        })
    }

    fn set_admission(&mut self, old: &Admission, next: &Admission) -> Result<(), Error> {
        let mode = |mode| match mode {
            Mode::Ready => "ready",
            Mode::Draining => "draining",
        };
        let changed = self.connection.execute(
            "UPDATE admission SET generation=?1,mode=?2 WHERE singleton=1 AND generation=?3 AND mode=?4",
            params![i64::try_from(next.generation)?,mode(next.mode),i64::try_from(old.generation)?,mode(old.mode)])?;
        if changed != 1 {
            return Err("durable admission compare-and-swap failed".into());
        }
        Ok(())
    }

    fn open(directory: &Path, binding: &str, host: Uuid) -> Result<Self, Error> {
        crate::unix::prepare_directory(directory)?;
        let lock = private_file(&directory.join("owner.lock"))?;
        lock.try_lock()
            .map_err(|error| format!("journal already owned or lock failed: {error}"))?;
        let path = directory.join("operations.sqlite3");
        let file = private_file(&path)?;
        let rollback = directory.join("operations.sqlite3-journal");
        match std::fs::symlink_metadata(&rollback) {
            Ok(metadata) if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 => {
                return Err("rollback journal must be a private regular file".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        file.sync_all()?;
        File::open(directory)?.sync_all()?;
        let mut connection = Connection::open(&path)?;
        connection.busy_timeout(Duration::ZERO)?;
        connection.pragma_update(None, "page_size", 4096)?;
        let page_bytes: u32 = connection.query_row("PRAGMA page_size", [], |row| row.get(0))?;
        if page_bytes != 4096 {
            return Err("unsupported journal page size".into());
        }
        connection.pragma_update(None, "max_page_count", MAX_PAGES)?;
        connection.pragma_update(None, "cache_size", -2048)?;
        connection.pragma_update(None, "mmap_size", 0)?;
        connection.pragma_update(None, "journal_mode", "DELETE")?;
        connection.pragma_update(None, "synchronous", "EXTRA")?;
        let check: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if check != "ok" {
            return Err(format!("journal integrity check failed: {check}").into());
        }
        let transaction = connection.transaction()?;
        transaction.execute_batch(
            "CREATE TABLE IF NOT EXISTS metadata (
                singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                version INTEGER NOT NULL, id TEXT NOT NULL, binding TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS operations (
                position INTEGER PRIMARY KEY, id TEXT UNIQUE NOT NULL,
                host TEXT NOT NULL, method TEXT NOT NULL, params TEXT NOT NULL,
                state TEXT NOT NULL CHECK(state IN ('accepted','finished','unknown')),
                outcome TEXT NOT NULL, completed_sequence INTEGER);
             CREATE INDEX IF NOT EXISTS operation_state ON operations(state);",
        )?;
        let existing: Option<(u32, String, String)> = transaction
            .query_row(
                "SELECT version,id,binding FROM metadata WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let (id, initialize_admission) = if let Some((version, id, previous)) = existing {
            if !matches!(version, 1 | SCHEMA) || previous != binding {
                return Err("journal schema or provider/workspace binding differs; use its original configuration or a separate socket".into());
            }
            (Uuid::parse_str(&id)?, version == 1)
        } else {
            let id = Uuid::new_v4();
            transaction.execute(
                "INSERT INTO metadata VALUES (1,?1,?2,?3)",
                params![SCHEMA, id.to_string(), binding],
            )?;
            (id, true)
        };
        if initialize_admission {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS admission (
                singleton INTEGER PRIMARY KEY CHECK(singleton=1), generation INTEGER NOT NULL,
                mode TEXT NOT NULL CHECK(mode IN ('ready','draining')));
                INSERT OR IGNORE INTO admission VALUES (1,0,'ready');",
            )?;
        }
        // A current journal must already have its fence. Missing or invalid
        // state is corruption, never permission to reopen mutation admission.
        Self::read_admission(&transaction)?;
        transaction.execute("UPDATE metadata SET version=?1 WHERE singleton=1", [SCHEMA])?;
        // No replay: a committed admission does not prove whether the provider
        // received it, nor whether a side effect happened before the crash.
        transaction.execute(
            "UPDATE operations SET state='unknown',outcome=?1,completed_sequence=NULL WHERE state='accepted'",
            [serde_json::to_string(&Outcome::Unknown("Host stopped before a durable result; inspect the provider before any manual retry".into()))?],
        )?;
        transaction.commit()?;
        Ok(Self {
            connection,
            _lock: lock,
            id,
            host,
        })
    }

    fn get(&self, id: Uuid) -> Result<Option<Operation>, Error> {
        let raw: Option<(String, String, String, String, Option<i64>)> = self
            .connection
            .query_row(
                "SELECT host,method,params,outcome,completed_sequence FROM operations WHERE id=?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;
        raw.map(|(host, method, params, outcome, sequence)| {
            Ok(Operation {
                id,
                method,
                params: serde_json::from_str(&params)?,
                outcome: serde_json::from_str(&outcome)?,
                // Old event watermarks cannot order a new Host's projection.
                completed_sequence: if host == self.host.to_string() {
                    sequence.map(u64::try_from).transpose()?
                } else {
                    None
                },
            })
        })
        .transpose()
    }

    fn admit(&mut self, operation: &Operation) -> Result<u64, Error> {
        self.connection.execute(
            "INSERT INTO operations(id,host,method,params,state,outcome) VALUES (?1,?2,?3,?4,'accepted',?5)",
            params![operation.id.to_string(), self.host.to_string(), operation.method,
                serde_json::to_string(&operation.params)?, serde_json::to_string(&Outcome::Accepted)?],
        )?;
        Ok(self.connection.last_insert_rowid().try_into()?)
    }

    fn finish(&mut self, operation: &Operation) -> Result<(), Error> {
        let changed = self.connection.execute(
            "UPDATE operations SET state='finished',outcome=?1,completed_sequence=?2 WHERE id=?3 AND host=?4 AND state='accepted'",
            params![serde_json::to_string(&operation.outcome)?, operation.completed_sequence.map(i64::try_from).transpose()?,
                operation.id.to_string(), self.host.to_string()],
        )?;
        if changed != 1 {
            return Err("journal completion identity/state mismatch".into());
        }
        Ok(())
    }

    fn page(&self, cursor: Option<String>) -> Result<Page, Error> {
        let before = match cursor {
            Some(cursor) => {
                if cursor.len() > 256 {
                    return Err("journal cursor exceeds limit".into());
                }
                let cursor: Cursor = serde_json::from_str(&cursor)?;
                if cursor.journal != self.id
                    || cursor.before == 0
                    || cursor.before > i64::MAX as u64
                {
                    return Err("cursor belongs to a different journal or invalid position".into());
                }
                cursor.before
            }
            None => i64::MAX as u64,
        };
        let mut statement = self.connection.prepare(
            "SELECT position,id,host,method,state FROM operations WHERE position<?1 ORDER BY position DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(
            params![i64::try_from(before)?, i64::try_from(PAGE_SIZE + 1)?],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )?;
        let mut entries = Vec::new();
        for row in rows {
            let (position, id, host, method, state) = row?;
            entries.push(Entry {
                position: position.try_into()?,
                id: Uuid::parse_str(&id)?,
                host_id: Uuid::parse_str(&host)?,
                method,
                state,
            });
        }
        let more = entries.len() > PAGE_SIZE;
        entries.truncate(PAGE_SIZE);
        let next_cursor = if more {
            Some(serde_json::to_string(&Cursor {
                journal: self.id,
                before: entries.last().expect("full page").position,
            })?)
        } else {
            None
        };
        Ok(Page {
            entries,
            next_cursor,
        })
    }
}

impl Journal {
    pub async fn open(directory: PathBuf, binding: String, host: Uuid) -> Result<Self, Error> {
        let database =
            tokio::task::spawn_blocking(move || Database::open(&directory, &binding, host))
                .await??;
        Ok(Self(Arc::new(Mutex::new(database))))
    }

    async fn run<T: Send + 'static>(
        &self,
        action: impl FnOnce(&mut Database) -> Result<T, Error> + Send + 'static,
    ) -> Result<T, Error> {
        let database = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let mut database = database
                .lock()
                .map_err(|_| "journal worker lock poisoned")?;
            action(&mut database)
        })
        .await?
    }

    pub async fn get(&self, id: Uuid) -> Result<Option<Operation>, Error> {
        self.run(move |db| db.get(id)).await
    }
    pub async fn admit(&self, operation: Operation) -> Result<u64, Error> {
        self.run(move |db| db.admit(&operation)).await
    }
    pub async fn finish(&self, operation: Operation) -> Result<(), Error> {
        self.run(move |db| db.finish(&operation)).await
    }
    pub async fn page(&self, cursor: Option<String>) -> Result<Page, Error> {
        self.run(move |db| db.page(cursor)).await
    }
    pub async fn total(&self) -> Result<u64, Error> {
        self.run(|db| {
            Ok(u64::try_from(db.connection.query_row(
                "SELECT count(*) FROM operations",
                [],
                |row| row.get::<_, i64>(0),
            )?)?)
        })
        .await
    }
    pub async fn admission(&self) -> Result<Admission, Error> {
        self.run(|db| db.admission()).await
    }
    pub async fn set_admission(&self, old: Admission, next: Admission) -> Result<(), Error> {
        self.run(move |db| db.set_admission(&old, &next)).await
    }
    pub async fn unknown_count(&self) -> Result<u64, Error> {
        self.run(|db| {
            Ok(u64::try_from(db.connection.query_row(
                "SELECT count(*) FROM operations WHERE state='unknown'",
                [],
                |row| row.get::<_, i64>(0),
            )?)?)
        })
        .await
    }
    pub async fn fail(&self, reason: String) -> Result<(), Error> {
        self.run(move |db| {
            db.connection.execute("UPDATE operations SET state='unknown',outcome=?1,completed_sequence=NULL WHERE state='accepted'",
                [serde_json::to_string(&Outcome::Unknown(reason))?])?;
            Ok(())
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("fleetpane-journal-test-{}", Uuid::new_v4()));
            crate::unix::prepare_directory(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn operation() -> Operation {
        Operation {
            id: Uuid::new_v4(),
            method: "fixture/mutate".into(),
            params: json!({"nonce":"one"}),
            outcome: Outcome::Accepted,
            completed_sequence: None,
        }
    }

    #[test]
    fn restart_recovers_unknown_and_keeps_finished_receipt_without_old_watermark() {
        let directory = Directory::new();
        let host = Uuid::new_v4();
        let mut db = Database::open(&directory.0, "synthetic-provider/workspace", host).unwrap();
        assert!(Database::open(&directory.0, "synthetic-provider/workspace", host).is_err());
        let pending = operation();
        let mut finished = operation();
        db.admit(&pending).unwrap();
        db.admit(&finished).unwrap();
        finished.outcome = Outcome::Finished(json!({"id":finished.id,"result":{"ok":true}}));
        finished.completed_sequence = Some(17);
        db.finish(&finished).unwrap();
        assert_eq!(db.get(finished.id).unwrap(), Some(finished.clone()));
        assert!(
            db.finish(&finished).is_err(),
            "completion CAS must reject a second result"
        );
        drop(db);
        assert!(Database::open(&directory.0, "other-provider/workspace", Uuid::new_v4()).is_err());
        let db =
            Database::open(&directory.0, "synthetic-provider/workspace", Uuid::new_v4()).unwrap();
        let uncertain = db.get(pending.id).unwrap().unwrap();
        assert!(matches!(uncertain.outcome, Outcome::Unknown(_)));
        finished.completed_sequence = None;
        assert_eq!(db.get(finished.id).unwrap(), Some(finished));
    }

    #[test]
    fn disk_budget_failures_never_publish_admission_or_completion() {
        let directory = Directory::new();
        let mut db = Database::open(&directory.0, "synthetic", Uuid::new_v4()).unwrap();
        let mut accepted = operation();
        db.admit(&accepted).unwrap();
        let pages: u32 = db
            .connection
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap();
        db.connection
            .pragma_update(None, "max_page_count", pages)
            .unwrap();
        let oversized = Operation {
            params: json!({"text":"x".repeat(100_000)}),
            ..operation()
        };
        assert!(db.admit(&oversized).is_err());
        assert!(db.get(oversized.id).unwrap().is_none());
        accepted.outcome = Outcome::Finished(json!({"result":"y".repeat(100_000)}));
        accepted.completed_sequence = Some(1);
        assert!(db.finish(&accepted).is_err());
        assert_eq!(
            db.get(accepted.id).unwrap().unwrap().outcome,
            Outcome::Accepted
        );
        drop(db);
        let db = Database::open(&directory.0, "synthetic", Uuid::new_v4()).unwrap();
        assert!(matches!(
            db.get(accepted.id).unwrap().unwrap().outcome,
            Outcome::Unknown(_)
        ));
    }

    #[test]
    fn corruption_version_and_unsafe_files_are_rejected() {
        let directory = Directory::new();
        let db = Database::open(&directory.0, "synthetic", Uuid::new_v4()).unwrap();
        db.connection
            .execute("UPDATE metadata SET version=99", [])
            .unwrap();
        drop(db);
        assert!(Database::open(&directory.0, "synthetic", Uuid::new_v4()).is_err());
        std::fs::write(
            directory.0.join("operations.sqlite3"),
            b"corrupt synthetic database",
        )
        .unwrap();
        assert!(Database::open(&directory.0, "synthetic", Uuid::new_v4()).is_err());
        std::fs::set_permissions(
            directory.0.join("owner.lock"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(Database::open(&directory.0, "synthetic", Uuid::new_v4()).is_err());
    }

    #[test]
    fn missing_or_invalid_saved_admission_never_defaults_to_ready() {
        for damage in [
            "DROP TABLE admission",
            "DELETE FROM admission",
            "UPDATE admission SET generation=-1",
        ] {
            let directory = Directory::new();
            let mut db = Database::open(&directory.0, "synthetic", Uuid::new_v4()).unwrap();
            let old = db.admission().unwrap();
            db.set_admission(&old, &old.change(0, Mode::Draining).unwrap())
                .unwrap();
            db.connection.execute_batch(damage).unwrap();
            drop(db);
            assert!(
                Database::open(&directory.0, "synthetic", Uuid::new_v4()).is_err(),
                "saved admission was silently repaired after {damage}"
            );
        }
    }

    #[test]
    fn admission_migrates_v1_and_survives_restarts_without_reopening_writes() {
        let directory = Directory::new();
        let mut db = Database::open(&directory.0, "synthetic", Uuid::new_v4()).unwrap();
        // Recreate the previous journal layout, then exercise the real migration.
        db.connection
            .execute_batch("DROP TABLE admission; UPDATE metadata SET version=1")
            .unwrap();
        let operation = operation();
        db.admit(&operation).unwrap();
        drop(db);
        let mut db = Database::open(&directory.0, "synthetic", Uuid::new_v4()).unwrap();
        assert_eq!(db.admission().unwrap(), Admission::default());
        assert!(matches!(
            db.get(operation.id).unwrap().unwrap().outcome,
            Outcome::Unknown(_)
        ));
        let old = db.admission().unwrap();
        let drained = old.change(old.generation, Mode::Draining).unwrap();
        db.set_admission(&old, &drained).unwrap();
        assert!(db.set_admission(&old, &old).is_err());
        drop(db);
        let mut db = Database::open(&directory.0, "synthetic", Uuid::new_v4()).unwrap();
        assert_eq!(db.admission().unwrap(), drained);
        let ready = drained.change(1, Mode::Ready).unwrap();
        db.set_admission(&drained, &ready).unwrap();
        assert_eq!(db.admission().unwrap().generation, 2);
    }
}
