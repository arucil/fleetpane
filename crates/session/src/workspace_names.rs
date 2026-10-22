//! Local display names. Renaming never inspects a directory or contacts a Host.
use crate::{
    client::Error,
    storage::{StorageError, Store},
    workspaces::{Workspace, decode, workspace_row},
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_NAME_BYTES: usize = 256;

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Saved {
    pub revision: u64,
    pub value: Option<String>,
}

impl Saved {
    pub(crate) fn check(&self) -> Result<(), StorageError> {
        self.check_for("workspace")
    }

    pub(crate) fn decode(
        revision: Option<i64>,
        value: Option<String>,
        subject: &str,
    ) -> Result<Self, StorageError> {
        let name = match (revision, value) {
            (None, None) => Self::default(),
            (Some(revision), value) if revision > 0 => Self {
                revision: revision.try_into()?,
                value,
            },
            _ => return Err(format!("invalid {subject} name revision").into()),
        };
        name.check_for(subject)?;
        Ok(name)
    }

    pub(crate) fn check_for(&self, subject: &str) -> Result<(), StorageError> {
        if let Some(value) = &self.value
            && (value.is_empty()
                || value.len() > MAX_NAME_BYTES
                || value.trim() != value
                || value
                    .chars()
                    .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')))
        {
            return Err(format!("{subject} name must be 1–256 UTF-8 bytes, on one line, without surrounding whitespace or control characters").into());
        }
        Ok(())
    }
}

impl Store {
    /// Compare and set the observed revision. None explicitly restores the
    /// directory/branch label; retaining the row prevents reset/rename ABA.
    pub async fn rename_workspace(
        &self,
        id: Uuid,
        expected_revision: u64,
        value: Option<String>,
    ) -> Result<Workspace, Error> {
        self.job(move |database| {
            let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let workspace = decode(transaction.query_row(
                "SELECT w.position,w.id,w.project,w.data,n.revision,n.value,pn.revision,pn.value FROM workspaces w LEFT JOIN workspace_names n ON n.workspace=w.id LEFT JOIN project_names pn ON pn.project=w.project WHERE w.id=?1",
                [id.to_string()], workspace_row,
            ).optional()?.ok_or("workspace not found")?)?;
            if workspace.name.revision != expected_revision {
                return Err("workspace name changed in another window; reload the name before saving".into());
            }
            let revision = expected_revision.checked_add(1)
                .and_then(|n| i64::try_from(n).ok()).ok_or("workspace name revision exhausted")?;
            let name = Saved { revision: revision.try_into()?, value };
            name.check()?;
            transaction.execute(
                "INSERT INTO workspace_names(workspace,revision,value) VALUES(?1,?2,?3) ON CONFLICT(workspace) DO UPDATE SET revision=excluded.revision,value=excluded.value",
                params![id.to_string(), revision, name.value],
            )?;
            transaction.commit()?;
            Ok(Workspace { name, ..workspace })
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hosts::Endpoint, workspaces::Kind};
    use std::path::PathBuf;

    async fn fixture() -> (PathBuf, Store, Workspace) {
        let root = std::env::temp_dir().join(format!("fp-names-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let store = Store::open(root.join("state")).await.unwrap();
        let workspace = store
            .register_workspace(
                Endpoint {
                    name: "Offline".into(),
                    socket: "/tmp/fp-names-offline.sock".into(),
                },
                root.clone(),
                Kind::Folder,
            )
            .await
            .unwrap();
        (root, store, workspace)
    }

    #[tokio::test]
    async fn names_preserve_exact_catalog_identity_pins_and_registration_refresh() {
        let (root, store, original) = fixture().await;
        let renamed = store
            .rename_workspace(original.id, 0, Some("研究 / release 🚀".into()))
            .await
            .unwrap();
        assert_eq!(renamed.name.value.as_deref(), Some("研究 / release 🚀"));
        assert_eq!(renamed.display_name(), "研究 / release 🚀");
        assert_eq!(
            serde_json::to_value(&original.registration).unwrap(),
            serde_json::to_value(&renamed.registration).unwrap()
        );
        assert_eq!(
            (renamed.id, renamed.project_id, renamed.position),
            (original.id, original.project_id, original.position)
        );
        let pins = store.set_workspace_pinned(original.id, true).await.unwrap();
        assert_eq!(pins[0].name, renamed.name);
        let refreshed = store
            .register_workspace(
                original.registration.endpoint.clone(),
                root.clone(),
                Kind::Folder,
            )
            .await
            .unwrap();
        assert_eq!(refreshed.name, renamed.name);
        let reopened = Store::open(root.join("state")).await.unwrap();
        assert_eq!(
            reopened.workspaces(None).await.unwrap()[0].name,
            renamed.name
        );
        assert_eq!(
            reopened
                .workspace_at(
                    &original.registration.endpoint.socket,
                    &original.registration.directory.path,
                )
                .await
                .unwrap()
                .unwrap()
                .name,
            renamed.name
        );
        // Name edits use the saved UUID even while its original directory is gone.
        let moved = root.with_extension("moved");
        std::fs::rename(&root, &moved).unwrap();
        let offline = Store::open(moved.join("state")).await.unwrap();
        let reset = offline
            .rename_workspace(original.id, 1, None)
            .await
            .unwrap();
        assert_eq!(
            reset.name,
            Saved {
                revision: 2,
                value: None
            }
        );
        assert_eq!(reset.display_name(), original.display_name());
        assert!(
            offline
                .rename_workspace(original.id, 0, Some("stale".into()))
                .await
                .is_err()
        );
        assert!(
            offline
                .rename_workspace(Uuid::new_v4(), 0, None)
                .await
                .is_err()
        );
        std::fs::remove_dir_all(moved).unwrap();
    }

    #[tokio::test]
    async fn concurrent_compare_and_set_and_failed_write_never_overwrite_a_name() {
        let (root, store, w) = fixture().await;
        let other = Store::open(root.join("state")).await.unwrap();
        let (a, b) = tokio::join!(
            store.rename_workspace(w.id, 0, Some("A".into())),
            other.rename_workspace(w.id, 0, Some("B".into()))
        );
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        let before = store.workspace(w.id).await.unwrap().name;
        store.job(|db| { db.execute_batch("CREATE TRIGGER refuse_name BEFORE UPDATE ON workspace_names BEGIN SELECT RAISE(ABORT,'owned name refusal'); END;")?; Ok(()) }).await.unwrap();
        assert!(
            store
                .rename_workspace(w.id, 1, Some("lost".into()))
                .await
                .unwrap_err()
                .to_string()
                .contains("owned name refusal")
        );
        assert_eq!(store.workspace(w.id).await.unwrap().name, before);
        store
            .job(|db| {
                db.execute_batch("DROP TRIGGER refuse_name;")?;
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(
            store
                .rename_workspace(w.id, 1, None)
                .await
                .unwrap()
                .name
                .revision,
            2
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn validation_and_corrupt_persisted_names_are_explicit() {
        let (root, store, w) = fixture().await;
        for value in [
            "".to_owned(),
            " ".into(),
            " a".into(),
            "a ".into(),
            "a\nb".into(),
            "a\u{7f}".into(),
            "a\u{2028}b".into(),
            "界".repeat(86),
        ] {
            assert!(store.rename_workspace(w.id, 0, Some(value)).await.is_err());
            assert_eq!(store.workspace(w.id).await.unwrap().name.revision, 0);
        }
        assert_eq!(
            store
                .rename_workspace(w.id, 0, Some("x".repeat(MAX_NAME_BYTES)))
                .await
                .unwrap()
                .name
                .revision,
            1
        );
        store
            .job(|db| {
                db.execute("UPDATE workspace_names SET value=?1", ["x".repeat(257)])?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(store.workspace(w.id).await.is_err());
        assert!(store.workspaces(None).await.is_err());
        assert!(store.rename_workspace(w.id, 1, None).await.is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn v17_upgrade_keeps_all_existing_state_and_missing_table_is_an_error() {
        let (root, store, w) = fixture().await;
        store.set_workspace_pinned(w.id, true).await.unwrap();
        store.job(|db| { db.execute_batch("INSERT INTO drafts VALUES('kept',7,'草稿 exact  '); INSERT INTO pending_alert_cursors VALUES('socket','host',12); DROP TABLE project_names; DROP TABLE workspace_names; PRAGMA user_version=17;")?; Ok(()) }).await.unwrap();
        let reopened = Store::open(root.join("state")).await.unwrap();
        assert_eq!(
            reopened.workspace(w.id).await.unwrap().name,
            Saved::default()
        );
        assert_eq!(reopened.pinned_workspaces().await.unwrap()[0].id, w.id);
        reopened
            .job(|db| {
                assert_eq!(
                    db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))?,
                    19
                );
                assert_eq!(
                    db.query_row(
                        "SELECT data FROM drafts WHERE key='kept' AND revision=7",
                        [],
                        |r| r.get::<_, String>(0)
                    )?,
                    "草稿 exact  "
                );
                assert_eq!(
                    db.query_row("SELECT sequence FROM pending_alert_cursors", [], |r| r
                        .get::<_, i64>(0))?,
                    12
                );
                db.execute_batch("DROP TABLE project_names; DROP TABLE workspace_names;")?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(Store::open(root.join("state")).await.is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
