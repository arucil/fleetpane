use super::*;
use crate::hosts::Endpoint;
use fleetpane_runtime::attention::{Kind, Record};

fn observation() -> Observation {
    Observation {
        socket: "/tmp/completion-alerts.sock".into(),
        host_id: Uuid::new_v4(),
        records: vec![],
    }
}
fn result(source: &Observation, sequence: u64, kind: Kind) -> Item {
    Item {
        endpoint: Endpoint {
            name: "Host 中文".into(),
            socket: source.socket.clone(),
        },
        host_id: source.host_id,
        record: Record {
            sequence,
            observed_unix_ms: 10,
            thread_id: "thread".into(),
            turn_id: format!("turn-{sequence}"),
            kind,
            stop_reason: None,
        },
        reviewed: false,
        cwd: Some("/tmp/workspace 中文".into()),
    }
}
async fn store() -> (PathBuf, Store) {
    let root = std::env::temp_dir().join(format!("fp-alert-claims-{}", Uuid::new_v4()));
    let store = Store::open(root.clone()).await.unwrap();
    (root, store)
}
async fn claim(store: &Store, source: &Observation) -> Batch {
    store
        .claim_completion_alerts(vec![source.clone()])
        .await
        .unwrap()
}

#[tokio::test]
async fn first_feed_is_silent_then_exact_results_are_claimed_once_across_reopen_and_stale_reads() {
    let (root, store) = store().await;
    let mut source = observation();
    source.records.push(result(&source, 2, Kind::Failed));
    assert!(claim(&store, &source).await.items.is_empty());
    let stale = source.clone();
    source.records.push(result(&source, 9, Kind::Completed));
    source.records.push(result(&source, 5, Kind::Interrupted));
    let batch = claim(&store, &source).await;
    assert_eq!(batch.items.len(), 2);
    assert_eq!(batch.items[0].record, source.records[1].record);
    assert_eq!(batch.items[1].record, source.records[2].record);
    assert!(claim(&store, &source).await.items.is_empty());
    assert!(claim(&store, &stale).await.items.is_empty());
    let reopened = Store::open(root.clone()).await.unwrap();
    source.records.retain(|i| i.record.sequence == 9);
    assert!(claim(&reopened, &source).await.items.is_empty());
    source.records.push(result(&source, 17, Kind::Failed));
    assert_eq!(
        claim(&reopened, &source).await.items[0].record.kind,
        Kind::Failed
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn simultaneous_windows_share_one_atomic_claim() {
    let (root, store) = store().await;
    let mut source = observation();
    claim(&store, &source).await;
    source.records.push(result(&source, 1, Kind::Completed));
    let second = Store::open(root.clone()).await.unwrap();
    let (a, b) = tokio::join!(
        store.claim_completion_alerts(vec![source.clone()]),
        second.claim_completion_alerts(vec![source])
    );
    assert_eq!(a.unwrap().items.len() + b.unwrap().items.len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn global_and_all_scoped_mutes_consume_results_without_replaying_on_unmute() {
    let (root, store) = store().await;
    let mut source = observation();
    claim(&store, &source).await;
    let sample = result(&source, 1, Kind::Failed);
    let mut policies = vec![Preferences {
        muted: true,
        ..Default::default()
    }];
    policies.push(Preferences {
        muted_kinds: vec![super::super::Kind::Failed],
        ..Default::default()
    });
    policies.push(Preferences {
        muted_hosts: vec![source.socket.clone()],
        ..Default::default()
    });
    policies.push(Preferences {
        muted_sessions: vec![Session {
            socket: source.socket.clone(),
            host_id: source.host_id,
            thread_id: "thread".into(),
        }],
        ..Default::default()
    });
    policies.push(Preferences {
        muted_workspaces: vec![Workspace {
            socket: source.socket.clone(),
            directory: sample.cwd.as_ref().unwrap().into(),
        }],
        ..Default::default()
    });
    for policy in policies {
        let saved = store.notifications().await.unwrap();
        let muted = store
            .save_notifications(saved.revision, policy)
            .await
            .unwrap();
        source
            .records
            .push(result(&source, muted.revision, Kind::Failed));
        let batch = claim(&store, &source).await;
        assert_eq!(batch.policy, muted);
        assert!(batch.items.is_empty());
        store
            .save_notifications(muted.revision, Preferences::default())
            .await
            .unwrap();
        assert!(claim(&store, &source).await.items.is_empty());
    }
    assert!(
        source
            .records
            .iter()
            .all(|item| !item.reviewed && item.record.kind == Kind::Failed)
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn reviewed_results_are_consumed_and_same_path_different_host_is_independent() {
    let (root, store) = store().await;
    let mut a = observation();
    let mut b = a.clone();
    b.host_id = Uuid::new_v4();
    store
        .claim_completion_alerts(vec![a.clone(), b.clone()])
        .await
        .unwrap();
    a.records.push(result(&a, 1, Kind::Failed));
    a.records[0].reviewed = true;
    b.records.push(result(&b, 1, Kind::Completed));
    let batch = store
        .claim_completion_alerts(vec![a.clone(), b.clone()])
        .await
        .unwrap();
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].host_id, b.host_id);
    a.records[0].reviewed = false;
    assert!(claim(&store, &a).await.items.is_empty());
    // A replacement incarnation baselines even when it reuses old sequence values.
    a.host_id = Uuid::new_v4();
    a.records.clear();
    a.records.push(result(&a, 1, Kind::Failed));
    assert!(claim(&store, &a).await.items.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn invalid_batches_and_sql_failures_do_not_partially_consume() {
    let (root, store) = store().await;
    let mut source = observation();
    claim(&store, &source).await;
    source.records.push(result(&source, 1, Kind::Completed));
    let mut invalid = source.clone();
    invalid.socket = "relative".into();
    assert!(
        store
            .claim_completion_alerts(vec![source.clone(), invalid])
            .await
            .is_err()
    );
    assert!(
        store
            .claim_completion_alerts(vec![source.clone(), source.clone()])
            .await
            .is_err()
    );
    let mut invalid = source.clone();
    invalid.records[0].host_id = Uuid::new_v4();
    assert!(store.claim_completion_alerts(vec![invalid]).await.is_err());
    let mut invalid = source.clone();
    invalid.records.push(invalid.records[0].clone());
    assert!(store.claim_completion_alerts(vec![invalid]).await.is_err());
    store.job(|db|{db.execute_batch("CREATE TRIGGER refuse_claim BEFORE UPDATE ON completion_alert_cursors BEGIN SELECT RAISE(ABORT,'fixture claim refusal'); END;")?;Ok(())}).await.unwrap();
    assert!(
        claim_error(&store, &source)
            .await
            .contains("fixture claim refusal")
    );
    store
        .job(|db| {
            db.execute_batch("DROP TRIGGER refuse_claim;")?;
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(claim(&store, &source).await.items.len(), 1);
    store
        .save_notifications(0, Preferences::default())
        .await
        .unwrap();
    store
        .job(|db| {
            db.execute_batch("UPDATE notification_preferences SET data='broken';")?;
            Ok(())
        })
        .await
        .unwrap();
    source.records.push(result(&source, 2, Kind::Completed));
    assert!(
        store
            .claim_completion_alerts(vec![source.clone()])
            .await
            .is_err()
    );
    store
        .job(|db| {
            db.execute_batch("DELETE FROM notification_preferences;")?;
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(claim(&store, &source).await.items.len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}
async fn claim_error(store: &Store, source: &Observation) -> String {
    store
        .claim_completion_alerts(vec![source.clone()])
        .await
        .unwrap_err()
        .to_string()
}

#[tokio::test]
async fn cursor_capacity_is_explicit_and_whole_batch_rolls_back() {
    let (root, store) = store().await;
    store
        .job(|db| {
            for n in 0..MAX_CURSORS - 1 {
                db.execute(
                    "INSERT INTO completion_alert_cursors VALUES(?1,?2,0)",
                    params![format!("/tmp/old-{n}.sock"), Uuid::new_v4().to_string()],
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();
    let a = observation();
    let b = observation();
    assert!(
        store
            .claim_completion_alerts(vec![a.clone(), b.clone()])
            .await
            .unwrap_err()
            .to_string()
            .contains("limit reached")
    );
    store
        .job(|db| {
            let count: u32 =
                db.query_row("SELECT count(*) FROM completion_alert_cursors", [], |r| {
                    r.get(0)
                })?;
            assert_eq!(count, MAX_CURSORS - 1);
            Ok(())
        })
        .await
        .unwrap();
    assert!(claim(&store, &a).await.items.is_empty());
    assert!(claim_error(&store, &b).await.contains("limit reached"));
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn schema_fifteen_upgrade_retains_policy_and_drafts_and_current_damage_is_reported() {
    let (root, store) = store().await;
    store
        .save_notifications(
            0,
            Preferences {
                muted: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    store.job(|db|{db.execute_batch("INSERT INTO drafts VALUES('retained',7,'exact draft 中文'); DROP TABLE project_names; DROP TABLE workspace_names; DROP TABLE pending_alert_cursors; DROP TABLE completion_alert_cursors; PRAGMA user_version=15;")?;Ok(())}).await.unwrap();
    let reopened = Store::open(root.clone()).await.unwrap();
    let source = observation();
    assert!(claim(&reopened, &source).await.items.is_empty());
    assert!(reopened.notifications().await.unwrap().value.muted);
    reopened
        .job(|db| {
            assert_eq!(
                db.query_row("SELECT data FROM drafts WHERE key='retained'", [], |r| {
                    r.get::<_, String>(0)
                })?,
                "exact draft 中文"
            );
            assert_eq!(
                db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))?,
                19
            );
            db.execute_batch(
                "DROP TABLE pending_alert_cursors; DROP TABLE completion_alert_cursors;",
            )?;
            Ok(())
        })
        .await
        .unwrap();
    assert!(Store::open(root.clone()).await.is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unavailable_and_legacy_hosts_do_not_manufacture_empty_baselines() {
    use crate::attention::Host;
    let source = observation();
    let mut report = Report {
        observed_unix_ms: 1,
        hosts: vec![Host {
            pending_sequence: None,
            endpoint: Endpoint {
                name: "Host".into(),
                socket: source.socket,
            },
            host_id: Some(source.host_id),
            items: vec![],
            error: None,
            completions: vec![],
            completion_evicted: None,
        }],
    };
    assert!(Observation::from_report(&report).is_empty());
    report.hosts[0].completion_evicted = Some(0);
    assert_eq!(Observation::from_report(&report).len(), 1);
    report.hosts[0].error = Some("offline".into());
    assert!(Observation::from_report(&report).is_empty());
}
