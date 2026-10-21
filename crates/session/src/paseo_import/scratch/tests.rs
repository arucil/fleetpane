use super::*;
use std::os::unix::fs::symlink;

fn root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("fp-scratch-test-{}", Uuid::new_v4()));
    fleetpane_runtime::unix::prepare_directory(&root).unwrap();
    root
}
fn private(path: &Path, data: &[u8]) {
    use std::io::Write;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap()
        .write_all(data)
        .unwrap();
}

#[test]
fn shared_writers_refuse_collection_until_all_owners_exit_and_preserve_complete_hardlinks() {
    let root = root();
    let first = Writer::open(&root).unwrap();
    let second = Writer::open(&root).unwrap();
    let temp = first.path();
    private(&temp, b"history bytes");
    let journal = temp.with_file_name(format!(
        "{}-journal",
        temp.file_name().unwrap().to_str().unwrap()
    ));
    private(&journal, b"journal bytes");
    let final_path = root.join("complete.sqlite");
    std::fs::hard_link(&temp, &final_path).unwrap();
    assert!(
        collect(&root, false)
            .unwrap_err()
            .to_string()
            .contains("active")
    );
    assert!(collect(&root, true).is_err());
    drop(first);
    assert!(collect(&root, true).is_err());
    drop(second);
    let before = collect(&root, false).unwrap();
    assert_eq!(before.files, 2);
    assert_eq!(before.bytes, 26);
    assert!(!before.removed);
    assert!(temp.exists() && journal.exists());
    let after = collect(&root, true).unwrap();
    assert_eq!(after.files, 2);
    assert!(after.removed);
    assert!(!temp.exists() && !journal.exists());
    assert_eq!(std::fs::read(final_path).unwrap(), b"history bytes");
    assert_eq!(collect(&root, true).unwrap().files, 0);
    // The lock inode stays stable; no writer can open a replacement during GC.
    let lock_path = root.join("paseo-scratch-v1.lock");
    let inode = std::fs::metadata(&lock_path).unwrap().ino();
    let _writer = Writer::open(&root).unwrap();
    assert_eq!(std::fs::metadata(lock_path).unwrap().ino(), inode);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn preflight_refuses_unknown_names_symlinks_public_files_and_nested_paths_without_partial_deletion()
{
    for kind in ["unknown", "symlink", "public", "directory", "noncanonical"] {
        let root = root();
        let writer = Writer::open(&root).unwrap();
        let valid = writer.path();
        private(&valid, b"keep until full preflight");
        let path = if kind == "unknown" {
            writer.directory.join("unowned.txt")
        } else if kind == "noncanonical" {
            writer
                .directory
                .join(format!("{}.tmp", Uuid::new_v4().simple()))
        } else {
            writer.path()
        };
        match kind {
            "symlink" => symlink(&valid, &path).unwrap(),
            "public" => {
                private(&path, b"keep");
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            }
            "directory" => std::fs::create_dir(&path).unwrap(),
            _ => private(&path, b"keep"),
        }
        drop(writer);
        assert!(collect(&root, true).is_err(), "{kind}");
        assert!(valid.exists(), "{kind}");
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn legacy_files_and_export_targets_are_reported_or_retained_without_reading_content() {
    let root = root();
    for directory in ["paseo-history", "paseo-forks"] {
        let directory = root.join(directory);
        fleetpane_runtime::unix::prepare_directory(&directory).unwrap();
        private(&directory.join("old.tmp"), b"old worker owns this");
        private(&directory.join("final.sqlite"), b"preserve published data");
    }
    let writer = Writer::open(&root).unwrap();
    let file = writer.path();
    private(&file, b"new abandoned copy");
    drop(writer);
    let report = collect(&root, true).unwrap();
    assert_eq!(report.files, 1);
    assert_eq!(report.legacy_files_retained, 2);
    assert!(
        root.join("paseo-history/old.tmp").exists() && root.join("paseo-forks/old.tmp").exists()
    );
    assert!(root.join("paseo-history/final.sqlite").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn exclusive_collection_fences_new_writers_and_rejects_invalid_lock_or_directory() {
    let root = root();
    let guard = lock(&root, true).unwrap();
    assert!(Writer::open(&root).is_err());
    drop(guard);
    std::fs::set_permissions(
        root.join("paseo-scratch-v1.lock"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(Writer::open(&root).is_err());
    assert!(collect(&root, true).is_err());
    std::fs::remove_file(root.join("paseo-scratch-v1.lock")).unwrap();
    symlink(root.join("outside"), root.join(DIRECTORY)).unwrap();
    assert!(Writer::open(&root).is_err());
    assert!(collect(&root, true).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn cancelled_awaiter_cannot_release_a_blocking_writer_guard() {
    let root = root();
    let path = root.clone();
    let (started, ready) = tokio::sync::oneshot::channel();
    let (finish, wait) = std::sync::mpsc::channel();
    let worker = tokio::spawn(async move {
        tokio::task::spawn_blocking(move || {
            let writer = Writer::open(&path).unwrap();
            private(&writer.path(), b"in flight");
            started.send(()).unwrap();
            wait.recv().unwrap();
            drop(writer);
        })
        .await
        .unwrap();
    });
    ready.await.unwrap();
    worker.abort();
    let _cancelled = worker.await;
    assert!(collect(&root, true).is_err());
    finish.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            match lock(&root, true) {
                Ok(guard) => {
                    drop(guard);
                    break;
                }
                Err(_) => tokio::task::yield_now().await,
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(collect(&root, true).unwrap().files, 1);
    std::fs::remove_dir_all(root).unwrap();
}
