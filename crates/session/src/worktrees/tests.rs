use super::*;
use crate::hosts::Endpoint;
use std::{fs, os::unix::fs::symlink, process::Command};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("fp-worktree-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(self.0.join("repo"))
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .trim_end_matches('\n')
            .to_owned()
    }
    async fn source(&self, store: &Store) -> Uuid {
        store
            .register_workspace(
                Endpoint {
                    name: "Offline fixture".into(),
                    socket: self.0.join("unused/control.sock"),
                },
                self.0.join("repo"),
                Kind::Git,
            )
            .await
            .unwrap()
            .id
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        // Unblock the fixture filter even if an assertion fails while its owning
        // worker is completing. Production code never polls this file.
        if self.0.exists() {
            fs::write(self.0.join("release"), "release").unwrap();
        }
    }
}

#[tokio::test]
async fn cancelled_awaiter_keeps_repository_lock_until_owned_checkout_completes() {
    let fixture = Fixture::new();
    let root = &fixture.0;
    fs::create_dir(root.join("repo")).unwrap();
    fixture.git(&["init", "-b", "main"]);
    fixture.git(&["config", "user.email", "fixture@example.invalid"]);
    fixture.git(&["config", "user.name", "Fixture"]);
    fs::write(root.join("repo/file"), "original\n").unwrap();
    fs::write(
        root.join("repo/.gitattributes"),
        "file filter=fleetpane-test\n",
    )
    .unwrap();
    fixture.git(&["add", "."]);
    fixture.git(&["commit", "-qm", "base"]);
    fixture.git(&["branch", "existing"]);
    let script = root.join("filter.py");
    fs::write(&script, format!(
        "import sys,time\nfrom pathlib import Path\nr=Path({})\nr.joinpath('entered').write_text('entered')\nwhile not r.joinpath('release').exists(): time.sleep(.01)\nsys.stdout.buffer.write(sys.stdin.buffer.read())\n",
        serde_json::to_string(root.to_str().unwrap()).unwrap()
    )).unwrap();
    fixture.git(&[
        "config",
        "filter.fleetpane-test.smudge",
        &format!("python3 {}", script.display()),
    ]);
    fixture.git(&["config", "filter.fleetpane-test.required", "true"]);
    fixture.git(&["config", "filter.fleetpane-test.clean", "cat"]);
    let a = Store::open(root.join("a")).await.unwrap();
    let b = Store::open(root.join("b")).await.unwrap();
    let a_source = fixture.source(&a).await;
    let b_source = fixture.source(&b).await;
    let first = a
        .plan_existing_worktree(Uuid::new_v4(), a_source, "existing".into())
        .await
        .unwrap();
    let second = b
        .plan_existing_worktree(Uuid::new_v4(), b_source, "existing".into())
        .await
        .unwrap();
    // Missing mode preserves all legacy plans; an explicit mode is serialized.
    let mut legacy = serde_json::to_value(&first.plan).unwrap();
    legacy.as_object_mut().unwrap().remove("branch_mode");
    let decoded: Plan = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(decoded.branch_mode, BranchMode::New);
    assert_eq!(serde_json::to_value(decoded).unwrap(), legacy);
    assert_eq!(
        serde_json::to_value(&first.plan).unwrap()["branch_mode"],
        "existing"
    );
    let task_store = a.clone();
    let id = first.plan.id;
    let task = tokio::spawn(async move { task_store.apply_worktree(id).await });
    tokio::time::timeout(Duration::from_secs(15), async {
        while !root.join("entered").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    let error = b
        .apply_worktree(second.plan.id)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("holds this repository"), "{error}");
    assert_eq!(
        b.worktree_job(second.plan.id).await.unwrap().state,
        State::Prepared
    );
    assert_eq!(a.worktree_job(id).await.unwrap().state, State::Attempted);
    fs::write(root.join("release"), "release").unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let job = a.worktree_job(id).await.unwrap();
            if job.state != State::Attempted {
                assert!(matches!(job.state, State::Completed { .. }), "{job:?}");
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let error = b
        .apply_worktree(second.plan.id)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("already checked out"), "{error}");
    let new = b
        .plan_worktree(Uuid::new_v4(), b_source, "new".into(), "main".into())
        .await
        .unwrap();
    assert!(matches!(
        b.apply_worktree(new.plan.id).await.unwrap().state,
        State::Completed { .. }
    ));
    assert_eq!(
        fs::read_to_string(first.plan.cwd.join("file")).unwrap(),
        "original\n"
    );
    fixture.git(&["worktree", "remove", first.plan.path.to_str().unwrap()]);
    fixture.git(&["worktree", "remove", new.plan.path.to_str().unwrap()]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn repository_guard_refuses_parallel_symlink_fifo_and_public_lock_files() {
    let fixture = Fixture::new();
    let path = fixture.0.join("fleetpane-worktree-create.lock");
    let held = repository_lock(&fixture.0).unwrap();
    assert!(repository_lock(&fixture.0).is_err());
    drop(held);
    drop(repository_lock(&fixture.0).unwrap());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(repository_lock(&fixture.0).is_err());
    fs::remove_file(&path).unwrap();
    fs::write(fixture.0.join("target"), "preserve").unwrap();
    symlink(fixture.0.join("target"), &path).unwrap();
    assert!(repository_lock(&fixture.0).is_err());
    assert_eq!(
        fs::read_to_string(fixture.0.join("target")).unwrap(),
        "preserve"
    );
    fs::remove_file(&path).unwrap();
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &path,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    assert!(repository_lock(&fixture.0).is_err());
    fs::remove_dir_all(&fixture.0).unwrap();
}
