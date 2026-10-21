//! Read-only Paseo discovery; explicit, idempotent local catalog imports.
mod batch;
pub mod continuation;
mod history;
mod scratch;
pub use batch::{
    BatchControl, BatchPage, BatchPlan, BatchProgress, BatchReceipt, BatchReport, BatchState,
    BatchSummary, Selection, WorkspaceResult,
};
pub use scratch::ScratchReport;
#[cfg(test)]
mod tests;
use crate::{
    client::Error,
    hosts::Endpoint,
    storage::Store,
    workspaces::{self, Kind, Registration},
};
pub use history::{HistoryPage, HistoryStatus, MigrationReport, Roots};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::OpenOptions,
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const MAX_FILE: u64 = 16 * 1024 * 1024;
const MAX_RECORDS: usize = 20_000;
fn invalid(e: impl std::fmt::Display) -> Error {
    Error::Input(format!("Paseo import: {e}"))
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceWorkspace {
    pub workspace_id: String,
    pub project_id: String,
    pub cwd: PathBuf,
    pub kind: String,
    pub display_name: String,
    pub title: Option<String>,
    pub branch: Option<String>,
    pub archived_at: Option<String>,
    pub pinned_at: Option<String>,
}
impl SourceWorkspace {
    pub fn name(&self) -> &str {
        self.title.as_deref().unwrap_or(&self.display_name)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: String,
    pub provider: String,
    pub cwd: PathBuf,
    pub workspace_id: Option<String>,
    pub title: Option<String>,
    pub last_status: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub last_activity_at: Option<String>,
    pub last_user_message_at: Option<String>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    pub archived_at: Option<String>,
    pub persistence: Option<Handle>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Handle {
    pub provider: String,
    pub session_id: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Candidate {
    pub source: SourceWorkspace,
    pub sessions: Vec<Agent>,
    pub revision: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Catalog {
    pub home: PathBuf,
    pub revision: String,
    pub candidates: Vec<Candidate>,
    pub unassociated_sessions: usize,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ImportedWorkspace {
    pub id: Uuid,
    pub source_home: PathBuf,
    pub source: SourceWorkspace,
    pub endpoint: Endpoint,
    pub workspace_id: Option<Uuid>,
    pub registration_error: Option<String>,
    pub session_count: usize,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Session {
    pub position: u64,
    pub id: Uuid,
    pub parent: Uuid,
    pub source_home: PathBuf,
    pub agent: Agent,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<(T, Vec<u8>), Error> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(path)
        .map_err(|e| invalid(format!("{}: {e}", path.display())))?;
    if !file.metadata().map_err(invalid)?.is_file() {
        return Err(invalid("source must be a regular JSON file"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(invalid)?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(invalid("source JSON exceeds 16 MiB"));
    }
    Ok((
        serde_json::from_slice(&bytes).map_err(|e| invalid(format!("{}: {e}", path.display())))?,
        bytes,
    ))
}

pub async fn discover(home: PathBuf) -> Result<Catalog, Error> {
    tokio::task::spawn_blocking(move || {
        if !home.is_absolute() {
            return Err(invalid("choose an absolute Paseo home directory"));
        }
        let home = std::fs::canonicalize(home).map_err(invalid)?;
        let (workspaces, bytes): (Vec<SourceWorkspace>, _) =
            read_json(&home.join("projects/workspaces.json"))?;
        if workspaces.len() > MAX_RECORDS {
            return Err(invalid("source exceeds 20,000 workspaces"));
        }
        let mut digest = Sha256::new();
        digest.update(&bytes);
        let mut ids = BTreeSet::new();
        let mut candidates = Vec::new();
        for source in workspaces {
            if source.workspace_id.is_empty()
                || !ids.insert(source.workspace_id.clone())
                || !source.cwd.is_absolute()
                || !matches!(
                    source.kind.as_str(),
                    "directory" | "worktree" | "local_checkout"
                )
            {
                return Err(invalid(
                    "duplicate workspace ID, relative cwd or unsupported workspace kind",
                ));
            }
            candidates.push(Candidate {
                source,
                sessions: Vec::new(),
                revision: String::new(),
            });
        }
        // Current and legacy storage: agents/<project-key>/<id>.json or agents/<id>.json.
        let mut paths = Vec::new();
        let agents_dir = home.join("agents");
        if std::fs::symlink_metadata(&agents_dir)
            .map_err(invalid)?
            .file_type()
            .is_symlink()
        {
            return Err(invalid("agent storage must not be a symlink"));
        }
        let mut directories = 0;
        for entry in std::fs::read_dir(&agents_dir).map_err(invalid)? {
            let entry = entry.map_err(invalid)?;
            let kind = entry.file_type().map_err(invalid)?;
            if kind.is_dir() {
                directories += 1;
                if directories > MAX_RECORDS {
                    return Err(invalid("source exceeds 20,000 agent directories"));
                }
                for child in std::fs::read_dir(entry.path()).map_err(invalid)? {
                    let child = child.map_err(invalid)?;
                    if child.file_type().map_err(invalid)?.is_symlink() {
                        return Err(invalid("symlink in agent storage"));
                    }
                    if child.file_type().map_err(invalid)?.is_file()
                        && child.path().extension().is_some_and(|v| v == "json")
                    {
                        paths.push(child.path());
                    }
                    if paths.len() > MAX_RECORDS {
                        return Err(invalid("source exceeds 20,000 session files"));
                    }
                }
            } else if kind.is_file() && entry.path().extension().is_some_and(|v| v == "json") {
                paths.push(entry.path());
            } else if kind.is_symlink() {
                return Err(invalid(
                    "symlink in agent storage; choose a regular exported source",
                ));
            }
            if paths.len() > MAX_RECORDS {
                return Err(invalid("source exceeds 20,000 session files"));
            }
        }
        paths.sort();
        let mut agents = BTreeSet::new();
        let mut orphaned = 0;
        let index: BTreeMap<_, _> = candidates
            .iter()
            .enumerate()
            .map(|(i, c)| (c.source.workspace_id.clone(), i))
            .collect();
        let mut source_bytes = bytes.len();
        for path in paths {
            let (agent, bytes): (Agent, _) = read_json(&path)?;
            source_bytes += bytes.len();
            if source_bytes > 128 * 1024 * 1024 {
                return Err(invalid("source metadata exceeds 128 MiB"));
            }
            digest.update(
                path.strip_prefix(&home)
                    .map_err(invalid)?
                    .as_os_str()
                    .as_encoded_bytes(),
            );
            digest.update([0]);
            digest.update(&bytes);
            if agent.id.is_empty() || !agents.insert(agent.id.clone()) || !agent.cwd.is_absolute() {
                return Err(invalid("duplicate session ID or invalid session cwd"));
            }
            let selected = match &agent.workspace_id {
                Some(id) => index.get(id).copied(),
                None => {
                    let matches: Vec<_> = candidates
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| c.source.cwd == agent.cwd)
                        .map(|(i, _)| i)
                        .collect();
                    if matches.len() == 1 {
                        Some(matches[0])
                    } else {
                        None
                    }
                }
            };
            if let Some(i) = selected {
                candidates[i].sessions.push(agent);
            } else {
                orphaned += 1;
            }
        }
        for candidate in &mut candidates {
            let bytes =
                serde_json::to_vec(&(&candidate.source, &candidate.sessions)).map_err(invalid)?;
            candidate.revision = format!("{:x}", Sha256::digest(bytes));
        }
        Ok(Catalog {
            home,
            revision: format!("{:x}", digest.finalize()),
            candidates,
            unassociated_sessions: orphaned,
        })
    })
    .await
    .map_err(invalid)?
}

impl Store {
    /// Register the selected directory in place and copy session identities in one
    /// SQLite transaction. No provider process, Git mutation, or source write.
    pub async fn import_paseo_workspace(
        &self,
        catalog: &Catalog,
        source_id: &str,
        endpoint: Endpoint,
    ) -> Result<ImportedWorkspace, Error> {
        crate::hosts::Configuration::single(endpoint.socket.clone()).map_err(invalid)?;
        let expected = catalog
            .candidates
            .iter()
            .find(|c| c.source.workspace_id == source_id)
            .ok_or_else(|| invalid("workspace absent from preview"))?;
        let fresh = discover(catalog.home.clone()).await?;
        let candidate = fresh
            .candidates
            .into_iter()
            .find(|c| c.source.workspace_id == source_id)
            .ok_or_else(|| invalid("workspace absent from preview"))?;
        if candidate.revision != expected.revision {
            return Err(invalid(
                "Paseo workspace changed; preview again before importing",
            ));
        }
        self.save_paseo_candidate(catalog.home.clone(), candidate, endpoint)
            .await
            .map(|(workspace, _)| workspace)
    }

    async fn save_paseo_candidate(
        &self,
        home: PathBuf,
        candidate: Candidate,
        endpoint: Endpoint,
    ) -> Result<(ImportedWorkspace, bool), Error> {
        let kind = if candidate.source.kind == "directory" {
            Kind::Folder
        } else {
            Kind::Git
        };
        let (registration, registration_error) =
            match workspaces::inspect(candidate.source.cwd.clone(), kind).await {
                Ok((directory, project, _)) => (
                    Some(Registration {
                        endpoint: endpoint.clone(),
                        directory,
                        project,
                        kind,
                    }),
                    None,
                ),
                Err(e) => (None, Some(e.to_string())),
            };
        self.job(move|db| {
            let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let source=home.to_str().ok_or("non-UTF-8 source home")?;let socket=endpoint.socket.to_str().ok_or("non-UTF-8 endpoint")?.to_owned();
            let prior:Option<String>=tx.query_row("SELECT id FROM paseo_workspaces WHERE source=?1 AND source_id=?2 AND endpoint=?3",params![source,candidate.source.workspace_id,socket],|r|r.get(0)).optional()?;
            if let Some(id)=prior {
                let saved:String=tx.query_row("SELECT data FROM paseo_workspaces WHERE id=?1",[id],|r|r.get(0))?;
                return Ok((serde_json::from_str(&saved)?, true));
            }
            let workspace=registration.as_ref().map(|r|workspaces::save_registration(&tx,r)).transpose()?;
            let imported=ImportedWorkspace{id:Uuid::new_v4(),source_home:home.clone(),source:candidate.source,endpoint,workspace_id:workspace.map(|w|w.id),registration_error,session_count:candidate.sessions.len()};
            tx.execute("INSERT INTO paseo_workspaces(id,source,source_id,endpoint,workspace,data) VALUES(?1,?2,?3,?4,?5,?6)",params![imported.id.to_string(),source,imported.source.workspace_id,socket,imported.workspace_id.map(|id|id.to_string()),serde_json::to_string(&imported)?])?;
            for agent in candidate.sessions {
                let session=Session{position:0,id:Uuid::new_v4(),parent:imported.id,source_home:home.clone(),agent};
                tx.execute("INSERT INTO paseo_sessions(id,parent,source_id,data) VALUES(?1,?2,?3,?4)",params![session.id.to_string(),imported.id.to_string(),session.agent.id,serde_json::to_string(&session)?])?;
            }
            tx.commit()?;Ok((imported, false))
        }).await
    }
    pub async fn paseo_workspaces(&self) -> Result<Vec<ImportedWorkspace>, Error> {
        self.job(|db| {
            let mut q =
                db.prepare("SELECT data FROM paseo_workspaces ORDER BY rowid DESC LIMIT 20000")?;
            q.query_map([], |r| r.get::<_, String>(0))?
                .map(|r| Ok(serde_json::from_str(&r?)?))
                .collect()
        })
        .await
    }
    pub async fn paseo_workspace(&self, id: Uuid) -> Result<ImportedWorkspace, Error> {
        self.job(move |db| {
            let data: String = db.query_row(
                "SELECT data FROM paseo_workspaces WHERE id=?1",
                [id.to_string()],
                |r| r.get(0),
            )?;
            Ok(serde_json::from_str(&data)?)
        })
        .await
    }
    pub async fn paseo_sessions(
        &self,
        parent: Uuid,
        before: Option<u64>,
    ) -> Result<Vec<Session>, Error> {
        let before = i64::try_from(before.unwrap_or(i64::MAX as u64)).map_err(invalid)?;
        self.job(move|db| {let mut q=db.prepare("SELECT position,data FROM paseo_sessions WHERE parent=?1 AND position<?2 ORDER BY position DESC LIMIT 32")?;q.query_map(params![parent.to_string(),before],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?)))?.map(|r|{let(p,s)=r?;let mut v:Session=serde_json::from_str(&s)?;v.position=p.try_into()?;Ok(v)}).collect()}).await
    }
    pub async fn paseo_session(&self, id: Uuid) -> Result<Session, Error> {
        self.job(move |db| {
            let (p, s): (i64, String) = db.query_row(
                "SELECT position,data FROM paseo_sessions WHERE id=?1",
                [id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let mut v: Session = serde_json::from_str(&s)?;
            v.position = p.try_into()?;
            Ok(v)
        })
        .await
    }
}
