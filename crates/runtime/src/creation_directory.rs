//! Host-local session creation preflight. Never creates or rewrites a path.
use serde_json::Value;
use std::{path::Path, time::Duration};
use tokio::{
    sync::Semaphore,
    time::{Instant, timeout_at},
};

static CHECKS: Semaphore = Semaphore::const_new(2);
const DEADLINE: Duration = Duration::from_secs(2);

pub(crate) async fn validate(method: &str, params: &Value) -> Result<(), String> {
    if method != "thread/start" {
        return Ok(());
    }
    let cwd = params
        .get("cwd")
        .and_then(Value::as_str)
        .ok_or("Session creation requires a workspace directory string")?;
    if !Path::new(cwd).is_absolute() || cwd.len() > 4096 || cwd.contains('\0') {
        return Err("Choose an absolute workspace directory up to 4 KiB without NUL bytes".into());
    }
    let deadline = Instant::now() + DEADLINE;
    let permit = timeout_at(deadline, CHECKS.acquire())
        .await
        .map_err(|_| "Workspace directory checks are busy; no session was created")?
        .map_err(|error| format!("Workspace directory check unavailable: {error}"))?;
    let cwd = cwd.to_owned();
    let job = tokio::task::spawn_blocking(move || {
        // A timed-out filesystem call retains the slot until it actually exits.
        let _permit = permit;
        let metadata = std::fs::metadata(&cwd)
            .map_err(|error| format!("Workspace directory {cwd:?}: {error}"))?;
        if !metadata.is_dir() {
            return Err(format!("Workspace path is not a directory: {cwd:?}"));
        }
        Ok(())
    });
    timeout_at(deadline, job)
        .await
        .map_err(|_| "Workspace directory check exceeded its deadline; no session was created")?
        .map_err(|error| format!("Workspace directory worker failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn validates_only_explicit_creation_and_rejects_invalid_input() {
        assert!(validate("thread/resume", &Value::Null).await.is_ok());
        for params in [
            Value::Null,
            json!({}),
            json!({"cwd":null}),
            json!({"cwd":2}),
            json!({"cwd":"relative"}),
            json!({"cwd":"/nul\0"}),
            json!({"cwd":"/".to_owned()+&"x".repeat(4096)}),
        ] {
            assert!(validate("thread/start", &params).await.is_err());
        }
    }

    #[tokio::test]
    async fn accepts_directory_aliases_without_changing_exact_path_and_rejects_files() {
        let root = std::env::temp_dir().join(format!("fp-create-cwd-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let directory = root.join("中文 trailing ");
        std::fs::create_dir(&directory).unwrap();
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&directory, &alias).unwrap();
        let file = root.join("file");
        std::fs::write(&file, b"text").unwrap();
        for path in [&directory, &alias] {
            assert!(validate("thread/start", &json!({"cwd":path})).await.is_ok());
        }
        for path in [&file, &root.join("absent")] {
            assert!(
                validate("thread/start", &json!({"cwd":path}))
                    .await
                    .is_err()
            );
        }
        std::fs::remove_dir(&directory).unwrap();
        assert!(
            validate("thread/start", &json!({"cwd":alias}))
                .await
                .is_err()
        );
        std::fs::remove_file(alias).unwrap();
        std::fs::remove_file(file).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
