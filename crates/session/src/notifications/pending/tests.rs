use super::*;
use crate::hosts::Endpoint;
use serde_json::json;

fn source() -> Observation {
    Observation {
        socket: "/tmp/pending-alerts.sock".into(),
        host_id: Uuid::new_v4(),
        sequence: 0,
        items: vec![],
    }
}
fn item(source: &Observation, sequence: u64, method: &str) -> Item {
    let request =
        json!({"id":1,"method":method,"params":{"threadId":"same","command":"echo synthetic"}});
    let mut item = Item::from_request(
        Endpoint {
            name: "Synthetic".into(),
            socket: source.socket.clone(),
        },
        source.host_id,
        request,
    )
    .unwrap();
    item.request_sequence = Some(sequence);
    item.observed_unix_ms = Some(10);
    item.cwd = Some("/tmp/workspace".into());
    item
}
async fn store() -> (PathBuf, Store) {
    let root = std::env::temp_dir().join(format!("fp-pending-{}", Uuid::new_v4()));
    let store = Store::open(root.clone()).await.unwrap();
    (root, store)
}
async fn claim(store: &Store, source: &Observation) -> Batch {
    store
        .claim_pending_alerts(vec![source.clone()])
        .await
        .unwrap()
}

#[tokio::test]
async fn baseline_reopen_stale_empty_snapshot_id_reuse_and_concurrent_process_stores() {
    let (root, store) = store().await;
    let mut source = source();
    source.sequence = 5;
    source
        .items
        .push(item(&source, 5, crate::questions::METHOD));
    assert!(claim(&store, &source).await.items.is_empty());
    let stale = source.clone();
    source.items.clear();
    source.sequence = 10;
    assert!(claim(&store, &source).await.items.is_empty());
    assert!(claim(&store, &stale).await.items.is_empty());
    source
        .items
        .push(item(&source, 11, crate::questions::METHOD));
    source.sequence = 11;
    let second = Store::open(root.clone()).await.unwrap();
    let (a, b) = tokio::join!(claim(&store, &source), claim(&second, &source));
    assert_eq!(a.items.len() + b.items.len(), 1);
    assert!(claim(&second, &source).await.items.is_empty());
    source.host_id = Uuid::new_v4();
    source.items[0].host_id = source.host_id;
    assert!(claim(&second, &source).await.items.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn mute_scopes_consume_without_answering_and_unmute_never_replays() {
    let (root, store) = store().await;
    let mut source = source();
    claim(&store, &source).await;
    let candidate = item(&source, 1, "session/request_permission");
    let scopes = [
        Preferences {
            muted: true,
            ..Default::default()
        },
        Preferences {
            muted_kinds: vec![Kind::Approval],
            ..Default::default()
        },
        Preferences {
            muted_hosts: vec![source.socket.clone()],
            ..Default::default()
        },
        Preferences {
            muted_sessions: vec![Session {
                socket: source.socket.clone(),
                host_id: source.host_id,
                thread_id: "same".into(),
            }],
            ..Default::default()
        },
        Preferences {
            muted_workspaces: vec![Workspace {
                socket: source.socket.clone(),
                directory: "/tmp/workspace".into(),
            }],
            ..Default::default()
        },
    ];
    for policy in scopes {
        let saved = store.notifications().await.unwrap();
        store
            .save_notifications(saved.revision, policy)
            .await
            .unwrap();
        source.sequence += 1;
        source.items = vec![candidate.clone()];
        source.items[0].request_sequence = Some(source.sequence);
        assert!(claim(&store, &source).await.items.is_empty());
        let saved = store.notifications().await.unwrap();
        store
            .save_notifications(saved.revision, Preferences::default())
            .await
            .unwrap();
        assert!(claim(&store, &source).await.items.is_empty());
    }
    source.sequence += 1;
    source.items[0].request_sequence = Some(source.sequence);
    assert_eq!(claim(&store, &source).await.items.len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn invalid_batch_write_failure_policy_corruption_and_limit_roll_back() {
    let (root, store) = store().await;
    let mut source = source();
    claim(&store, &source).await;
    source.sequence = 1;
    source
        .items
        .push(item(&source, 1, crate::questions::METHOD));
    let mut bad = source.clone();
    bad.items[0].request_sequence = None;
    assert!(
        store
            .claim_pending_alerts(vec![source.clone(), bad])
            .await
            .is_err()
    );
    let mut bad = source.clone();
    bad.items[0].kind = attention::Kind::Approval;
    assert!(store.claim_pending_alerts(vec![bad]).await.is_err());
    store.job(|db|{db.execute_batch("CREATE TRIGGER refuse_pending BEFORE UPDATE ON pending_alert_cursors BEGIN SELECT RAISE(ABORT,'owned refusal'); END;")?;Ok(())}).await.unwrap();
    assert!(
        store
            .claim_pending_alerts(vec![source.clone()])
            .await
            .unwrap_err()
            .to_string()
            .contains("owned refusal")
    );
    store
        .job(|db| {
            db.execute_batch("DROP TRIGGER refuse_pending;")?;
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
    source.sequence = 2;
    source.items[0].request_sequence = Some(2);
    assert!(
        store
            .claim_pending_alerts(vec![source.clone()])
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
    store
        .job(|db| {
            for n in 0..MAX_CURSORS - 1 {
                db.execute(
                    "INSERT INTO pending_alert_cursors VALUES(?1,?2,0)",
                    params![format!("/tmp/{n}.sock"), Uuid::new_v4().to_string()],
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();
    let mut replacement = source.clone();
    replacement.host_id = Uuid::new_v4();
    replacement.items.clear();
    assert!(
        store
            .claim_pending_alerts(vec![replacement])
            .await
            .unwrap_err()
            .to_string()
            .contains("limit reached")
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn schema_sixteen_upgrade_preserves_claims_and_drafts_and_current_damage_fails() {
    let (root, store) = store().await;
    store.job(|db|{db.execute_batch("INSERT INTO drafts VALUES('kept',2,'draft 中文  '); INSERT INTO completion_alert_cursors VALUES('old','host',7); DROP TABLE project_names; DROP TABLE workspace_names; DROP TABLE pending_alert_cursors; PRAGMA user_version=16;")?;Ok(())}).await.unwrap();
    let upgraded = Store::open(root.clone()).await.unwrap();
    assert!(claim(&upgraded, &source()).await.items.is_empty());
    upgraded
        .job(|db| {
            assert_eq!(
                db.query_row("SELECT sequence FROM completion_alert_cursors", [], |r| r
                    .get::<_, i64>(
                    0
                ))?,
                7
            );
            assert_eq!(
                db.query_row("SELECT data FROM drafts", [], |r| r.get::<_, String>(0))?,
                "draft 中文  "
            );
            db.execute_batch("DROP TABLE pending_alert_cursors;")?;
            Ok(())
        })
        .await
        .unwrap();
    assert!(Store::open(root.clone()).await.is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn capacity_failure_rolls_back_an_earlier_update_and_new_baseline() {
    let (root, store) = store().await;
    let mut first = source();
    claim(&store, &first).await;
    store
        .job(|db| {
            for n in 0..MAX_CURSORS - 2 {
                db.execute(
                    "INSERT INTO pending_alert_cursors VALUES(?1,?2,0)",
                    params![format!("/tmp/old-{n}.sock"), Uuid::new_v4().to_string()],
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();
    first.sequence = 1;
    first.items.push(item(&first, 1, crate::questions::METHOD));
    assert!(
        store
            .claim_pending_alerts(vec![first.clone(), source(), source()])
            .await
            .unwrap_err()
            .to_string()
            .contains("limit reached")
    );
    assert_eq!(claim(&store, &first).await.items.len(), 1);
    store
        .job(|db| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM pending_alert_cursors", [], |r| r
                    .get::<_, u32>(0))?,
                MAX_CURSORS - 1
            );
            Ok(())
        })
        .await
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_faulted_and_unavailable_reports_do_not_invent_a_supported_empty_feed() {
    let mut report = Report {
        observed_unix_ms: 1,
        hosts: vec![attention::Host {
            endpoint: Endpoint {
                name: "Synthetic".into(),
                socket: "/tmp/legacy.sock".into(),
            },
            host_id: Some(Uuid::new_v4()),
            pending_sequence: None,
            items: vec![],
            error: None,
            completions: vec![],
            completion_evicted: None,
        }],
    };
    assert!(Observation::from_report(&report).is_empty());
    report.hosts[0].pending_sequence = Some(0);
    assert_eq!(Observation::from_report(&report).len(), 1);
    report.hosts[0].error = Some("offline".into());
    assert!(Observation::from_report(&report).is_empty());
}
