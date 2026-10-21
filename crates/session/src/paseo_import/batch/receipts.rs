//! Durable per-workspace checkpoints. Reads never resume work. A new attempt
//! revalidates the saved plan and gets its own receipt; prior reports stay intact.
use super::{BatchControl, BatchPlan, BatchProgress, BatchReport, WorkspaceResult};
use crate::{
    client::Error,
    storage::{StorageError, Store},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};
use uuid::Uuid;

const MAX_RECORD: usize = 4 * 1024 * 1024;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchState {
    Running,
    Completed,
    Stopped,
    Failed,
}
impl BatchState {
    pub fn label(self, writer_active: bool) -> &'static str {
        match self {
            Self::Running if writer_active => "Running in another worker",
            _ if writer_active => "Retry in progress; reload attempts",
            Self::Running => "Interrupted; completion unconfirmed",
            Self::Completed => "Finished checking selection",
            Self::Stopped => "Stopped after current copy",
            Self::Failed => "Failed; saved progress retained",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BatchSummary {
    pub id: Uuid,
    pub position: u64,
    pub state: BatchState,
    pub writer_active: bool,
    pub progress: BatchProgress,
    pub error: Option<String>,
    pub source: std::path::PathBuf,
    pub target: String,
}

#[derive(Debug, Serialize)]
pub struct BatchReceipt {
    pub summary: BatchSummary,
    pub plan: BatchPlan,
}

#[derive(Debug, Serialize)]
pub struct BatchPage {
    pub summary: BatchSummary,
    pub results: Vec<WorkspaceResult>,
    pub next: Option<u64>,
}

fn encode(value: &impl Serialize) -> Result<String, StorageError> {
    let text = serde_json::to_string(value)?;
    if text.len() > MAX_RECORD {
        return Err("batch record exceeds 4 MiB; receipt was not advanced".into());
    }
    Ok(text)
}

// Never unlink this inode. flock releases on process death; PIDs/elapsed time
// do not establish whether another worker is still writing.
fn lock(root: &Path, id: Uuid) -> Result<File, StorageError> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK)
                .bits()
                .try_into()?,
        )
        .open(root.join(format!("paseo-batch-{id}.lock")))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("batch worker lock must be a private regular file".into());
    }
    Ok(file)
}

fn writer_lock(root: &Path, id: Uuid) -> Result<Option<File>, StorageError> {
    let file = lock(root, id)?;
    match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(Some(file)),
        Err(rustix::io::Errno::WOULDBLOCK) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn receipt(db: &Connection, root: &Path, id: Uuid) -> Result<BatchReceipt, StorageError> {
    let (position, plan, state, progress, error): (i64, String, String, String, Option<String>) = db.query_row(
        "SELECT position,CASE WHEN length(CAST(plan AS BLOB))<=4194304 THEN plan ELSE NULL END,state,CASE WHEN length(CAST(progress AS BLOB))<=4194304 THEN progress ELSE NULL END,CASE WHEN length(CAST(error AS BLOB))>4194304 THEN X'00' ELSE error END FROM paseo_batches WHERE id=?1", [id.to_string()],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)),
    )?;
    let plan: BatchPlan = serde_json::from_str(&plan)?;
    plan.validate()?;
    let state: BatchState = serde_json::from_value(serde_json::Value::String(state))?;
    let writer_active = writer_lock(root, id)?.is_none();
    Ok(BatchReceipt {
        summary: BatchSummary {
            id,
            position: position.try_into()?,
            state,
            writer_active,
            progress: serde_json::from_str(&progress)?,
            error,
            source: plan.home.clone(),
            target: plan.endpoint.name.clone(),
        },
        plan,
    })
}

impl Store {
    pub(super) async fn begin_paseo_batch(&self, plan: BatchPlan) -> Result<(Uuid, File), Error> {
        let root = self.root().to_owned();
        self.job(move |db| {
            let id = Uuid::new_v4();
            let file = writer_lock(&root, id)?.ok_or("batch UUID already has a writer")?;
            let progress = BatchProgress {
                workspaces_total: plan.workspaces.len(),
                ..Default::default()
            };
            db.execute(
                "INSERT INTO paseo_batches(id,plan,state,progress) VALUES(?1,?2,'running',?3)",
                params![id.to_string(), encode(&plan)?, encode(&progress)?],
            )?;
            Ok((id, file))
        })
        .await
    }

    pub(super) async fn checkpoint_paseo_batch(
        &self,
        id: Uuid,
        progress: &BatchProgress,
        result: Option<(usize, &WorkspaceResult)>,
    ) -> Result<(), Error> {
        let progress = progress.clone();
        let result = result.map(|(ordinal, result)| (ordinal, result.clone()));
        self.job(move |db| {
            let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if tx.execute("UPDATE paseo_batches SET progress=?1 WHERE id=?2 AND state='running'",params![encode(&progress)?,id.to_string()])?!=1 { return Err("batch receipt is no longer running".into()); }
            if let Some((ordinal,result))=result {
                tx.execute("INSERT INTO paseo_batch_results(batch,ordinal,data) VALUES(?1,?2,?3) ON CONFLICT(batch,ordinal) DO UPDATE SET data=excluded.data",params![id.to_string(),i64::try_from(ordinal)?,encode(&result)?])?;
            }
            tx.commit()?;Ok(())
        }).await
    }

    pub(super) async fn finish_paseo_batch(&self, report: &BatchReport) -> Result<(), Error> {
        let id = report.batch_id;
        let progress = report.progress.clone();
        let state = if report.cancelled {
            "stopped"
        } else {
            "completed"
        };
        self.job(move |db| {
            if db.execute(
                "UPDATE paseo_batches SET progress=?1,state=?2 WHERE id=?3 AND state='running'",
                params![encode(&progress)?, state, id.to_string()],
            )? != 1
            {
                return Err("batch receipt is no longer running".into());
            }
            Ok(())
        })
        .await
    }

    pub(super) async fn fail_paseo_batch(&self, id: Uuid, error: String) -> Result<(), Error> {
        self.job(move |db| {
            if error.len() > MAX_RECORD {
                return Err("batch failure exceeds 4 MiB; last checkpoint retained".into());
            }
            if db.execute(
                "UPDATE paseo_batches SET state='failed',error=?1 WHERE id=?2 AND state='running'",
                params![error, id.to_string()],
            )? != 1
            {
                return Err("batch receipt is no longer running".into());
            }
            Ok(())
        })
        .await
    }

    pub async fn paseo_batch(&self, id: Uuid) -> Result<BatchReceipt, Error> {
        let root = self.root().to_owned();
        self.job(move |db| receipt(db, &root, id)).await
    }

    /// Newest 32 summaries. Transcript/results stay in separately bounded pages.
    pub async fn paseo_batches(&self, before: Option<u64>) -> Result<Vec<BatchSummary>, Error> {
        let root = self.root().to_owned();
        self.job(move |db| {
            let mut q = db.prepare(
                "SELECT id FROM paseo_batches WHERE position<?1 ORDER BY position DESC LIMIT 32",
            )?;
            let ids = q
                .query_map(
                    [before.map(i64::try_from).transpose()?.unwrap_or(i64::MAX)],
                    |r| r.get::<_, String>(0),
                )?
                .collect::<Result<Vec<_>, _>>()?;
            ids.into_iter()
                .map(|id| Ok(receipt(db, &root, id.parse()?)?.summary))
                .collect()
        })
        .await
    }

    pub async fn paseo_batch_results(
        &self,
        id: Uuid,
        after: Option<u64>,
    ) -> Result<BatchPage, Error> {
        let root = self.root().to_owned();
        self.job(move |db| {
            let tx=db.transaction()?;
            let summary=receipt(&tx,&root,id)?.summary;
            let mut results=Vec::new(); let mut last=None;let mut bytes=0;
            {
                let mut q=tx.prepare("SELECT ordinal,CASE WHEN length(CAST(data AS BLOB))<=4194304 THEN data ELSE NULL END FROM paseo_batch_results WHERE batch=?1 AND ordinal>?2 ORDER BY ordinal LIMIT 32")?;
                for row in q.query_map(params![id.to_string(),after.map(i64::try_from).transpose()?.unwrap_or(-1)],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?)))? {
                    let (ordinal,data)=row?;
                    if !results.is_empty() && bytes+data.len()>256*1024 {break;}
                    bytes+=data.len();last=Some(ordinal);results.push(serde_json::from_str(&data)?);
                }
            }
            let next=match last {
                Some(last) if tx.query_row("SELECT ordinal FROM paseo_batch_results WHERE batch=?1 AND ordinal>?2 LIMIT 1",params![id.to_string(),last],|r|r.get::<_,i64>(0)).optional()?.is_some()=>Some(last.try_into()?),
                _=>None,
            };
            tx.commit()?;Ok(BatchPage{summary,results,next})
        }).await
    }

    /// Explicit retry creates a new receipt; immutable archives/import identities
    /// make repeated traversal safe. Never attach to or cancel another writer.
    pub async fn retry_paseo_batch(
        &self,
        id: Uuid,
        control: BatchControl,
    ) -> Result<BatchReport, Error> {
        let store = self.clone();
        tokio::spawn(async move {
            let root = store.root().to_owned();
            let (saved, _previous_writer) = store
                .job(move |db| {
                    let file = writer_lock(&root, id)?
                        .ok_or("batch still has an active worker; reload its progress")?;
                    Ok((receipt(db, &root, id)?, file))
                })
                .await?;
            store.execute_paseo_batch(saved.plan, control).await
        })
        .await
        .map_err(super::invalid)?
    }
}
