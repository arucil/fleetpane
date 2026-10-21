//! Device-local appearance preferences. No Host identity, configuration or RPC.
use crate::{
    client::Error,
    storage::{StorageError, Store},
};
use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub theme: Theme,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Saved {
    pub revision: u64,
    pub value: Preferences,
}

fn read(database: &Connection) -> Result<Saved, StorageError> {
    let mut query = database.prepare(
        "SELECT singleton,revision,CASE WHEN length(CAST(data AS BLOB))<=512 THEN data ELSE NULL END FROM appearance LIMIT 2",
    )?;
    let rows: Vec<(i64, i64, Option<String>)> = query
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<Result<_, _>>()?;
    match rows.as_slice() {
        [] => Ok(Saved::default()),
        [(1, revision, data)] if *revision > 0 => {
            let data = data.as_ref().ok_or("appearance record exceeds 512 bytes")?;
            Ok(Saved {
                revision: u64::try_from(*revision)?,
                value: serde_json::from_str(data)?,
            })
        }
        _ => Err("invalid appearance record identity or revision".into()),
    }
}

impl Store {
    pub async fn appearance(&self) -> Result<Saved, Error> {
        self.job(|database| read(database)).await
    }

    /// Compare-and-set after a successful read; never overwrite another window.
    pub async fn save_appearance(
        &self,
        expected_revision: u64,
        value: Preferences,
    ) -> Result<Saved, Error> {
        self.job(move |database| {
            let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current = read(&transaction)?;
            if current.revision != expected_revision {
                return Err("appearance preferences changed in another window; reload before saving".into());
            }
            let revision = expected_revision.checked_add(1)
                .and_then(|revision| i64::try_from(revision).ok())
                .ok_or("appearance revision exhausted")?;
            transaction.execute(
                "INSERT INTO appearance(singleton,revision,data) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET revision=excluded.revision,data=excluded.data",
                params![revision, serde_json::to_string(&value)?],
            )?;
            transaction.commit()?;
            Ok(Saved { revision: u64::try_from(revision)?, value })
        }).await
    }
}

/// A device preference path, independent of any selected Host or workspace.
/// Environment values are input boundaries: empty/relative values are errors.
pub fn default_directory() -> Result<std::path::PathBuf, String> {
    fn absolute(value: std::ffi::OsString, name: &str) -> Result<std::path::PathBuf, String> {
        let path = std::path::PathBuf::from(value);
        if !path.is_absolute() {
            return Err(format!("{name} must be an absolute nonempty path"));
        }
        Ok(path)
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(path) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(absolute(path, "XDG_CONFIG_HOME")?.join("fleetpane/preferences"));
    }
    let home = absolute(
        std::env::var_os("HOME").ok_or("HOME is not set; use --preferences-dir")?,
        "HOME",
    )?;
    #[cfg(target_os = "macos")]
    let directory = home.join("Library/Application Support/Fleetpane/preferences");
    #[cfg(not(target_os = "macos"))]
    let directory = home.join(".config/fleetpane/preferences");
    Ok(directory)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fixture() -> (std::path::PathBuf, Store) {
        let root = std::env::temp_dir().join(format!("fp-appearance-{}", uuid::Uuid::new_v4()));
        let store = Store::open(root.clone()).await.unwrap();
        (root, store)
    }

    #[tokio::test]
    async fn first_launch_creates_missing_appearance_ancestors_privately() {
        use std::os::unix::fs::PermissionsExt;
        let root =
            std::env::temp_dir().join(format!("fp-first-appearance-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let directory = root.join("config/fleetpane/preferences");
        let store = Store::open(directory.clone()).await.unwrap();
        assert_eq!(store.appearance().await.unwrap(), Saved::default());
        for path in [
            root.join("config"),
            root.join("config/fleetpane"),
            directory,
        ] {
            assert_eq!(
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn first_launch_preserves_existing_parents_and_refuses_invalid_leaves() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root =
            std::env::temp_dir().join(format!("fp-first-appearance-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        let store = Store::open(root.join("fleetpane/preferences"))
            .await
            .unwrap();
        assert_eq!(store.appearance().await.unwrap(), Saved::default());
        assert_eq!(
            std::fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o755
        );
        let public = root.join("public-leaf");
        std::fs::create_dir(&public).unwrap();
        std::fs::set_permissions(&public, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(Store::open(public).await.is_err());
        let link = root.join("link");
        symlink(root.join("fleetpane/preferences"), &link).unwrap();
        assert!(Store::open(link).await.is_err());
        let file = root.join("file");
        std::fs::write(&file, b"keep").unwrap();
        assert!(Store::open(file.join("preferences")).await.is_err());
        assert_eq!(std::fs::read(file).unwrap(), b"keep");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn persistence_conflicts_and_failed_commit_keep_the_last_saved_value() {
        let (root, store) = fixture().await;
        assert_eq!(store.appearance().await.unwrap(), Saved::default());
        let saved = store
            .save_appearance(0, Preferences { theme: Theme::Dark })
            .await
            .unwrap();
        assert_eq!(saved.revision, 1);
        let other = Store::open(root.clone()).await.unwrap();
        assert_eq!(other.appearance().await.unwrap(), saved);
        let (a, b) = tokio::join!(
            store.save_appearance(
                1,
                Preferences {
                    theme: Theme::Light
                }
            ),
            other.save_appearance(
                1,
                Preferences {
                    theme: Theme::System
                }
            )
        );
        assert_ne!(a.is_ok(), b.is_ok());
        let current = store.appearance().await.unwrap();
        assert_eq!(current.revision, 2);
        assert!(
            store
                .save_appearance(1, Preferences::default())
                .await
                .unwrap_err()
                .to_string()
                .contains("another window")
        );
        store.job(|db| { db.execute_batch("CREATE TRIGGER reject_appearance BEFORE UPDATE ON appearance BEGIN SELECT RAISE(ABORT,'injected appearance storage failure'); END;")?; Ok(()) }).await.unwrap();
        assert!(
            store
                .save_appearance(2, Preferences::default())
                .await
                .is_err()
        );
        assert_eq!(other.appearance().await.unwrap(), current);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn migration_preserves_drafts_and_current_schema_damage_is_not_defaulted() {
        use crate::drafts::Key;
        let (root, store) = fixture().await;
        let key = Key {
            socket: root.join("host.sock"),
            host_id: uuid::Uuid::new_v4(),
            thread_id: "thread".into(),
            cwd: "/synthetic/project".into(),
        };
        let draft = store.draft(key.clone()).await.unwrap();
        let draft = store
            .edit_draft(&draft, "保留草稿\nunchanged".into())
            .await
            .unwrap();
        store
            .job(|db| {
                db.execute_batch(
                    "DROP TABLE attention_reviews; DROP TABLE host_selection; DROP TABLE paseo_batch_results; DROP TABLE paseo_batches; DROP TABLE paseo_continuations; DROP TABLE paseo_sessions; DROP TABLE paseo_workspaces; DROP TABLE recent_directories; DROP TABLE appearance; PRAGMA user_version=5;",
                )?;
                Ok(())
            })
            .await
            .unwrap();
        let upgraded = Store::open(root.clone()).await.unwrap();
        assert_eq!(upgraded.draft(key).await.unwrap().text, draft.text);
        assert_eq!(upgraded.appearance().await.unwrap(), Saved::default());
        upgraded
            .job(|db| {
                db.execute_batch("DROP TABLE appearance;")?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(Store::open(root.clone()).await.is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn invalid_oversize_unknown_and_exhausted_records_fail_explicitly() {
        let (root, store) = fixture().await;
        for data in [
            "not JSON".to_string(),
            "{\"theme\":\"future\"}".into(),
            "{\"theme\":\"dark\",\"ignored\":true}".into(),
            "x".repeat(513),
        ] {
            store.job(move |db| { db.execute("INSERT INTO appearance VALUES(1,1,?1) ON CONFLICT(singleton) DO UPDATE SET data=excluded.data",[data])?; Ok(()) }).await.unwrap();
            assert!(store.appearance().await.is_err());
            assert!(
                store
                    .save_appearance(1, Preferences::default())
                    .await
                    .is_err()
            );
        }
        store
            .job(|db| {
                db.execute(
                    "UPDATE appearance SET revision=?1,data=?2",
                    params![i64::MAX, "{\"theme\":\"dark\"}"],
                )?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(
            store
                .save_appearance(u64::try_from(i64::MAX).unwrap(), Preferences::default())
                .await
                .is_err()
        );
        assert_eq!(store.appearance().await.unwrap().value.theme, Theme::Dark);
        std::fs::remove_dir_all(root).unwrap();
    }
}
