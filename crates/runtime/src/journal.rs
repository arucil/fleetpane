//! Durable mutation receipts. One bounded blocking task at a time from the Host loop.

use crate::{
    FaultScope, Operation, OperationEntry as Entry, OperationPage as Page, Outcome,
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
const SCHEMA: u32 = 3;
const MAX_PAGES: u32 = 65_536; // 256 MiB at the fixed 4096-byte page size.
const PAGE_SIZE: usize = 32;

fn archive_bounds(connection: &Connection) -> Result<(usize, usize), Error> {
    let (rows,bytes): (i64,i64) = connection.query_row(
        "SELECT count(*),coalesce(sum(length(CAST(data AS BLOB))+length(CAST(session AS BLOB))+length(CAST(parent AS BLOB))+length(CAST(id AS BLOB))),0) FROM acp_archive", [],
        |row| Ok((row.get(0)?,row.get(1)?))
    )?;
    if !(0..=64_064).contains(&rows) || !(0..=16 * 1024 * 1024).contains(&bytes) {
        return Err(
            "ACP archive exceeds 64064 rows or 16 MiB; no history was silently discarded".into(),
        );
    }
    Ok((rows.try_into()?, bytes.try_into()?))
}

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
    archive_rows: usize,
    archive_bytes: usize,
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
        let (id, initialize_admission, initialize_archive) = if let Some((version, id, previous)) =
            existing
        {
            if !matches!(version, 1 | 2 | SCHEMA) || previous != binding {
                return Err("journal schema or provider/workspace binding differs; use its original configuration or a separate socket".into());
            }
            (Uuid::parse_str(&id)?, version == 1, version < 3)
        } else {
            let id = Uuid::new_v4();
            transaction.execute(
                "INSERT INTO metadata VALUES (1,?1,?2,?3)",
                params![SCHEMA, id.to_string(), binding],
            )?;
            (id, true, true)
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
        if initialize_archive {
            transaction.execute_batch(
                "CREATE TABLE acp_archive (
                 position INTEGER PRIMARY KEY,
                 kind TEXT NOT NULL CHECK(kind IN ('session','turn','item')),
                 session TEXT NOT NULL, parent TEXT NOT NULL, id TEXT NOT NULL,
                 data TEXT NOT NULL, UNIQUE(kind,session,parent,id));",
            )?;
        }
        // A v3 store must already contain its archive, even when it is empty.
        let (archive_rows, archive_bytes) = archive_bounds(&transaction)?;
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
            archive_rows,
            archive_bytes,
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

    fn fail(&mut self, reason: &str, scope: FaultScope) -> Result<(), Error> {
        let transaction = self.connection.transaction()?;
        let pending = {
            let mut statement = transaction
                .prepare("SELECT id,method FROM operations WHERE host=?1 AND state='accepted'")?;
            statement
                .query_map([self.host.to_string()], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        let outcome = serde_json::to_string(&Outcome::Unknown(reason.to_owned()))?;
        for (id, method) in pending {
            if scope.includes(&method) {
                transaction.execute(
                    "UPDATE operations SET state='unknown',outcome=?1,completed_sequence=NULL WHERE id=?2 AND host=?3 AND state='accepted'",
                    params![outcome, id, self.host.to_string()],
                )?;
            }
        }
        transaction.commit()?;
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
    pub(crate) async fn archive_write(
        &self,
        rows: Vec<crate::acp::archive::Row>,
    ) -> Result<(), Error> {
        self.run(move |db| {
            let mut count=db.archive_rows;
            let mut bytes=db.archive_bytes;
            let transaction = db.connection.transaction()?;
            for row in rows {
                let data=serde_json::to_string(&row.data)?;
                let previous: Option<i64> = transaction.query_row(
                    "SELECT length(CAST(data AS BLOB)) FROM acp_archive WHERE kind=?1 AND session=?2 AND parent=?3 AND id=?4",
                    params![row.kind,row.session,row.parent,row.id], |row| row.get(0)
                ).optional()?;
                match previous {
                    Some(previous)=>bytes=bytes.checked_sub(usize::try_from(previous)?).ok_or("ACP archive size accounting mismatch")?,
                    None=>{count+=1;bytes+=row.session.len()+row.parent.len()+row.id.len();}
                }
                bytes+=data.len();
                if count > 64_064 || bytes > 16 * 1024 * 1024 {
                    return Err("ACP archive exceeds 64064 rows or 16 MiB; no history was silently discarded".into());
                }
                transaction.execute(
                    "INSERT INTO acp_archive(kind,session,parent,id,data) VALUES (?1,?2,?3,?4,?5)
                     ON CONFLICT(kind,session,parent,id) DO UPDATE SET data=excluded.data",
                    params![
                        row.kind,
                        row.session,
                        row.parent,
                        row.id,
                        data
                    ],
                )?;
            }
            transaction.commit()?;
            db.archive_rows=count;
            db.archive_bytes=bytes;
            Ok(())
        })
        .await
    }

    pub(crate) async fn archive_read(&self) -> Result<Vec<crate::acp::archive::Row>, Error> {
        self.run(|db| {
            archive_bounds(&db.connection)?;
            let mut statement = db.connection.prepare(
                "SELECT kind,session,parent,id,data FROM acp_archive
                 ORDER BY CASE kind WHEN 'session' THEN 0 WHEN 'turn' THEN 1 ELSE 2 END,position",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?;
            rows.map(|row| {
                let (kind, session, parent, id, data) = row?;
                Ok(crate::acp::archive::Row {
                    kind,
                    session,
                    parent,
                    id,
                    data: serde_json::from_str(&data)?,
                })
            })
            .collect()
        })
        .await
    }

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
    pub async fn fail(&self, reason: String, scope: FaultScope) -> Result<(), Error> {
        self.run(move |db| db.fail(&reason, scope)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    pub(super) struct Directory(pub(super) PathBuf);
    impl Directory {
        pub(super) fn new() -> Self {
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
    fn provider_failure_keeps_terminal_receipts_pending_until_their_own_result() {
        let directory = Directory::new();
        let mut db = Database::open(&directory.0, "synthetic", Uuid::new_v4()).unwrap();
        let mut terminal = operation();
        terminal.method = "fleetpane/terminal/input".into();
        let provider = operation();
        let mut future = operation();
        future.method = "fleetpane/terminal/future".into();
        for op in [&terminal, &provider, &future] {
            db.admit(op).unwrap();
        }
        db.fail("provider exited", FaultScope::Provider).unwrap();
        assert!(matches!(
            db.get(terminal.id).unwrap().unwrap().outcome,
            Outcome::Accepted
        ));
        for op in [provider, future] {
            assert!(matches!(
                db.get(op.id).unwrap().unwrap().outcome,
                Outcome::Unknown(_)
            ));
        }
        terminal.outcome = Outcome::Finished(json!({"id":terminal.id,"result":{}}));
        terminal.completed_sequence = Some(1);
        db.finish(&terminal).unwrap();
        let mut close = operation();
        close.method = "fleetpane/terminal/close".into();
        db.admit(&close).unwrap();
        db.fail("journal unavailable", FaultScope::Host).unwrap();
        assert!(matches!(
            db.get(close.id).unwrap().unwrap().outcome,
            Outcome::Unknown(_)
        ));
        assert_eq!(db.get(terminal.id).unwrap(), Some(terminal));
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
            .execute_batch(
                "DROP TABLE admission; DROP TABLE acp_archive; UPDATE metadata SET version=1",
            )
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

#[cfg(test)]
mod archive_tests {
    use super::*;
    use crate::acp::archive::Row;
    use serde_json::json;

    fn row(id: &str, text: &str) -> Row {
        Row {
            kind: "item".into(),
            session: "synthetic".into(),
            parent: "turn".into(),
            id: id.into(),
            data: json!({"id":id,"type":"acpMessage","text":text}),
        }
    }

    #[tokio::test]
    async fn archive_upserts_preserve_order_and_transaction_failures_preserve_rows_and_budget() {
        let directory = tests::Directory::new();
        let journal = Journal::open(directory.0.clone(), "synthetic".into(), Uuid::new_v4())
            .await
            .unwrap();
        journal
            .archive_write(vec![row("z", "中文"), row("a", "second")])
            .await
            .unwrap();
        journal
            .archive_write(vec![row("z", "changed 中文")])
            .await
            .unwrap();
        let rows = journal.archive_read().await.unwrap();
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["z", "a"]
        );
        assert_eq!(rows[0].data["text"], "changed 中文");
        journal.run(|db| {
            assert_eq!(archive_bounds(&db.connection)?,(db.archive_rows,db.archive_bytes));
            db.connection.execute_batch("CREATE TRIGGER fail_archive BEFORE INSERT ON acp_archive WHEN NEW.id='bad' BEGIN SELECT RAISE(FAIL,'synthetic archive failure'); END;")?;
            Ok(())
        }).await.unwrap();
        assert!(
            journal
                .archive_write(vec![row("z", "must roll back"), row("bad", "fail")])
                .await
                .is_err()
        );
        assert_eq!(
            journal.archive_read().await.unwrap()[0].data["text"],
            "changed 中文"
        );
        journal
            .run(|db| {
                assert_eq!(
                    archive_bounds(&db.connection)?,
                    (db.archive_rows, db.archive_bytes)
                );
                Ok(())
            })
            .await
            .unwrap();
        drop(journal);
        let journal = Journal::open(directory.0.clone(), "synthetic".into(), Uuid::new_v4())
            .await
            .unwrap();
        assert_eq!(journal.archive_read().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn v2_upgrade_keeps_admission_and_current_missing_malformed_or_oversize_archive_fails() {
        let directory = tests::Directory::new();
        let journal = Journal::open(directory.0.clone(), "synthetic".into(), Uuid::new_v4())
            .await
            .unwrap();
        let admission = journal.admission().await.unwrap();
        let draining = admission.change(0, Mode::Draining).unwrap();
        journal
            .set_admission(admission, draining.clone())
            .await
            .unwrap();
        journal
            .run(|db| {
                db.connection
                    .execute_batch("DROP TABLE acp_archive; UPDATE metadata SET version=2")?;
                Ok(())
            })
            .await
            .unwrap();
        drop(journal);
        let journal = Journal::open(directory.0.clone(), "synthetic".into(), Uuid::new_v4())
            .await
            .unwrap();
        assert_eq!(journal.admission().await.unwrap(), draining);
        assert!(journal.archive_read().await.unwrap().is_empty());
        journal
            .archive_write(vec![row("one", "valid")])
            .await
            .unwrap();
        journal
            .run(|db| {
                db.connection
                    .execute("UPDATE acp_archive SET data='{'", [])?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(journal.archive_read().await.is_err());
        journal
            .run(|db| {
                db.connection.execute(
                    "UPDATE acp_archive SET data=?1",
                    ["x".repeat(16 * 1024 * 1024)],
                )?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(journal.archive_read().await.is_err());
        journal
            .run(|db| {
                db.connection.execute_batch("DROP TABLE acp_archive")?;
                Ok(())
            })
            .await
            .unwrap();
        drop(journal);
        assert!(
            Journal::open(directory.0.clone(), "synthetic".into(), Uuid::new_v4())
                .await
                .is_err()
        );
    }
}
