//! Bounded local workspace preferences. No filesystem inspection or Host RPC.
use crate::{
    client::Error,
    storage::{StorageError, Store},
    workspaces::{Workspace, decode, workspace_row},
};
use rusqlite::{Connection, TransactionBehavior};
use uuid::Uuid;

pub const MAX_PINNED_WORKSPACES: usize = 32;

fn read(database: &Connection) -> Result<Vec<Workspace>, StorageError> {
    let mut statement = database.prepare(
        "SELECT w.position,w.id,w.project,w.data FROM workspace_pins p \
         JOIN workspaces w ON w.id=p.workspace ORDER BY p.position DESC LIMIT ?1",
    )?;
    let pins: Vec<Workspace> = statement
        .query_map([i64::try_from(MAX_PINNED_WORKSPACES + 1)?], workspace_row)?
        .map(|row| decode(row?))
        .collect::<Result<_, _>>()?;
    if pins.len() > MAX_PINNED_WORKSPACES {
        return Err("workspace pin records exceed the supported limit of 32".into());
    }
    Ok(pins)
}

impl Store {
    pub async fn pinned_workspaces(&self) -> Result<Vec<Workspace>, Error> {
        self.job(|database| read(database)).await
    }

    /// Explicit assignment, not toggle. Repeated requests preserve order; all
    /// writers share the same transaction, including the capacity check.
    pub async fn set_workspace_pinned(
        &self,
        id: Uuid,
        pinned: bool,
    ) -> Result<Vec<Workspace>, Error> {
        self.job(move |database| {
            let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let id = id.to_string();
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id=?1)",
                [&id],
                |row| row.get(0),
            )?;
            if !exists {
                return Err("workspace not found".into());
            }
            if pinned {
                let present: bool = transaction.query_row(
                    "SELECT EXISTS(SELECT 1 FROM workspace_pins WHERE workspace=?1)",
                    [&id],
                    |row| row.get(0),
                )?;
                if !present {
                    let count: i64 = transaction.query_row(
                        "SELECT count(*) FROM workspace_pins",
                        [],
                        |row| row.get(0),
                    )?;
                    if count >= i64::try_from(MAX_PINNED_WORKSPACES)? {
                        return Err("32 workspaces are already pinned; unpin one first".into());
                    }
                    transaction
                        .execute("INSERT INTO workspace_pins(workspace) VALUES(?1)", [&id])?;
                }
            } else {
                transaction.execute("DELETE FROM workspace_pins WHERE workspace=?1", [&id])?;
            }
            let pins = read(&transaction)?;
            transaction.commit()?;
            Ok(pins)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hosts::Endpoint, workspaces::Kind};
    use std::path::PathBuf;

    async fn fixture() -> (PathBuf, Store) {
        let root = std::env::temp_dir().join(format!("fp-pins-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let store = Store::open(root.join("state")).await.unwrap();
        (root, store)
    }

    async fn register(store: &Store, path: PathBuf, host: &str) -> Workspace {
        store
            .register_workspace(
                Endpoint {
                    name: host.into(),
                    socket: PathBuf::from(format!("/tmp/{host}/control.sock")),
                },
                path,
                Kind::Folder,
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn pins_preserve_identity_order_and_offline_access_across_store_reopen() {
        let (root, store) = fixture().await;
        let a = register(&store, root.clone(), "A").await;
        let b = register(&store, root.clone(), "B").await;
        assert_ne!(a.id, b.id);
        store.set_workspace_pinned(a.id, true).await.unwrap();
        store.set_workspace_pinned(b.id, true).await.unwrap();
        let pins = store.set_workspace_pinned(a.id, true).await.unwrap();
        assert_eq!(pins.iter().map(|w| w.id).collect::<Vec<_>>(), [b.id, a.id]);
        let reopened = Store::open(root.join("state")).await.unwrap();
        assert_eq!(
            reopened
                .pinned_workspaces()
                .await
                .unwrap()
                .iter()
                .map(|w| w.id)
                .collect::<Vec<_>>(),
            [b.id, a.id]
        );
        assert_eq!(
            reopened.set_workspace_pinned(a.id, false).await.unwrap()[0].id,
            b.id
        );
        assert_eq!(
            reopened
                .set_workspace_pinned(a.id, false)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(reopened.workspace(a.id).await.unwrap().id, a.id);
        assert_eq!(reopened.workspaces(None).await.unwrap().len(), 2);
        assert!(
            reopened
                .set_workspace_pinned(Uuid::new_v4(), true)
                .await
                .is_err()
        );
        assert!(
            reopened
                .set_workspace_pinned(Uuid::new_v4(), false)
                .await
                .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn pins_outlive_pages_and_capacity_is_atomic_across_concurrent_writers() {
        let (root, store) = fixture().await;
        let mut ids = Vec::new();
        for index in 0..35 {
            let path = root.join(index.to_string());
            std::fs::create_dir(&path).unwrap();
            ids.push(register(&store, path, "Local").await.id);
        }
        store.set_workspace_pinned(ids[0], true).await.unwrap();
        assert!(
            !store
                .workspaces(None)
                .await
                .unwrap()
                .iter()
                .any(|w| w.id == ids[0])
        );
        assert_eq!(store.pinned_workspaces().await.unwrap()[0].id, ids[0]);
        store.set_workspace_pinned(ids[0], false).await.unwrap();
        let tasks: Vec<_> = ids
            .iter()
            .copied()
            .enumerate()
            .map(|(index, id)| {
                let store = store.clone();
                let directory = store.root().to_path_buf();
                tokio::spawn(async move {
                    let store = if index % 2 == 0 {
                        Store::open(directory).await?
                    } else {
                        store
                    };
                    store.set_workspace_pinned(id, true).await
                })
            })
            .collect();
        let mut successes = 0;
        for task in tasks {
            match task.await.unwrap() {
                Ok(_) => successes += 1,
                Err(Error::Input(message))
                    if message
                        == "Local state: 32 workspaces are already pinned; unpin one first" => {}
                Err(error) => panic!("unexpected pin failure: {error}"),
            }
        }
        assert_eq!(successes, MAX_PINNED_WORKSPACES);
        let pins = store.pinned_workspaces().await.unwrap();
        assert_eq!(pins.len(), MAX_PINNED_WORKSPACES);
        assert_eq!(
            store
                .set_workspace_pinned(pins[0].id, true)
                .await
                .unwrap()
                .len(),
            MAX_PINNED_WORKSPACES
        );
        let missing = *ids
            .iter()
            .find(|id| !pins.iter().any(|p| p.id == **id))
            .unwrap();
        assert!(store.set_workspace_pinned(missing, true).await.is_err());
        store.set_workspace_pinned(pins[0].id, false).await.unwrap();
        assert_eq!(
            store
                .set_workspace_pinned(missing, true)
                .await
                .unwrap()
                .len(),
            MAX_PINNED_WORKSPACES
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn v4_upgrade_preserves_catalog_and_drafts_and_rejects_broken_pin_schema() {
        let (root, store) = fixture().await;
        let workspace = register(&store, root.clone(), "Local").await;
        store.job(|db| {
            db.execute_batch("INSERT INTO drafts(key,revision,data) VALUES('saved',7,'exact legacy bytes'); DROP TABLE workspace_layout; DROP TABLE session_selection; DROP TABLE attention_reviews; DROP TABLE host_selection; DROP TABLE paseo_batch_results; DROP TABLE paseo_batches; DROP TABLE paseo_continuations; DROP TABLE paseo_sessions; DROP TABLE paseo_workspaces; DROP TABLE recent_directories; DROP TABLE appearance; DROP TABLE workspace_pins; PRAGMA user_version=4;")?;
            Ok(())
        }).await.unwrap();
        let reopened = Store::open(root.join("state")).await.unwrap();
        assert_eq!(
            reopened.workspace(workspace.id).await.unwrap().id,
            workspace.id
        );
        assert!(reopened.pinned_workspaces().await.unwrap().is_empty());
        reopened
            .job(|db| {
                assert_eq!(
                    db.query_row(
                        "SELECT data FROM drafts WHERE key='saved' AND revision=7",
                        [],
                        |row| row.get::<_, String>(0)
                    )?,
                    "exact legacy bytes"
                );
                assert_eq!(
                    db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?,
                    14
                );
                db.execute("DROP TABLE workspace_pins", [])?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(Store::open(root.join("state")).await.is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
