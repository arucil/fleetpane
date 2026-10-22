//! Bounded local workspace preferences. No filesystem inspection or Host RPC.
use crate::{
    client::Error,
    storage::{StorageError, Store},
    workspaces::{Workspace, decode, workspace_row},
};
use rusqlite::{Connection, TransactionBehavior};
use std::collections::HashSet;
use uuid::Uuid;

pub const MAX_PINNED_WORKSPACES: usize = 32;

pub(crate) fn read(database: &Connection) -> Result<Vec<Workspace>, StorageError> {
    let mut statement = database.prepare(
        "SELECT w.position,w.id,w.project,w.data,n.revision,n.value,pn.revision,pn.value FROM workspace_pins p \
         JOIN workspaces w ON w.id=p.workspace LEFT JOIN workspace_names n ON n.workspace=w.id LEFT JOIN project_names pn ON pn.project=w.project ORDER BY p.position DESC LIMIT ?1",
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
        self.job(move |database| update(database, id, Change::Assign(pinned)))
            .await
    }

    /// One atomic user action. Concurrent windows cannot read the same old pin
    /// and accidentally collapse two requested transitions into one assignment.
    pub async fn toggle_workspace_pinned(&self, id: Uuid) -> Result<Vec<Workspace>, Error> {
        self.job(move |database| update(database, id, Change::Toggle))
            .await
    }

    /// Replace only the order of the exact observed pin list. Membership changes
    /// and concurrent reorderings require an explicit read before another save.
    pub async fn reorder_workspace_pins(
        &self,
        expected: Vec<Uuid>,
        order: Vec<Uuid>,
    ) -> Result<Vec<Workspace>, Error> {
        self.job(move |database| {
            if expected.len() > MAX_PINNED_WORKSPACES
                || order.len() != expected.len()
                || expected.iter().collect::<HashSet<_>>().len() != expected.len()
                || order.iter().collect::<HashSet<_>>() != expected.iter().collect()
            {
                return Err(
                    "pin order must contain each observed workspace exactly once (at most 32)"
                        .into(),
                );
            }
            let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current = read(&transaction)?;
            if current
                .iter()
                .map(|workspace| workspace.id)
                .collect::<Vec<_>>()
                != expected
            {
                return Err(
                    "workspace pins changed in another window; reload pins before reordering"
                        .into(),
                );
            }
            if expected != order {
                transaction.execute("DELETE FROM workspace_pins", [])?;
                for id in order.iter().rev() {
                    transaction.execute(
                        "INSERT INTO workspace_pins(workspace) VALUES(?1)",
                        [id.to_string()],
                    )?;
                }
            }
            let pins = read(&transaction)?;
            transaction.commit()?;
            Ok(pins)
        })
        .await
    }
}

enum Change {
    Assign(bool),
    Toggle,
}

fn update(
    database: &mut Connection,
    id: Uuid,
    change: Change,
) -> Result<Vec<Workspace>, StorageError> {
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
    let pinned = match change {
        Change::Assign(pinned) => pinned,
        Change::Toggle => !transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM workspace_pins WHERE workspace=?1)",
            [&id],
            |row| row.get::<_, bool>(0),
        )?,
    };
    if pinned {
        let present: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM workspace_pins WHERE workspace=?1)",
            [&id],
            |row| row.get(0),
        )?;
        if !present {
            let count: i64 =
                transaction
                    .query_row("SELECT count(*) FROM workspace_pins", [], |row| row.get(0))?;
            if count >= i64::try_from(MAX_PINNED_WORKSPACES)? {
                return Err("32 workspaces are already pinned; unpin one first".into());
            }
            transaction.execute("INSERT INTO workspace_pins(workspace) VALUES(?1)", [&id])?;
        }
    } else {
        transaction.execute("DELETE FROM workspace_pins WHERE workspace=?1", [&id])?;
    }
    let pins = read(&transaction)?;
    transaction.commit()?;
    Ok(pins)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hosts::Endpoint, workspaces::Kind};
    use std::path::PathBuf;

    fn ids(pins: &[Workspace]) -> Vec<Uuid> {
        pins.iter().map(|workspace| workspace.id).collect()
    }

    #[tokio::test]
    async fn order_survives_reopen_and_pin_commands_keep_newest_first() {
        let (root, store) = fixture().await;
        let mut expected = vec![];
        for host in ["A", "B", "C"] {
            let workspace = register(&store, root.clone(), host).await;
            expected = ids(&store
                .set_workspace_pinned(workspace.id, true)
                .await
                .unwrap());
        }
        let catalog = serde_json::to_value(store.workspaces(None).await.unwrap()).unwrap();
        let order = vec![expected[1], expected[2], expected[0]];
        store
            .rename_workspace(order[0], 0, Some("研究 🚀".into()))
            .await
            .unwrap();
        let saved = store
            .reorder_workspace_pins(expected.clone(), order.clone())
            .await
            .unwrap();
        assert_eq!(ids(&saved), order);
        assert_eq!(saved[0].name.value.as_deref(), Some("研究 🚀"));
        let reopened = Store::open(root.join("state")).await.unwrap();
        assert_eq!(ids(&reopened.pinned_workspaces().await.unwrap()), order);
        assert_eq!(
            ids(&store.set_workspace_pinned(order[0], true).await.unwrap()),
            order
        );
        let remaining = ids(&store.toggle_workspace_pinned(order[1]).await.unwrap());
        assert_eq!(remaining, [order[0], order[2]]);
        assert!(
            store
                .reorder_workspace_pins(order.clone(), expected)
                .await
                .unwrap_err()
                .to_string()
                .contains("changed in another window")
        );
        assert_eq!(
            ids(&store.toggle_workspace_pinned(order[1]).await.unwrap()),
            [order[1], order[0], order[2]]
        );
        store.rename_workspace(order[0], 1, None).await.unwrap();
        let mut after = serde_json::to_value(store.workspaces(None).await.unwrap()).unwrap();
        for workspace in after.as_array_mut().unwrap() {
            workspace["name"]["revision"] = serde_json::json!(0);
        }
        assert_eq!(after, catalog);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn invalid_orders_and_mid_write_failure_preserve_every_pin() {
        let (root, store) = fixture().await;
        let a = register(&store, root.clone(), "A").await;
        let b = register(&store, root.clone(), "B").await;
        store.set_workspace_pinned(a.id, true).await.unwrap();
        let original = store.set_workspace_pinned(b.id, true).await.unwrap();
        let expected = ids(&original);
        for (old, order) in [
            (expected.clone(), vec![a.id]),
            (expected.clone(), vec![a.id, a.id]),
            (vec![a.id, a.id], vec![a.id, b.id]),
            (expected.clone(), vec![a.id, Uuid::new_v4()]),
            (vec![Uuid::new_v4(); 33], vec![Uuid::new_v4(); 33]),
        ] {
            assert!(store.reorder_workspace_pins(old, order).await.is_err());
            assert_eq!(
                serde_json::to_value(store.pinned_workspaces().await.unwrap()).unwrap(),
                serde_json::to_value(&original).unwrap()
            );
        }
        // Fail on the second insertion: the preceding DELETE and INSERT roll back.
        store.job(move |db| {
            db.execute_batch(&format!("CREATE TRIGGER refuse_order BEFORE INSERT ON workspace_pins WHEN NEW.workspace='{}' BEGIN SELECT RAISE(ABORT,'owned order refusal'); END", a.id))?;
            Ok(())
        }).await.unwrap();
        assert_eq!(
            ids(&store
                .reorder_workspace_pins(expected.clone(), expected.clone())
                .await
                .unwrap()),
            expected
        );
        assert!(
            store
                .reorder_workspace_pins(expected.clone(), vec![a.id, b.id])
                .await
                .unwrap_err()
                .to_string()
                .contains("owned order refusal")
        );
        assert_eq!(
            serde_json::to_value(store.pinned_workspaces().await.unwrap()).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn concurrent_orders_commit_once_and_empty_order_is_valid() {
        let (root, store) = fixture().await;
        assert!(
            store
                .reorder_workspace_pins(vec![], vec![])
                .await
                .unwrap()
                .is_empty()
        );
        let mut expected = vec![];
        for index in 0..32 {
            let workspace = register(&store, root.clone(), &format!("Host {index}")).await;
            expected = ids(&store
                .set_workspace_pinned(workspace.id, true)
                .await
                .unwrap());
        }
        let other = Store::open(root.join("state")).await.unwrap();
        let mut first = expected.clone();
        first.rotate_left(1);
        let mut second = expected.clone();
        second.reverse();
        let (a, b) = tokio::join!(
            store.reorder_workspace_pins(expected.clone(), first.clone()),
            other.reorder_workspace_pins(expected, second.clone())
        );
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        let (winner, loser) = match (a, b) {
            (Ok(_), Err(error)) => (first, error),
            (Err(error), Ok(_)) => (second, error),
            _ => panic!("exactly one changed order must commit"),
        };
        assert!(loser.to_string().contains("changed in another window"));
        assert_eq!(ids(&store.pinned_workspaces().await.unwrap()), winner);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn toggles_are_atomic_across_stores_and_failed_transitions_preserve_pins() {
        let (root, store) = fixture().await;
        let a = register(&store, root.clone(), "A").await;
        let b = register(&store, root.clone(), "B").await;
        store.set_workspace_pinned(b.id, true).await.unwrap();
        let other = Store::open(root.join("state")).await.unwrap();
        let (first, second) = tokio::join!(
            store.toggle_workspace_pinned(a.id),
            other.toggle_workspace_pinned(a.id)
        );
        let replies = [first.unwrap(), second.unwrap()];
        assert_eq!(
            replies
                .iter()
                .filter(|pins| pins.iter().any(|w| w.id == a.id))
                .count(),
            1
        );
        assert_eq!(
            store
                .pinned_workspaces()
                .await
                .unwrap()
                .iter()
                .map(|w| w.id)
                .collect::<Vec<_>>(),
            [b.id]
        );
        store.job(|db| { db.execute_batch("CREATE TRIGGER reject_pin BEFORE INSERT ON workspace_pins BEGIN SELECT RAISE(ABORT,'owned pin refusal'); END")?; Ok(()) }).await.unwrap();
        assert!(
            store
                .toggle_workspace_pinned(a.id)
                .await
                .unwrap_err()
                .to_string()
                .contains("owned pin refusal")
        );
        assert_eq!(store.pinned_workspaces().await.unwrap()[0].id, b.id);
        store
            .job(|db| {
                db.execute_batch("DROP TRIGGER reject_pin")?;
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(
            store.toggle_workspace_pinned(a.id).await.unwrap()[0].id,
            a.id
        );
        assert!(store.toggle_workspace_pinned(Uuid::new_v4()).await.is_err());
        assert_eq!(
            store.toggle_workspace_pinned(a.id).await.unwrap()[0].id,
            b.id
        );
        std::fs::remove_dir_all(root).unwrap();
    }

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
        assert!(store.toggle_workspace_pinned(missing).await.is_err());
        assert_eq!(
            serde_json::to_value(store.pinned_workspaces().await.unwrap()).unwrap(),
            serde_json::to_value(&pins).unwrap()
        );
        assert_eq!(
            store
                .toggle_workspace_pinned(pins[0].id)
                .await
                .unwrap()
                .len(),
            MAX_PINNED_WORKSPACES - 1
        );
        assert_eq!(
            store
                .toggle_workspace_pinned(pins[0].id)
                .await
                .unwrap()
                .len(),
            MAX_PINNED_WORKSPACES
        );
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
            db.execute_batch("INSERT INTO drafts(key,revision,data) VALUES('saved',7,'exact legacy bytes'); DROP TABLE project_names; DROP TABLE workspace_names; DROP TABLE pending_alert_cursors; DROP TABLE completion_alert_cursors; DROP TABLE notification_preferences; DROP TABLE workspace_layout; DROP TABLE session_selection; DROP TABLE attention_reviews; DROP TABLE host_selection; DROP TABLE paseo_batch_results; DROP TABLE paseo_batches; DROP TABLE paseo_continuations; DROP TABLE paseo_sessions; DROP TABLE paseo_workspaces; DROP TABLE recent_directories; DROP TABLE appearance; DROP TABLE workspace_pins; PRAGMA user_version=4;")?;
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
                    19
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
