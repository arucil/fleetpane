//! Explicit local Git mutation. Durable plans are never automatically replayed.
use crate::{
    client::Error,
    storage::{StorageError, Store},
    workspaces::{Directory, Kind, Registration, directory, git},
};
use rusqlite::{OptionalExtension, params};
use rustix::process::{Pid, Signal, kill_process_group};
use serde::{Deserialize, Serialize};
use std::{os::unix::process::CommandExt, path::PathBuf, process::Stdio, time::Duration};
use uuid::Uuid;

fn invalid(message: impl std::fmt::Display) -> Error {
    Error::Input(format!("Worktree: {message}"))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub id: Uuid,
    pub source_id: Uuid,
    pub project_id: Uuid,
    pub source: Registration,
    pub parent: Directory,
    pub path: PathBuf,
    pub cwd: PathBuf,
    pub branch: String,
    pub base_ref: String,
    pub commit: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum State {
    Prepared,
    Attempted,
    Completed { workspace_id: Uuid },
    Failed { error: String },
}

#[derive(Clone, Debug, Serialize)]
pub struct Job {
    pub position: u64,
    pub plan: Plan,
    pub state: State,
}

async fn absent(path: PathBuf) -> Result<(), Error> {
    tokio::task::spawn_blocking(move || match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(invalid(error)),
        Ok(_) => Err(invalid(
            "target already exists; no existing directory will be reused",
        )),
    })
    .await
    .map_err(invalid)?
}

fn text(output: Vec<u8>) -> Result<String, Error> {
    let value = String::from_utf8(output).map_err(invalid)?;
    value
        .strip_suffix('\n')
        .map(str::to_owned)
        .ok_or_else(|| invalid("Git output lacks its terminator"))
}

fn validate_inputs(branch: &str, base: &str) -> Result<(), Error> {
    if branch.is_empty()
        || branch.len() > 255
        || branch.starts_with('-')
        || branch == "HEAD"
        || branch.contains('\0')
        || base.is_empty()
        || base.len() > 1024
        || base.contains('\0')
    {
        return Err(invalid(
            "choose a new branch (1–255 bytes) and an existing commit/ref (1–1024 bytes)",
        ));
    }
    Ok(())
}

impl Store {
    /// Resolves the baseline now. Applying this plan never follows a moving ref.
    pub async fn plan_worktree(
        &self,
        id: Uuid,
        source_id: Uuid,
        branch: String,
        base_ref: String,
    ) -> Result<Job, Error> {
        validate_inputs(&branch, &base_ref)?;
        if let Some(job) = self.find_worktree(id).await? {
            if job.plan.source_id != source_id
                || job.plan.branch != branch
                || job.plan.base_ref != base_ref
            {
                return Err(invalid("UUID already belongs to a different worktree plan"));
            }
            return Ok(job);
        }
        let source = self.workspace(source_id).await?;
        let overview = source.inspect().await?;
        let state = overview
            .git
            .ok_or_else(|| invalid("select a registered Git workspace"))?;
        git(
            &state.root,
            &["check-ref-format", &format!("refs/heads/{branch}")],
        )
        .await?;
        let existing = git(
            &state.root,
            &[
                "for-each-ref",
                "--format=%(refname)",
                "--count=1",
                &format!("refs/heads/{branch}"),
            ],
        )
        .await?;
        if !existing.is_empty() {
            return Err(invalid(
                "branch already exists or has conflicting child refs; choose a new branch",
            ));
        }
        let commit = text(
            git(
                &state.root,
                &[
                    "rev-parse",
                    "--verify",
                    "--end-of-options",
                    &format!("{base_ref}^{{commit}}"),
                ],
            )
            .await?,
        )?;
        if !matches!(commit.len(), 40 | 64) || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(invalid(
                "Git did not resolve the baseline to a full commit ID",
            ));
        }
        let relative = source
            .registration
            .directory
            .path
            .strip_prefix(&state.root)
            .map_err(invalid)?
            .to_path_buf();
        if !relative.as_os_str().is_empty() {
            let tree = format!(
                "{commit}:{}",
                relative
                    .to_str()
                    .ok_or_else(|| invalid("non-UTF-8 subdirectory"))?
            );
            if text(git(&state.root, &["cat-file", "-t", &tree]).await?)? != "tree" {
                return Err(invalid("selected subdirectory is absent from the baseline"));
            }
        }
        let parent_path = self.root().join("worktrees");
        let parent = tokio::task::spawn_blocking(move || {
            fleetpane_runtime::unix::prepare_directory(&parent_path).map(|()| parent_path)
        })
        .await
        .map_err(invalid)?
        .map_err(invalid)?;
        let parent = directory(parent).await?;
        let path = parent.path.join(id.to_string());
        absent(path.clone()).await?;
        let plan = Plan {
            id,
            source_id,
            project_id: source.project_id,
            source: source.registration,
            parent,
            cwd: path.join(relative),
            path,
            branch,
            base_ref,
            commit,
        };
        let data = serde_json::to_string(&plan).map_err(invalid)?;
        self.job(move |db| {
            db.execute("INSERT INTO worktree_jobs(id,plan,state) VALUES(?1,?2,?3) ON CONFLICT(id) DO NOTHING", params![id.to_string(),data,serde_json::to_string(&State::Prepared)?])?;
            Ok(())
        }).await?;
        let stored = self.worktree_job(id).await?;
        if stored.plan.source_id != plan.source_id
            || stored.plan.branch != plan.branch
            || stored.plan.base_ref != plan.base_ref
        {
            return Err(invalid("UUID already belongs to a different worktree plan"));
        }
        Ok(stored)
    }

    async fn find_worktree(&self, id: Uuid) -> Result<Option<Job>, Error> {
        self.job(move |db| {
            db.query_row(
                "SELECT position,plan,state FROM worktree_jobs WHERE id=?1",
                [id.to_string()],
                row,
            )
            .optional()?
            .map(decode)
            .transpose()
        })
        .await
    }

    pub async fn worktree_job(&self, id: Uuid) -> Result<Job, Error> {
        self.find_worktree(id)
            .await?
            .ok_or_else(|| invalid("creation plan not found"))
    }

    pub async fn worktree_jobs(&self, before: Option<u64>) -> Result<Vec<Job>, Error> {
        let before = i64::try_from(before.unwrap_or(i64::MAX as u64)).map_err(invalid)?;
        self.job(move |db| {
            let mut statement = db.prepare("SELECT position,plan,state FROM worktree_jobs WHERE position<?1 ORDER BY position DESC LIMIT 32")?;
            statement.query_map([before], row)?.map(|r|decode(r?)).collect()
        }).await
    }

    /// The attempted transition is committed before invoking Git, including when
    /// the controller exits before it can observe/persist Git's outcome.
    pub async fn apply_worktree(&self, id: Uuid) -> Result<Job, Error> {
        let job = self.worktree_job(id).await?;
        if job.state != State::Prepared {
            return Err(invalid(
                "plan was already attempted; inspect its saved status and residual path, never resend it",
            ));
        }
        let plan = &job.plan;
        let source = self.workspace(plan.source_id).await?;
        let overview = source.inspect().await?;
        let git_state = overview
            .git
            .ok_or_else(|| invalid("source is not a Git workspace"))?;
        if source.project_id != plan.project_id
            || source.registration.directory != plan.source.directory
            || source.registration.project != plan.source.project
            || source.registration.endpoint.socket != plan.source.endpoint.socket
            || directory(plan.parent.path.clone()).await? != plan.parent
        {
            return Err(invalid(
                "source repository or destination parent identity changed; prepare a new plan",
            ));
        }
        absent(plan.path.clone()).await?;
        self.job(move |db| {
            let changed = db.execute(
                "UPDATE worktree_jobs SET state=?1 WHERE id=?2 AND state=?3",
                params![
                    serde_json::to_string(&State::Attempted)?,
                    id.to_string(),
                    serde_json::to_string(&State::Prepared)?
                ],
            )?;
            if changed != 1 {
                return Err("worktree plan was already attempted".into());
            }
            Ok(())
        })
        .await?;
        let outcome = async {
            add(&git_state.root, plan).await.map_err(|error| {
                invalid(format!("Git creation step reported an error: {error}"))
            })?;
            let workspace = self
                .register_workspace(plan.source.endpoint.clone(), plan.cwd.clone(), Kind::Git)
                .await
                .map_err(|error| {
                    invalid(format!(
                        "Git creation completed; catalog registration failed: {error}"
                    ))
                })?;
            let inspected = workspace.inspect().await.map_err(|error| {
                invalid(format!(
                    "Git creation and registration completed; verification failed: {error}"
                ))
            })?;
            let result = inspected
                .git
                .ok_or_else(|| invalid("created directory is not a Git worktree"))?;
            if workspace.project_id != plan.project_id
                || !result.linked_worktree
                || result.root != plan.path
                || result.branch.as_deref() != Some(plan.branch.as_str())
                || result.head.as_deref() != Some(plan.commit.as_str())
            {
                return Err(invalid(
                    "created worktree does not match its plan; inspect the residual directory",
                ));
            }
            Ok::<_, Error>(State::Completed {
                workspace_id: workspace.id,
            })
        }
        .await;
        let state = match outcome {
            Ok(state) => state,
            Err(error) => State::Failed {
                error: format!(
                    "{error}. Residual directory: {}; branch: {}. No automatic cleanup or retry was performed.",
                    plan.path.display(),
                    plan.branch
                ),
            },
        };
        self.job(move |db| {
            if db.execute(
                "UPDATE worktree_jobs SET state=?1 WHERE id=?2 AND state=?3",
                params![
                    serde_json::to_string(&state)?,
                    id.to_string(),
                    serde_json::to_string(&State::Attempted)?
                ],
            )? != 1
            {
                return Err("worktree result state changed unexpectedly".into());
            }
            Ok(())
        })
        .await?;
        self.worktree_job(id).await
    }
}

fn row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(i64, String, String)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
}
fn decode(row: (i64, String, String)) -> Result<Job, StorageError> {
    Ok(Job {
        position: row.0.try_into()?,
        plan: serde_json::from_str(&row.1)?,
        state: serde_json::from_str(&row.2)?,
    })
}

// Checkout may invoke configured Git filters. Bound the entire process group,
// including cancellation; do not leave a writer running after reporting timeout.
struct Group(Option<Pid>);
impl Group {
    fn kill(&mut self) -> Result<(), Error> {
        if let Some(pid) = self.0.take() {
            match kill_process_group(pid, Signal::KILL) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                Err(error) => return Err(invalid(error)),
            }
        }
        Ok(())
    }
}
impl Drop for Group {
    fn drop(&mut self) {
        if let Err(error) = self.kill() {
            eprintln!("{error}");
        }
    }
}

async fn add(root: &std::path::Path, plan: &Plan) -> Result<(), Error> {
    let mut command = tokio::process::Command::new("git");
    command
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "submodule.recurse=false",
            "-C",
        ])
        .arg(root)
        .args([
            "worktree",
            "add",
            "--quiet",
            "--no-track",
            "-b",
            &plan.branch,
            "--",
        ])
        .arg(&plan.path)
        .arg(&plan.commit);
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(b"GIT_") {
            command.env_remove(name);
        }
    }
    command
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command.as_std_mut().process_group(0);
    let mut child = command.spawn().map_err(invalid)?;
    let pid = child
        .id()
        .and_then(|id| i32::try_from(id).ok())
        .and_then(Pid::from_raw)
        .ok_or_else(|| invalid("Git process group ID missing"))?;
    let mut group = Group(Some(pid));
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| invalid("Git stdout pipe missing"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| invalid("Git stderr pipe missing"))?;
    let result = tokio::time::timeout(Duration::from_secs(120), async {
        tokio::join!(
            crate::workspaces::bounded(stdout),
            crate::workspaces::bounded(stderr),
            child.wait()
        )
    })
    .await;
    match result {
        Ok((out, err, status)) => {
            group.kill()?;
            let _out = out?;
            let err = err?;
            let status = status.map_err(invalid)?;
            if !status.success() {
                return Err(invalid(format!(
                    "git worktree add failed ({status}): {}",
                    String::from_utf8_lossy(&err)
                )));
            }
            Ok(())
        }
        Err(_) => {
            group.kill()?;
            child.wait().await.map_err(invalid)?;
            Err(invalid(
                "git worktree add exceeded 120 seconds; outcome may be incomplete",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reject_option_and_unbounded_inputs() {
        for branch in ["", "-b", "HEAD", "a\0b"] {
            assert!(validate_inputs(branch, "HEAD").is_err());
        }
        assert!(validate_inputs(&"a".repeat(256), "HEAD").is_err());
        assert!(validate_inputs("feature/中文", "refs/heads/main").is_ok());
        assert!(validate_inputs("feature", "").is_err());
        assert!(validate_inputs("feature", &"x".repeat(1025)).is_err());
    }
}
