use super::*;
use crate::{
    hosts::Endpoint,
    paseo_import::{Roots, Selection},
};

async fn fixture() -> (Store, BatchPlan) {
    let root = std::env::temp_dir().join(format!("fp-batch-receipt-{}", Uuid::new_v4()));
    let store = Store::open(root.clone()).await.unwrap();
    let plan = BatchPlan {
        home: root.join("absent-paseo"),
        workspaces: vec![Selection {
            workspace_id: "one".into(),
            revision: "a".repeat(64),
        }],
        endpoint: Endpoint {
            name: "saved target".into(),
            socket: root.join("absent.sock"),
        },
        roots: Roots {
            codex: root.join("codex"),
            dsh: root.join("dsh"),
        },
    };
    (store, plan)
}

fn result(n: usize) -> WorkspaceResult {
    WorkspaceResult {
        source_id: n.to_string(),
        name: format!("Workspace {n}"),
        imported_id: None,
        reused: false,
        registration_error: None,
        error: Some("explicit failure".into()),
        session_count: 0,
        histories_available: 0,
        history_failures: Vec::new(),
        history_warnings: Vec::new(),
        histories_finished: false,
    }
}

#[tokio::test]
async fn active_writer_interrupted_observation_retry_and_original_receipt_are_distinct() {
    let (store, plan) = fixture().await;
    let (id, writer) = store.begin_paseo_batch(plan).await.unwrap();
    assert!(store.paseo_batch(id).await.unwrap().summary.writer_active);
    assert!(
        store
            .retry_paseo_batch(id, BatchControl::default())
            .await
            .unwrap_err()
            .to_string()
            .contains("active worker")
    );
    assert_eq!(store.paseo_batches(None).await.unwrap().len(), 1);
    drop(writer);
    let reopened = Store::open(store.root().into()).await.unwrap();
    let saved = reopened.paseo_batch(id).await.unwrap();
    assert_eq!(saved.summary.state, BatchState::Running);
    assert!(!saved.summary.writer_active);
    assert_eq!(
        saved.summary.state.label(false),
        "Interrupted; completion unconfirmed"
    );
    assert!(reopened.paseo_workspaces().await.unwrap().is_empty());
    assert!(
        reopened
            .retry_paseo_batch(id, BatchControl::default())
            .await
            .is_err()
    );
    let summaries = reopened.paseo_batches(None).await.unwrap();
    assert_eq!(summaries.len(), 2);
    assert_eq!(summaries[0].state, BatchState::Failed);
    assert!(summaries[0].error.is_some());
    assert_eq!(summaries[1].id, id);
    assert_eq!(summaries[1].state, BatchState::Running);
    std::fs::remove_dir_all(store.root()).unwrap();
}

#[tokio::test]
async fn checkpoints_are_atomic_and_result_and_summary_pages_do_not_repeat_or_skip() {
    let (store, plan) = fixture().await;
    let (id, writer) = store.begin_paseo_batch(plan.clone()).await.unwrap();
    for ordinal in 0..70 {
        store
            .checkpoint_paseo_batch(
                id,
                &BatchProgress {
                    workspaces_done: ordinal + 1,
                    ..Default::default()
                },
                Some((ordinal, &result(ordinal))),
            )
            .await
            .unwrap();
    }
    let mut after = None;
    let mut names = Vec::new();
    loop {
        let page = store.paseo_batch_results(id, after).await.unwrap();
        assert!(page.results.len() <= 32);
        names.extend(page.results.into_iter().map(|r| r.source_id));
        after = page.next;
        if after.is_none() {
            break;
        }
    }
    assert_eq!(names, (0..70).map(|n| n.to_string()).collect::<Vec<_>>());
    // A record bound failure rolls back both the aggregate and result slot.
    let mut huge = result(70);
    huge.error = Some("x".repeat(MAX_RECORD));
    assert!(
        store
            .checkpoint_paseo_batch(id, &BatchProgress::default(), Some((70, &huge)))
            .await
            .is_err()
    );
    assert_eq!(
        store
            .paseo_batch(id)
            .await
            .unwrap()
            .summary
            .progress
            .workspaces_done,
        70
    );
    assert!(
        store
            .paseo_batch_results(id, Some(69))
            .await
            .unwrap()
            .results
            .is_empty()
    );
    for _ in 0..34 {
        let (_, lock) = store.begin_paseo_batch(plan.clone()).await.unwrap();
        drop(lock);
    }
    let newest = store.paseo_batches(None).await.unwrap();
    assert_eq!(newest.len(), 32);
    let older = store
        .paseo_batches(Some(newest.last().unwrap().position))
        .await
        .unwrap();
    assert_eq!(older.len(), 3);
    assert_eq!(older.last().unwrap().id, id);
    drop(writer);
    std::fs::remove_dir_all(store.root()).unwrap();
}

#[tokio::test]
async fn schema_nine_upgrade_preserves_data_and_current_schema_damage_is_an_error() {
    let (store, _) = fixture().await;
    store.job(|db|{
        db.execute_batch("INSERT INTO drafts(key,revision,data) VALUES('saved',2,'exact bytes'); DROP TABLE paseo_batch_results; DROP TABLE paseo_batches; PRAGMA user_version=9;")?;Ok(())
    }).await.unwrap();
    let reopened = Store::open(store.root().into()).await.unwrap();
    reopened
        .job(|db| {
            assert_eq!(
                db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
                10
            );
            assert_eq!(
                db.query_row("SELECT data FROM drafts WHERE key='saved'", [], |r| r
                    .get::<_, String>(0))?,
                "exact bytes"
            );
            db.execute_batch("DROP TABLE paseo_batch_results;")?;
            Ok(())
        })
        .await
        .unwrap();
    assert!(Store::open(store.root().into()).await.is_err());
    std::fs::remove_dir_all(store.root()).unwrap();
}

#[tokio::test]
async fn result_pages_obey_encoded_byte_budget_and_oversize_reads_fail_explicitly() {
    let (store, plan) = fixture().await;
    let (id, writer) = store.begin_paseo_batch(plan).await.unwrap();
    for n in 0..3 {
        let mut row = result(n);
        row.error = Some("x".repeat(150_000));
        store
            .checkpoint_paseo_batch(id, &BatchProgress::default(), Some((n, &row)))
            .await
            .unwrap();
    }
    let first = store.paseo_batch_results(id, None).await.unwrap();
    assert_eq!(first.results.len(), 1);
    assert_eq!(first.next, Some(0));
    let second = store.paseo_batch_results(id, first.next).await.unwrap();
    assert_eq!(second.results[0].source_id, "1");
    assert_eq!(second.next, Some(1));
    store
        .job(move |db| {
            db.execute(
                "UPDATE paseo_batch_results SET data=?1 WHERE batch=?2 AND ordinal=2",
                params!["x".repeat(MAX_RECORD + 1), id.to_string()],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    assert!(store.paseo_batch_results(id, Some(1)).await.is_err());
    drop(writer);
    std::fs::remove_dir_all(store.root()).unwrap();
}

#[tokio::test]
async fn worker_lock_rejects_symlinks_public_files_and_special_files() {
    use std::os::unix::fs::symlink;
    let (store, _) = fixture().await;
    for kind in ["symlink", "public", "fifo"] {
        let id = Uuid::new_v4();
        let path = store.root().join(format!("paseo-batch-{id}.lock"));
        match kind {
            "symlink" => symlink("not-followed", &path).unwrap(),
            "public" => {
                std::fs::write(&path, "").unwrap();
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            }
            "fifo" => {
                assert!(
                    std::process::Command::new("mkfifo")
                        .args(["-m", "600"])
                        .arg(&path)
                        .status()
                        .unwrap()
                        .success()
                );
            }
            _ => unreachable!(),
        }
        assert!(writer_lock(store.root(), id).is_err(), "{kind}");
    }
    std::fs::remove_dir_all(store.root()).unwrap();
}
