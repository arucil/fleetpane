use super::*;
use serde_json::json;

fn session() -> Session {
    Session {
        socket: "/tmp/alerts.sock".into(),
        host_id: Uuid::new_v4(),
        thread_id: "same".into(),
    }
}
fn allowed(p: &Preferences, s: &Session, k: Kind) -> bool {
    p.allows(k, &s.socket, s.host_id, Some(&s.thread_id))
}

#[test]
fn complete_mute_dominates_every_kind_and_unknown_session_without_changing_sources() {
    let s = session();
    let mut p = Preferences::default();
    for k in Kind::ALL {
        assert!(allowed(&p, &s, k));
    }
    p.muted = true;
    for k in Kind::ALL {
        assert!(!allowed(&p, &s, k));
        assert!(!p.allows(k, &s.socket, s.host_id, None));
    }
    p.muted = false;
    p.muted_kinds.push(Kind::Failed);
    assert!(!allowed(&p, &s, Kind::Failed));
    assert!(allowed(&p, &s, Kind::Approval));
    p.muted_hosts.push(s.socket.clone());
    for k in Kind::ALL {
        assert!(!allowed(&p, &s, k));
    }
}

#[test]
fn session_mute_uses_endpoint_incarnation_and_thread_not_labels_or_request_ids() {
    let s = session();
    let mut p = Preferences::default();
    p.muted_sessions.push(s.clone());
    assert!(!allowed(&p, &s, Kind::Approval));
    let mut other = s.clone();
    other.host_id = Uuid::new_v4();
    assert!(allowed(&p, &other, Kind::Approval));
    other = s.clone();
    other.socket = "/tmp/second.sock".into();
    assert!(allowed(&p, &other, Kind::Approval));
    other = s.clone();
    other.thread_id = "second".into();
    assert!(allowed(&p, &other, Kind::Approval));
    assert!(p.allows(Kind::Approval, &s.socket, s.host_id, None));
}

#[test]
fn workspace_policy_covers_new_sessions_but_never_guesses_unknown_or_neighbor_paths() {
    let s = session();
    let workspace = Workspace {
        socket: s.socket.clone(),
        directory: "/workspace 中文".into(),
    };
    let p = Preferences {
        muted_workspaces: vec![workspace.clone()],
        ..Default::default()
    };
    p.validate().unwrap();
    let mut item = attention::Item {
        request_sequence: None,
        observed_unix_ms: None,
        endpoint: crate::hosts::Endpoint {
            name: "Display name".into(),
            socket: s.socket.clone(),
        },
        host_id: s.host_id,
        id: json!(1),
        method: "future".into(),
        kind: attention::Kind::Approval,
        thread_id: Some(s.thread_id.clone()),
        cwd: Some("/workspace 中文".into()),
        turn_id: None,
        title: "Title".into(),
        request: json!({"opaque":true}),
    };
    let before = serde_json::to_value(&item).unwrap();
    for kind in [
        attention::Kind::Approval,
        attention::Kind::Question,
        attention::Kind::Other,
    ] {
        item.kind = kind;
        assert!(!p.pending_allowed(&item));
    }
    item.kind = attention::Kind::Approval;
    assert_eq!(serde_json::to_value(&item).unwrap(), before);
    item.thread_id = Some("new session".into());
    item.host_id = Uuid::new_v4();
    item.endpoint.name = "Renamed".into();
    assert!(!p.pending_allowed(&item));
    item.cwd = Some("/workspace 中文/child".into());
    assert!(p.pending_allowed(&item));
    item.cwd = Some("/workspace 中文-suffix".into());
    assert!(p.pending_allowed(&item));
    item.cwd = None;
    assert!(p.pending_allowed(&item));
    item.cwd = Some("/workspace 中文".into());
    item.endpoint.socket = "/tmp/other.sock".into();
    assert!(p.pending_allowed(&item));
    let mut result = attention::review::Item {
        endpoint: crate::hosts::Endpoint {
            name: "Same".into(),
            socket: s.socket,
        },
        host_id: s.host_id,
        cwd: Some("/workspace 中文".into()),
        reviewed: false,
        record: fleetpane_runtime::attention::Record {
            sequence: 1,
            observed_unix_ms: 42,
            thread_id: "new session".into(),
            turn_id: "turn".into(),
            kind: fleetpane_runtime::attention::Kind::Completed,
            stop_reason: None,
        },
    };
    for kind in [
        fleetpane_runtime::attention::Kind::Completed,
        fleetpane_runtime::attention::Kind::Failed,
        fleetpane_runtime::attention::Kind::Interrupted,
    ] {
        result.record.kind = kind;
        assert!(!p.result_allowed(&result));
        assert!(!result.reviewed);
    }
    result.cwd = None;
    assert!(p.result_allowed(&result));
    let mut invalid = p.clone();
    invalid.muted_workspaces.push(workspace);
    assert!(invalid.validate().is_err());
    invalid.muted_workspaces.pop();
    invalid.muted_workspaces[0].directory = "relative".into();
    assert!(invalid.validate().is_err());
    invalid.muted_workspaces[0].directory = "/exact/../neighbor".into();
    assert!(invalid.validate().is_err());
    invalid.muted_workspaces[0].directory = "/a\0b".into();
    assert!(invalid.validate().is_err());
}

#[tokio::test]
async fn old_policy_is_compatible_and_workspace_rules_share_cas_without_schema_change() {
    let root = std::env::temp_dir().join(format!("fp-workspace-alert-{}", Uuid::new_v4()));
    let store = Store::open(root.clone()).await.unwrap();
    store
        .job(|db| {
            db.execute(
                "INSERT INTO notification_preferences VALUES(1,7,?1)",
                [r#"{"muted":false,"muted_kinds":[],"muted_hosts":[],"muted_sessions":[]}"#],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    let old = store.notifications().await.unwrap();
    assert!(old.value.muted_workspaces.is_empty());
    let mut value = old.value;
    value.muted_workspaces.push(Workspace {
        socket: "/tmp/missing.sock".into(),
        directory: "/missing 中文".into(),
    });
    let saved = store.save_notifications(7, value.clone()).await.unwrap();
    assert_eq!(saved.revision, 8);
    assert_eq!(
        Store::open(root.clone())
            .await
            .unwrap()
            .notifications()
            .await
            .unwrap(),
        saved
    );
    assert!(
        store
            .save_notifications(7, Preferences::default())
            .await
            .is_err()
    );
    value.muted_workspaces = vec![value.muted_workspaces[0].clone(); 65];
    assert!(store.save_notifications(8, value).await.is_err());
    assert_eq!(store.notifications().await.unwrap(), saved);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn muting_does_not_review_results_or_rewrite_failure() {
    let s = session();
    let item = attention::review::Item {
        endpoint: crate::hosts::Endpoint {
            name: "Synthetic".into(),
            socket: s.socket.clone(),
        },
        host_id: s.host_id,
        reviewed: false,
        cwd: None,
        record: fleetpane_runtime::attention::Record {
            sequence: 1,
            observed_unix_ms: 42,
            thread_id: s.thread_id.clone(),
            turn_id: "turn".into(),
            kind: fleetpane_runtime::attention::Kind::Failed,
            stop_reason: None,
        },
    };
    let original = serde_json::to_value(&item).unwrap();
    let mut p = Preferences::default();
    assert!(p.result_allowed(&item));
    p.muted = true;
    assert!(!p.result_allowed(&item));
    assert_eq!(serde_json::to_value(&item).unwrap(), original);
    p.muted = false;
    let mut reviewed = item;
    reviewed.reviewed = true;
    assert!(!p.result_allowed(&reviewed));
}

#[test]
fn invalid_duplicate_and_excessive_scopes_are_rejected() {
    let s = session();
    let mut p = Preferences {
        muted_sessions: vec![s.clone(), s.clone()],
        ..Default::default()
    };
    assert!(p.validate().is_err());
    p.muted_sessions = vec![s];
    p.muted_sessions[0].host_id = Uuid::nil();
    assert!(p.validate().is_err());
    p = Preferences::default();
    p.muted_hosts.push("relative".into());
    assert!(p.validate().is_err());
    p = Preferences::default();
    p.muted_kinds = vec![Kind::Failed; 7];
    assert!(p.validate().is_err());
    assert!(serde_json::from_value::<Preferences>(json!({"muted":false,"muted_kinds":[],"muted_hosts":[],"muted_sessions":[],"future":true})).is_err());
}

#[test]
fn pending_policy_preserves_opaque_payloads_and_accepts_only_string_event_types() {
    let s = session();
    let mut item = attention::Item {
        request_sequence: None,
        observed_unix_ms: None,
        endpoint: crate::hosts::Endpoint {
            name: "Synthetic".into(),
            socket: s.socket.clone(),
        },
        host_id: s.host_id,
        id: json!(0),
        method: "future/method".into(),
        kind: attention::Kind::Other,
        thread_id: Some(s.thread_id.clone()),
        cwd: None,
        turn_id: None,
        title: "Future request".into(),
        request: json!({"opaque":{"future":true}}),
    };
    let p = Preferences {
        muted_kinds: vec![Kind::Other],
        ..Default::default()
    };
    let before = serde_json::to_value(&item).unwrap();
    assert!(!p.pending_allowed(&item));
    assert_eq!(serde_json::to_value(&item).unwrap(), before);
    item.kind = attention::Kind::Approval;
    assert!(p.pending_allowed(&item));
    item.kind = attention::Kind::Question;
    assert!(p.pending_allowed(&item));
    for kind in Kind::ALL {
        assert_eq!(
            serde_json::from_value::<Kind>(serde_json::to_value(kind).unwrap()).unwrap(),
            kind
        );
    }
    for invalid in [json!({"failed":null}), json!("future"), json!(true)] {
        assert!(serde_json::from_value::<Kind>(invalid).is_err());
    }
}

#[tokio::test]
async fn oversized_and_unknown_persisted_records_and_exhausted_revisions_are_not_defaulted() {
    let root = std::env::temp_dir().join(format!("fp-alert-invalid-{}", Uuid::new_v4()));
    let store = Store::open(root.clone()).await.unwrap();
    for data in [
        "x".repeat(524289),
        serde_json::to_string(
            &json!({"muted":false,"muted_kinds":["future"],"muted_hosts":[],"muted_sessions":[]}),
        )
        .unwrap(),
    ] {
        store.job(move|db|{db.execute("INSERT INTO notification_preferences VALUES(1,1,?1) ON CONFLICT(singleton) DO UPDATE SET data=excluded.data",[data])?;Ok(())}).await.unwrap();
        assert!(store.notifications().await.is_err());
        assert!(
            store
                .save_notifications(1, Preferences::default())
                .await
                .is_err()
        );
    }
    store
        .job(|db| {
            db.execute(
                "UPDATE notification_preferences SET revision=?1,data=?2",
                params![i64::MAX, serde_json::to_string(&Preferences::default())?],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    assert!(
        store
            .save_notifications(u64::try_from(i64::MAX).unwrap(), Preferences::default())
            .await
            .is_err()
    );
    assert_eq!(
        store.notifications().await.unwrap().revision,
        u64::try_from(i64::MAX).unwrap()
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn persistence_cas_storage_failure_and_corruption_are_explicit() {
    let root = std::env::temp_dir().join(format!("fp-alerts-{}", Uuid::new_v4()));
    let store = Store::open(root.clone()).await.unwrap();
    assert_eq!(store.notifications().await.unwrap(), Saved::default());
    let p = Preferences {
        muted: true,
        muted_sessions: vec![session()],
        ..Default::default()
    };
    let saved = store.save_notifications(0, p.clone()).await.unwrap();
    let other = Store::open(root.clone()).await.unwrap();
    assert_eq!(other.notifications().await.unwrap(), saved);
    let (a, b) = tokio::join!(
        store.save_notifications(1, Preferences::default()),
        other.save_notifications(1, p)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    let current = store.notifications().await.unwrap();
    assert_eq!(current.revision, 2);
    store.job(|db|{db.execute_batch("CREATE TRIGGER reject_alerts BEFORE UPDATE ON notification_preferences BEGIN SELECT RAISE(ABORT,'injected alert failure'); END;")?;Ok(())}).await.unwrap();
    assert!(
        store
            .save_notifications(2, Preferences::default())
            .await
            .is_err()
    );
    assert_eq!(other.notifications().await.unwrap(), current);
    store
        .job(|db| {
            db.execute_batch(
                "DROP TRIGGER reject_alerts; UPDATE notification_preferences SET data='invalid';",
            )?;
            Ok(())
        })
        .await
        .unwrap();
    assert!(store.notifications().await.is_err());
    assert!(
        store
            .save_notifications(2, Preferences::default())
            .await
            .is_err()
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn schema_fourteen_upgrade_preserves_drafts_and_current_table_damage_is_not_defaulted() {
    let root = std::env::temp_dir().join(format!("fp-alert-upgrade-{}", Uuid::new_v4()));
    let store = Store::open(root.clone()).await.unwrap();
    store.job(|db|{db.execute_batch("INSERT INTO drafts VALUES('retained',7,'exact draft'); DROP TABLE pending_alert_cursors; DROP TABLE completion_alert_cursors; DROP TABLE notification_preferences; PRAGMA user_version=14;")?;Ok(())}).await.unwrap();
    let upgraded = Store::open(root.clone()).await.unwrap();
    assert_eq!(upgraded.notifications().await.unwrap(), Saved::default());
    upgraded
        .job(|db| {
            assert_eq!(
                db.query_row("SELECT data FROM drafts", [], |r| r.get::<_, String>(0))?,
                "exact draft"
            );
            db.execute_batch("DROP TABLE notification_preferences;")?;
            Ok(())
        })
        .await
        .unwrap();
    assert!(Store::open(root.clone()).await.is_err());
    std::fs::remove_dir_all(root).unwrap();
}
