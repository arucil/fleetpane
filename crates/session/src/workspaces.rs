//! Existing local workspaces, independent of Git branch/remote and provider PIDs.
use crate::{
    client::{Command, Completed, Connection, Error},
    hosts::Endpoint,
    storage::{StorageError, Store},
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncReadExt};
use uuid::Uuid;

const MAX_OUTPUT: usize = 256 * 1024;
const MAX_CHANGES: usize = 500;

fn invalid(error: impl std::fmt::Display) -> Error {
    Error::Input(format!("Workspace: {error}"))
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Folder,
    Git,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Directory {
    pub path: PathBuf,
    pub device: u64,
    pub inode: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub endpoint: Endpoint,
    pub directory: Directory,
    pub project: Directory,
    pub kind: Kind,
}

#[derive(Clone, Debug, Serialize)]
pub struct Workspace {
    pub position: u64,
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: crate::workspace_names::Saved,
    pub project_name: crate::workspace_names::Saved,
    #[serde(flatten)]
    pub registration: Registration,
}

#[derive(Clone, Debug, Serialize)]
pub struct Change {
    pub status: String,
    pub path: String,
    pub original_path: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Git {
    pub root: PathBuf,
    pub git_directory: PathBuf,
    pub common_directory: PathBuf,
    pub linked_worktree: bool,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub upstream: Option<String>,
    pub changes: Vec<Change>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Overview {
    pub workspace: Workspace,
    pub git: Option<Git>,
}

pub(crate) async fn directory(path: PathBuf) -> Result<Directory, Error> {
    if !path.is_absolute() || path.as_os_str().len() > 4096 {
        return Err(invalid("choose an absolute directory within 4 KiB"));
    }
    tokio::task::spawn_blocking(move || {
        let path = std::fs::canonicalize(path).map_err(invalid)?;
        if path.to_str().is_none() {
            return Err(invalid("non-UTF-8 directory paths are not supported"));
        }
        let metadata = std::fs::metadata(&path).map_err(invalid)?;
        if !metadata.is_dir() {
            return Err(invalid("path is not a directory"));
        }
        Ok(Directory {
            path,
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    })
    .await
    .map_err(invalid)?
}

pub(crate) async fn bounded(mut stream: impl AsyncRead + Unpin) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    (&mut stream)
        .take((MAX_OUTPUT + 1) as u64)
        .read_to_end(&mut output)
        .await
        .map_err(invalid)?;
    if output.len() > MAX_OUTPUT {
        return Err(invalid(
            "Git output exceeds 256 KiB; narrow the workspace or inspect with Git directly",
        ));
    }
    Ok(output)
}

pub(crate) async fn git(cwd: &Path, arguments: &[&str]) -> Result<Vec<u8>, Error> {
    git_observation(cwd, arguments, false).await
}

// --no-index reports a successful comparison with differences using exit 1.
pub(crate) async fn git_observation(
    cwd: &Path,
    arguments: &[&str],
    accept_difference: bool,
) -> Result<Vec<u8>, Error> {
    let mut command = tokio::process::Command::new("git");
    command
        .args(["--no-optional-locks", "-c", "core.fsmonitor=false", "-C"])
        .arg(cwd)
        .args(arguments);
    // Caller process variables must not redirect this observation to another repo/index.
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(b"GIT_") {
            command.env_remove(name);
        }
    }
    command
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(invalid)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| invalid("Git stdout pipe missing"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| invalid("Git stderr pipe missing"))?;
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let (out, err, status) = tokio::join!(bounded(stdout), bounded(stderr), child.wait());
        Ok::<_, Error>((out?, err?, status.map_err(invalid)?))
    })
    .await;
    let (stdout, stderr, status) = match result {
        Ok(result) => result?,
        Err(_) => {
            child.kill().await.map_err(invalid)?;
            return Err(invalid("Git observation exceeded its 10 second deadline"));
        }
    };
    if !status.success() && !(accept_difference && status.code() == Some(1)) {
        return Err(invalid(format!(
            "git {} failed ({status}): {}",
            arguments.join(" "),
            String::from_utf8_lossy(&stderr)
        )));
    }
    Ok(stdout)
}

async fn git_directory(cwd: &Path, option: &str) -> Result<Directory, Error> {
    let output = git(cwd, &["rev-parse", "--path-format=absolute", option]).await?;
    let text = String::from_utf8(output).map_err(invalid)?;
    let path = text
        .strip_suffix('\n')
        .ok_or_else(|| invalid("Git directory response lacks its terminator"))?;
    directory(PathBuf::from(path)).await
}

fn parse_status(
    output: &[u8],
    root: PathBuf,
    git_directory: PathBuf,
    common_directory: PathBuf,
) -> Result<Git, Error> {
    if !output.ends_with(&[0]) {
        return Err(invalid("Git status is not NUL terminated"));
    }
    let mut branch = None;
    let mut head = None;
    let mut upstream = None;
    let mut changes = Vec::new();
    // Rename source paths occupy their own NUL record, so consume sequentially.
    let mut records = output[..output.len() - 1].split(|byte| *byte == 0);
    while let Some(record) = records.next() {
        let text = std::str::from_utf8(record)
            .map_err(|_| invalid("non-UTF-8 Git paths are not supported"))?;
        if let Some(value) = text.strip_prefix("# branch.head ") {
            branch = Some((value != "(detached)").then(|| value.to_owned()));
        } else if let Some(value) = text.strip_prefix("# branch.oid ") {
            head = Some((value != "(initial)").then(|| value.to_owned()));
        } else if let Some(value) = text.strip_prefix("# branch.upstream ") {
            upstream = Some(value.to_owned());
        } else if text.starts_with("# ") {
            continue;
        } else {
            let (status, path, original_path) = if let Some(path) = text.strip_prefix("? ") {
                ("??".into(), path.to_owned(), None)
            } else {
                let (fields, renamed) = match record.first() {
                    Some(b'1') => (9, false),
                    Some(b'2') => (10, true),
                    Some(b'u') => (11, false),
                    _ => return Err(invalid("unsupported Git status record")),
                };
                let parts: Vec<&str> = text.splitn(fields, ' ').collect();
                if parts.len() != fields || parts[1].len() != 2 {
                    return Err(invalid("malformed Git status record"));
                }
                let original = if renamed {
                    Some(
                        std::str::from_utf8(
                            records
                                .next()
                                .ok_or_else(|| invalid("rename source missing"))?,
                        )
                        .map_err(invalid)?
                        .to_owned(),
                    )
                } else {
                    None
                };
                (parts[1].to_owned(), parts[fields - 1].to_owned(), original)
            };
            if path.is_empty() {
                return Err(invalid("Git returned an empty changed path"));
            }
            changes.push(Change {
                status,
                path,
                original_path,
            });
            if changes.len() > MAX_CHANGES {
                return Err(invalid(
                    "Git status exceeds the 500-file limit; inspect with Git directly",
                ));
            }
        }
    }
    Ok(Git {
        linked_worktree: git_directory != common_directory,
        root,
        git_directory,
        common_directory,
        branch: branch.ok_or_else(|| invalid("Git branch header missing"))?,
        head: head.ok_or_else(|| invalid("Git HEAD header missing"))?,
        upstream,
        changes,
    })
}

pub(crate) async fn inspect(
    path: PathBuf,
    kind: Kind,
) -> Result<(Directory, Directory, Option<Git>), Error> {
    let current = directory(path).await?;
    if kind == Kind::Folder {
        return Ok((current.clone(), current, None));
    }
    let root = git_directory(&current.path, "--show-toplevel").await?;
    let common = git_directory(&current.path, "--git-common-dir").await?;
    let git_dir = git_directory(&current.path, "--absolute-git-dir").await?;
    let output = git(
        &current.path,
        &[
            "status",
            "--porcelain=v2",
            "--find-renames=50%",
            "--branch",
            "--untracked-files=normal",
            "-z",
        ],
    )
    .await?;
    let state = parse_status(&output, root.path, git_dir.path, common.path.clone())?;
    if directory(current.path.clone()).await? != current
        || git_directory(&current.path, "--git-common-dir").await? != common
    {
        return Err(invalid(
            "directory/repository changed during inspection; refresh before proceeding",
        ));
    }
    Ok((current, common, Some(state)))
}

impl Workspace {
    pub fn project_display_name(&self) -> std::borrow::Cow<'_, str> {
        if let Some(name) = &self.project_name.value {
            return std::borrow::Cow::Borrowed(name);
        }
        let path = &self.registration.project.path;
        let path = if self.registration.kind == Kind::Git
            && path.file_name().is_some_and(|name| name == ".git")
        {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        path.file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
    }

    /// Independently merge observed labels; a project update also reaches sibling
    /// worktrees, without changing either workspace's registration or identity.
    pub fn update_names_from(&mut self, fresh: &Self) -> bool {
        let mut changed = false;
        if self.id == fresh.id
            && fresh.name.revision >= self.name.revision
            && fresh.name != self.name
        {
            self.name = fresh.name.clone();
            changed = true;
        }
        if self.project_id == fresh.project_id
            && fresh.project_name.revision >= self.project_name.revision
            && fresh.project_name != self.project_name
        {
            self.project_name = fresh.project_name.clone();
            changed = true;
        }
        changed
    }

    pub fn display_name(&self) -> std::borrow::Cow<'_, str> {
        match &self.name.value {
            Some(name) => std::borrow::Cow::Borrowed(name),
            None => self
                .registration
                .directory
                .path
                .file_name()
                .unwrap_or(self.registration.directory.path.as_os_str())
                .to_string_lossy(),
        }
    }
    pub async fn inspect(&self) -> Result<Overview, Error> {
        let (current, project, git) = inspect(
            self.registration.directory.path.clone(),
            self.registration.kind,
        )
        .await?;
        if current != self.registration.directory || project != self.registration.project {
            return Err(invalid(
                "saved directory/repository was replaced; this entry cannot be used for the replacement",
            ));
        }
        Ok(Overview {
            workspace: self.clone(),
            git,
        })
    }

    pub async fn create(&self, connection: &Connection, id: Uuid) -> Result<Completed, Error> {
        if connection.socket != self.registration.endpoint.socket {
            return Err(invalid("controller is connected to a different Host"));
        }
        self.inspect().await?;
        let cwd = self
            .registration
            .directory
            .path
            .to_str()
            .ok_or_else(|| invalid("non-UTF-8 workspace directory"))?
            .to_owned();
        connection.execute(&Command::Create { cwd }, id).await
    }
}

impl Store {
    /// Exact saved root lookup for a session file link. Reuse its identity fence.
    pub async fn workspace_at(
        &self,
        socket: &Path,
        cwd: &Path,
    ) -> Result<Option<Workspace>, Error> {
        let socket = socket
            .to_str()
            .ok_or_else(|| invalid("non-UTF-8 Host endpoint"))?
            .to_owned();
        let cwd = cwd
            .to_str()
            .ok_or_else(|| invalid("non-UTF-8 workspace path"))?
            .to_owned();
        self.job(move |database| {
            database.query_row("SELECT w.position,w.id,w.project,w.data,n.revision,n.value,pn.revision,pn.value FROM workspaces w LEFT JOIN workspace_names n ON n.workspace=w.id LEFT JOIN project_names pn ON pn.project=w.project WHERE w.endpoint=?1 AND w.cwd=?2 ORDER BY w.position DESC LIMIT 1", params![socket,cwd], workspace_row)
                .optional()?.map(decode).transpose()
        }).await
    }

    pub async fn register_workspace(
        &self,
        endpoint: Endpoint,
        path: PathBuf,
        kind: Kind,
    ) -> Result<Workspace, Error> {
        // Validate endpoint shape without connecting to or claiming it.
        crate::hosts::Configuration::single(endpoint.socket.clone()).map_err(invalid)?;
        let (directory, project, _) = inspect(path, kind).await?;
        let registration = Registration {
            endpoint,
            directory,
            project,
            kind,
        };
        self.job(move |database| {
            let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let workspace = save_registration(&transaction, &registration)?;
            transaction.commit()?;
            Ok(workspace)
        })
        .await
    }

    pub async fn workspace(&self, id: Uuid) -> Result<Workspace, Error> {
        self.job(move |database| read(database, id)).await
    }

    pub async fn workspaces(&self, before: Option<u64>) -> Result<Vec<Workspace>, Error> {
        let before = i64::try_from(before.unwrap_or(i64::MAX as u64)).map_err(invalid)?;
        self.job(move |database| {
            let mut statement = database.prepare("SELECT w.position,w.id,w.project,w.data,n.revision,n.value,pn.revision,pn.value FROM workspaces w LEFT JOIN workspace_names n ON n.workspace=w.id LEFT JOIN project_names pn ON pn.project=w.project WHERE w.position < ?1 ORDER BY w.position DESC LIMIT 32")?;
            statement.query_map([before], workspace_row)?.map(|row| decode(row?)).collect()
        }).await
    }
}

pub(crate) fn read(database: &rusqlite::Connection, id: Uuid) -> Result<Workspace, StorageError> {
    decode(database.query_row(
        "SELECT w.position,w.id,w.project,w.data,n.revision,n.value,pn.revision,pn.value FROM workspaces w LEFT JOIN workspace_names n ON n.workspace=w.id LEFT JOIN project_names pn ON pn.project=w.project WHERE w.id=?1",
        [id.to_string()], workspace_row,
    ).optional()?.ok_or("workspace not found")?)
}

pub(crate) fn save_registration(
    database: &rusqlite::Connection,
    registration: &Registration,
) -> Result<Workspace, StorageError> {
    let endpoint = registration
        .endpoint
        .socket
        .to_str()
        .ok_or("non-UTF-8 Host endpoint")?;
    let identity = serde_json::to_string(&registration.directory)?;
    let project_key = serde_json::to_string(&(registration.kind, &registration.project))?;
    let kind = serde_json::to_string(&registration.kind)?;
    let cwd = registration
        .directory
        .path
        .to_str()
        .ok_or("non-UTF-8 directory")?;
    database.execute("INSERT INTO projects(id,endpoint,identity) VALUES(?1,?2,?3) ON CONFLICT(endpoint,identity) DO NOTHING", params![Uuid::new_v4().to_string(), endpoint, project_key])?;
    let project: String = database.query_row(
        "SELECT id FROM projects WHERE endpoint=?1 AND identity=?2",
        params![endpoint, project_key],
        |row| row.get(0),
    )?;
    database.execute("INSERT INTO workspaces(id,project,endpoint,cwd,kind,identity,data) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(endpoint,cwd,kind,identity,project) DO UPDATE SET data=excluded.data", params![Uuid::new_v4().to_string(),project,endpoint,cwd,kind,identity,serde_json::to_string(&registration)?])?;
    let row = database.query_row("SELECT w.position,w.id,w.project,w.data,n.revision,n.value,pn.revision,pn.value FROM workspaces w LEFT JOIN workspace_names n ON n.workspace=w.id LEFT JOIN project_names pn ON pn.project=w.project WHERE w.endpoint=?1 AND w.cwd=?2 AND w.kind=?3 AND w.identity=?4 AND w.project=?5", params![endpoint,cwd,kind,identity,project], workspace_row)?;
    let workspace = decode(row)?;
    Ok(workspace)
}

type WorkspaceRow = (
    i64,
    String,
    String,
    String,
    Option<i64>,
    Option<String>,
    Option<i64>,
    Option<String>,
);

pub(crate) fn workspace_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkspaceRow> {
    for (column, subject) in [(5, "workspace"), (7, "project")] {
        if let rusqlite::types::ValueRef::Text(value) = row.get_ref(column)?
            && value.len() > crate::workspace_names::MAX_NAME_BYTES
        {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{subject} name exceeds 256 bytes"),
                )
                .into(),
            ));
        }
    }
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
    ))
}

pub(crate) fn decode(row: WorkspaceRow) -> Result<Workspace, StorageError> {
    let name = crate::workspace_names::Saved::decode(row.4, row.5, "workspace")?;
    let project_name = crate::workspace_names::Saved::decode(row.6, row.7, "project")?;
    Ok(Workspace {
        position: row.0.try_into()?,
        id: row.1.parse()?,
        project_id: row.2.parse()?,
        name,
        project_name,
        registration: serde_json::from_str(&row.3)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn status(text: &[u8]) -> Result<Git, Error> {
        parse_status(
            text,
            "/repo".into(),
            "/repo/.git".into(),
            "/repo/.git".into(),
        )
    }
    #[test]
    fn status_retains_rename_newlines_conflicts_and_unborn_head() {
        let state = status(b"# branch.oid (initial)\0# branch.head main\0? new\nfile\0u UU N... 100644 100644 100644 100644 a b c conflict\0").unwrap();
        assert!(state.head.is_none());
        assert_eq!(state.branch.as_deref(), Some("main"));
        assert_eq!(state.changes[0].path, "new\nfile");
        assert_eq!(state.changes[1].status, "UU");
        let state = status(b"# branch.oid abc\0# branch.head (detached)\x002 R. N... 100644 100644 100644 a b R100 new name\0old name\0").unwrap();
        assert!(state.branch.is_none());
        assert_eq!(state.changes[0].original_path.as_deref(), Some("old name"));
        assert!(status(b"# branch.oid abc\0# branch.head main\x002 R. N... 100644 100644 100644 a b R100 new\0").is_err());
        assert!(status(b"# branch.head main\0").is_err());
        assert!(status(b"not terminated").is_err());
    }
    #[test]
    fn oversized_and_invalid_status_is_not_a_clean_workspace() {
        let mut output = b"# branch.oid abc\0# branch.head main\0".to_vec();
        for _ in 0..501 {
            output.extend(b"? new\0");
        }
        assert!(status(&output).is_err());
        assert!(status(b"# branch.oid abc\0# branch.head main\0? \xff\0").is_err());
    }
}
