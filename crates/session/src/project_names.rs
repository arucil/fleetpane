//! Device-local project labels, shared by saved workspaces in that project.
use crate::{client::Error, storage::Store, workspace_names::Saved};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

impl Store {
    /// Compare and set a saved project UUID without inspecting paths or Hosts.
    /// Reset retains a revision so stale edits cannot overwrite a reset name.
    pub async fn rename_project(
        &self,
        id: Uuid,
        expected_revision: u64,
        value: Option<String>,
    ) -> Result<Saved, Error> {
        self.job(move |database| {
            let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let (revision, current): (Option<i64>, Option<String>) = transaction.query_row(
                "SELECT n.revision,n.value FROM projects p LEFT JOIN project_names n ON n.project=p.id WHERE p.id=?1",
                [id.to_string()], |row| {
                    if let rusqlite::types::ValueRef::Text(value) = row.get_ref(1)?
                        && value.len() > crate::workspace_names::MAX_NAME_BYTES {
                        return Err(rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text,
                            std::io::Error::new(std::io::ErrorKind::InvalidData, "project name exceeds 256 bytes").into()));
                    }
                    Ok((row.get(0)?, row.get(1)?))
                },
            ).optional()?.ok_or("project not found")?;
            let saved = Saved::decode(revision, current, "project")?;
            if saved.revision != expected_revision {
                return Err("project name changed in another window; reload the name before saving".into());
            }
            let revision = expected_revision.checked_add(1).and_then(|n| i64::try_from(n).ok())
                .ok_or("project name revision exhausted")?;
            let name = Saved { revision: revision.try_into()?, value };
            name.check_for("project")?;
            transaction.execute(
                "INSERT INTO project_names(project,revision,value) VALUES(?1,?2,?3) ON CONFLICT(project) DO UPDATE SET revision=excluded.revision,value=excluded.value",
                params![id.to_string(), revision, name.value],
            )?;
            transaction.commit()?;
            Ok(name)
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        hosts::Endpoint,
        workspaces::{Kind, Workspace},
    };
    use std::path::{Path, PathBuf};

    async fn fixture() -> (PathBuf, Store, Workspace) {
        let root = std::env::temp_dir().join(format!("fp-project-names-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let store = Store::open(root.join("state")).await.unwrap();
        let workspace = store
            .register_workspace(endpoint("Offline"), root.clone(), Kind::Folder)
            .await
            .unwrap();
        (root, store, workspace)
    }
    fn endpoint(name: &str) -> Endpoint {
        Endpoint {
            name: name.into(),
            socket: PathBuf::from(format!("/tmp/fp-project-{name}.sock")),
        }
    }
    fn git(root: &Path, args: &[&str]) {
        let result = std::process::Command::new("git")
            .current_dir(root)
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[tokio::test]
    async fn shared_git_project_and_independent_host_and_folder_keep_identity() {
        let (root, store, folder) = fixture().await;
        git(&root, &["init", "-q"]);
        git(&root, &["commit", "--allow-empty", "-qm", "fixture"]);
        let linked = root.join("linked");
        git(
            &root,
            &[
                "worktree",
                "add",
                "-qb",
                "fixture-linked",
                linked.to_str().unwrap(),
            ],
        );
        let main = store
            .register_workspace(endpoint("Offline"), root.clone(), Kind::Git)
            .await
            .unwrap();
        let sibling = store
            .register_workspace(endpoint("Offline"), linked.clone(), Kind::Git)
            .await
            .unwrap();
        let other = store
            .register_workspace(endpoint("Other"), root.clone(), Kind::Git)
            .await
            .unwrap();
        assert_eq!(main.project_id, sibling.project_id);
        assert_ne!(main.project_id, folder.project_id);
        assert_ne!(main.project_id, other.project_id);
        store
            .rename_workspace(sibling.id, 0, Some("Worktree only".into()))
            .await
            .unwrap();
        store.set_workspace_pinned(sibling.id, true).await.unwrap();
        let name = store
            .rename_project(main.project_id, 0, Some("研究 / fleet 🚀".into()))
            .await
            .unwrap();
        let fresh = store.workspace(main.id).await.unwrap();
        let sibling_fresh = store.workspace(sibling.id).await.unwrap();
        assert_eq!(sibling_fresh.project_name, name);
        assert_eq!(sibling_fresh.name.value.as_deref(), Some("Worktree only"));
        assert_eq!(fresh.project_display_name(), "研究 / fleet 🚀");
        assert_eq!(
            store.workspace(other.id).await.unwrap().project_name,
            Saved::default()
        );
        assert_eq!(
            store.workspace(folder.id).await.unwrap().project_name,
            Saved::default()
        );
        assert_eq!(
            store.pinned_workspaces().await.unwrap()[0].project_name,
            name
        );
        assert_eq!(
            store
                .register_workspace(endpoint("Offline"), root.clone(), Kind::Git)
                .await
                .unwrap()
                .project_name,
            name
        );
        let mut cached = sibling_fresh.clone();
        store
            .rename_project(main.project_id, 1, None)
            .await
            .unwrap();
        let reset = store.workspace(main.id).await.unwrap();
        cached.update_names_from(&reset);
        cached.update_names_from(&fresh); // older project observation cannot revive a reset
        assert_eq!(cached.project_name, reset.project_name);
        assert_eq!(cached.name, sibling_fresh.name);
        assert_eq!(
            serde_json::to_value(&cached.registration).unwrap(),
            serde_json::to_value(&sibling.registration).unwrap()
        );
        assert_eq!(cached.id, sibling.id);
        assert!(
            store
                .rename_project(main.project_id, 0, Some("stale".into()))
                .await
                .is_err()
        );
        let moved = root.with_extension("moved");
        std::fs::rename(&root, &moved).unwrap();
        let reopened = Store::open(moved.join("state")).await.unwrap();
        assert_eq!(
            reopened
                .rename_project(main.project_id, 2, Some("Offline".into()))
                .await
                .unwrap()
                .revision,
            3
        );
        assert_eq!(
            reopened
                .workspace(sibling.id)
                .await
                .unwrap()
                .project_name
                .value
                .as_deref(),
            Some("Offline")
        );
        std::fs::remove_dir_all(moved).unwrap();
    }

    #[tokio::test]
    async fn compare_and_set_is_atomic_and_failed_write_keeps_saved_name() {
        let (root, store, w) = fixture().await;
        let other = Store::open(root.join("state")).await.unwrap();
        let (a, b) = tokio::join!(
            store.rename_project(w.project_id, 0, Some("A".into())),
            other.rename_project(w.project_id, 0, Some("B".into()))
        );
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        let before = store.workspace(w.id).await.unwrap().project_name;
        store.job(|db|{db.execute_batch("CREATE TRIGGER refuse_project BEFORE UPDATE ON project_names BEGIN SELECT RAISE(ABORT,'owned project refusal'); END;")?;Ok(())}).await.unwrap();
        assert!(
            store
                .rename_project(w.project_id, 1, None)
                .await
                .unwrap_err()
                .to_string()
                .contains("owned project refusal")
        );
        assert_eq!(store.workspace(w.id).await.unwrap().project_name, before);
        store
            .job(|db| {
                db.execute_batch("DROP TRIGGER refuse_project;")?;
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(
            store
                .rename_project(w.project_id, 1, None)
                .await
                .unwrap()
                .revision,
            2
        );
        assert!(store.rename_project(Uuid::new_v4(), 0, None).await.is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn invalid_and_corrupt_project_names_are_explicit_errors() {
        let (root, store, w) = fixture().await;
        for invalid in [
            "".into(),
            " spaced".into(),
            "a\nb".into(),
            "a\u{7f}".into(),
            "a\u{2028}b".into(),
            "界".repeat(86),
        ] {
            assert!(
                store
                    .rename_project(w.project_id, 0, Some(invalid))
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("project name must")
            );
        }
        store
            .rename_project(w.project_id, 0, Some("x".repeat(256)))
            .await
            .unwrap();
        store
            .job(|db| {
                db.execute("UPDATE project_names SET value=?1", ["x".repeat(257)])?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(store.workspace(w.id).await.is_err());
        assert!(store.workspaces(None).await.is_err());
        assert!(store.rename_project(w.project_id, 1, None).await.is_err());
        store
            .job(|db| {
                db.execute(
                    "UPDATE project_names SET revision=?1,value=NULL",
                    [i64::MAX],
                )?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(
            store
                .rename_project(w.project_id, i64::MAX as u64, None)
                .await
                .unwrap_err()
                .to_string()
                .contains("revision exhausted")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn folder_git_name_and_sibling_updates_keep_independent_revisions() {
        let (root, store, w) = fixture().await;
        let folder = root.join(".git");
        std::fs::create_dir(&folder).unwrap();
        let hidden = store
            .register_workspace(endpoint("Offline"), folder, Kind::Folder)
            .await
            .unwrap();
        assert_eq!(hidden.project_display_name(), ".git");
        store
            .rename_project(w.project_id, 0, Some("Project".into()))
            .await
            .unwrap();
        let fresh = store
            .rename_workspace(w.id, 0, Some("Workspace".into()))
            .await
            .unwrap();
        let mut cached = w.clone();
        cached.update_names_from(&fresh);
        cached.update_names_from(&hidden); // another project cannot change either label
        cached.update_names_from(&w); // old observations cannot clear either label
        assert_eq!(cached.name, fresh.name);
        assert_eq!(cached.project_name, fresh.project_name);
        assert_eq!(cached.display_name(), "Workspace");
        assert_eq!(cached.project_display_name(), "Project");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn schema_18_upgrade_preserves_all_existing_rows() {
        let (root, store, w) = fixture().await;
        store
            .rename_workspace(w.id, 0, Some("Workspace only".into()))
            .await
            .unwrap();
        store.set_workspace_pinned(w.id, true).await.unwrap();
        store.job(|db|{db.execute_batch("INSERT INTO drafts VALUES('kept',7,'草稿 exact  '); INSERT INTO pending_alert_cursors VALUES('socket','host',12); DROP TABLE project_names; PRAGMA user_version=18;")?;Ok(())}).await.unwrap();
        type TableRows = Vec<(String, Vec<Vec<rusqlite::types::Value>>)>;
        fn rows(db: &rusqlite::Connection) -> Result<TableRows, crate::storage::StorageError> {
            let tables=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name!='project_names' ORDER BY name")?.query_map([],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
            tables
                .into_iter()
                .map(|table| {
                    let mut statement =
                        db.prepare(&format!("SELECT * FROM \"{table}\" ORDER BY rowid"))?;
                    let count = statement.column_count();
                    let rows = statement
                        .query_map([], |r| {
                            (0..count).map(|i| r.get(i)).collect::<Result<Vec<_>, _>>()
                        })?
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok((table, rows))
                })
                .collect()
        }
        let path = root.join("state/creations.sqlite3");
        let before = rows(&rusqlite::Connection::open(&path).unwrap()).unwrap();
        let reopened = Store::open(root.join("state")).await.unwrap();
        assert_eq!(
            reopened.workspace(w.id).await.unwrap().project_name,
            Saved::default()
        );
        assert_eq!(
            rows(&rusqlite::Connection::open(&path).unwrap()).unwrap(),
            before
        );
        reopened
            .job(|db| {
                assert_eq!(
                    db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))?,
                    19
                );
                db.execute_batch("DROP TABLE project_names;")?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(Store::open(root.join("state")).await.is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
