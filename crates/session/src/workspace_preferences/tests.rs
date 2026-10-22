use super::*;
use crate::{hosts::Endpoint, workspaces::Kind};
use std::path::PathBuf;

async fn fixture() -> (PathBuf, Store, Workspace, Workspace) {
    let root = std::env::temp_dir().join(format!("fp-workspace-observe-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let store = Store::open(root.join("state")).await.unwrap();
    let a = store
        .register_workspace(
            Endpoint {
                name: "Offline A".into(),
                socket: root.join("a.sock"),
            },
            root.clone(),
            Kind::Folder,
        )
        .await
        .unwrap();
    let b = store
        .register_workspace(
            Endpoint {
                name: "Offline B".into(),
                socket: root.join("b.sock"),
            },
            root.clone(),
            Kind::Folder,
        )
        .await
        .unwrap();
    (root, store, a, b)
}

#[tokio::test]
async fn other_handle_changes_refresh_names_order_and_membership_without_writing() {
    let (root, store, a, b) = fixture().await;
    let other = Store::open(root.join("state")).await.unwrap();
    other.set_workspace_pinned(a.id, true).await.unwrap();
    other.set_workspace_pinned(b.id, true).await.unwrap();
    let initial = store.workspace_preferences(vec![a.id, b.id]).await.unwrap();
    assert_eq!(
        initial.pins.iter().map(|w| w.id).collect::<Vec<_>>(),
        [b.id, a.id]
    );
    other
        .rename_project(a.project_id, 0, Some("研究 🚀".into()))
        .await
        .unwrap();
    other
        .rename_workspace(b.id, 0, Some("独立".into()))
        .await
        .unwrap();
    other
        .reorder_workspace_pins(vec![b.id, a.id], vec![a.id, b.id])
        .await
        .unwrap();
    let fresh = store.workspace_preferences(vec![b.id, a.id]).await.unwrap();
    assert_eq!(fresh.workspaces[0].name.value.as_deref(), Some("独立"));
    assert_eq!(
        fresh.workspaces[1].project_name.value.as_deref(),
        Some("研究 🚀")
    );
    assert_eq!(fresh.workspaces[0].project_name.revision, 0);
    assert_eq!(
        fresh.pins.iter().map(|w| w.id).collect::<Vec<_>>(),
        [a.id, b.id]
    );
    // Observation cannot silently authorize an editor that still holds revision 0.
    assert!(
        other
            .rename_project(a.project_id, a.project_name.revision, None)
            .await
            .unwrap_err()
            .to_string()
            .contains("changed in another window")
    );
    other.rename_project(a.project_id, 1, None).await.unwrap();
    other.set_workspace_pinned(a.id, false).await.unwrap();
    let fresh = store.workspace_preferences(vec![a.id]).await.unwrap();
    assert_eq!(fresh.workspaces[0].project_name.revision, 2);
    assert_eq!(fresh.workspaces[0].project_name.value, None);
    assert_eq!(fresh.pins.len(), 1);
    assert_eq!(fresh.pins[0].id, b.id);
    drop(other);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn bounded_observation_refuses_unknown_duplicate_and_corrupt_identities() {
    let (root, store, a, b) = fixture().await;
    for ids in [
        vec![a.id, a.id],
        vec![Uuid::new_v4()],
        (0..=MAX_OBSERVED_WORKSPACES)
            .map(|_| Uuid::new_v4())
            .collect(),
    ] {
        assert!(store.workspace_preferences(ids).await.is_err());
    }
    store.set_workspace_pinned(b.id, true).await.unwrap();
    let a_id = a.id.to_string();
    store
        .job(move |db| {
            db.execute(
                "INSERT INTO workspace_names(workspace,revision,value) VALUES(?1,1,?2)",
                rusqlite::params![a_id, "x".repeat(257)],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    assert!(store.workspace_preferences(vec![a.id]).await.is_err());
    assert_eq!(
        store.workspace_preferences(vec![]).await.unwrap().pins[0].id,
        b.id
    );
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn missing_directory_observation_preserves_database_bytes_and_identity() {
    let (root, store, a, _) = fixture().await;
    let path = root.join("gone");
    std::fs::create_dir(&path).unwrap();
    let workspace = store
        .register_workspace(a.registration.endpoint.clone(), path.clone(), Kind::Folder)
        .await
        .unwrap();
    store
        .set_workspace_pinned(workspace.id, true)
        .await
        .unwrap();
    std::fs::remove_dir(&path).unwrap();
    let database = root.join("state/creations.sqlite3");
    let before = std::fs::read(&database).unwrap();
    let snapshot = store
        .workspace_preferences(vec![workspace.id])
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(&snapshot.workspaces[0]).unwrap(),
        serde_json::to_value(&workspace).unwrap()
    );
    assert_eq!(snapshot.pins[0].id, workspace.id);
    assert_eq!(std::fs::read(&database).unwrap(), before);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn workspace_and_pin_labels_come_from_one_read_transaction() {
    let (root, store, a, _) = fixture().await;
    store.set_workspace_pinned(a.id, true).await.unwrap();
    let writer = Store::open(root.join("state")).await.unwrap();
    let update = tokio::spawn(async move {
        for revision in 0..40 {
            writer
                .rename_workspace(a.id, revision, Some(format!("label {revision}")))
                .await
                .unwrap();
        }
    });
    for _ in 0..40 {
        let snapshot = store.workspace_preferences(vec![a.id]).await.unwrap();
        assert_eq!(snapshot.workspaces[0].name, snapshot.pins[0].name);
    }
    update.await.unwrap();
    assert_eq!(
        store
            .workspace_preferences(vec![a.id])
            .await
            .unwrap()
            .workspaces[0]
            .name
            .revision,
        40
    );
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
