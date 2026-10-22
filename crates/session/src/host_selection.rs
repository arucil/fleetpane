//! Configuration-local preferred endpoint. Never a controller or session identity.
use crate::{
    client::Error,
    hosts::Configuration,
    storage::{StorageError, Store},
};
use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Saved {
    pub revision: u64,
    pub socket: Option<PathBuf>,
}

fn validate(socket: &PathBuf) -> Result<(), StorageError> {
    Configuration::single(socket.clone())?;
    if serde_json::to_vec(socket)?.len() > 4096 {
        return Err("preferred Host socket exceeds 4096 encoded bytes".into());
    }
    Ok(())
}

fn read(database: &Connection) -> Result<Saved, StorageError> {
    let mut statement = database.prepare("SELECT singleton,revision,CASE WHEN length(CAST(socket AS BLOB))<=4096 THEN socket ELSE NULL END FROM host_selection LIMIT 2")?;
    let rows: Vec<(i64, i64, Option<String>)> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<Result<_, _>>()?;
    match rows.as_slice() {
        [] => Ok(Saved::default()),
        [(1, revision, Some(data))] if *revision > 0 => {
            let socket = serde_json::from_str(data)?;
            validate(&socket)?;
            Ok(Saved {
                revision: u64::try_from(*revision)?,
                socket: Some(socket),
            })
        }
        _ => Err("invalid preferred Host identity, revision or socket size".into()),
    }
}

impl Saved {
    /// Names and configuration order are mutable labels. No endpoint substitution.
    pub fn configured_index(&self, configuration: &Configuration) -> Option<usize> {
        self.socket.as_ref().and_then(|socket| {
            configuration
                .hosts
                .iter()
                .position(|host| host.socket == *socket)
        })
    }
}

impl Store {
    pub async fn host_selection(&self) -> Result<Saved, Error> {
        self.job(|database| read(database)).await
    }

    pub async fn save_host_selection(
        &self,
        expected_revision: u64,
        socket: PathBuf,
    ) -> Result<Saved, Error> {
        self.job(move |database| {
            validate(&socket)?;
            let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current = read(&transaction)?;
            if current.revision != expected_revision {
                return Err("preferred Host changed in another window; reload before saving".into());
            }
            let revision = expected_revision.checked_add(1).and_then(|v| i64::try_from(v).ok()).ok_or("preferred Host revision exhausted")?;
            transaction.execute("INSERT INTO host_selection(singleton,revision,socket) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET revision=excluded.revision,socket=excluded.socket", params![revision,serde_json::to_string(&socket)?])?;
            transaction.commit()?;
            Ok(Saved {revision: u64::try_from(revision)?, socket: Some(socket)})
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::{Preferences, Theme};

    #[tokio::test]
    async fn exact_endpoint_survives_reorder_and_rename_without_substitution() {
        let root = std::env::temp_dir().join(format!("fp-selected-{}", uuid::Uuid::new_v4()));
        let store = Store::open(root.clone()).await.unwrap();
        assert_eq!(store.host_selection().await.unwrap(), Saved::default());
        let socket = root.join("开发.sock");
        let saved = store.save_host_selection(0, socket.clone()).await.unwrap();
        let reopened = Store::open(root.clone()).await.unwrap();
        assert_eq!(reopened.host_selection().await.unwrap(), saved);
        let mut config = Configuration::single(root.join("other.sock")).unwrap();
        assert_eq!(saved.configured_index(&config), None);
        config.hosts.push(crate::hosts::Endpoint {
            name: "Renamed".into(),
            socket: socket.clone(),
        });
        assert_eq!(saved.configured_index(&config), Some(1));
        config.hosts.reverse();
        assert_eq!(saved.configured_index(&config), Some(0));
        assert_eq!(
            reopened.host_selection().await.unwrap().socket,
            Some(socket)
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn concurrent_windows_and_failed_commit_preserve_last_selection() {
        let root = std::env::temp_dir().join(format!("fp-selected-{}", uuid::Uuid::new_v4()));
        let store = Store::open(root.clone()).await.unwrap();
        let other = Store::open(root.clone()).await.unwrap();
        let (a, b) = tokio::join!(
            store.save_host_selection(0, root.join("a.sock")),
            other.save_host_selection(0, root.join("b.sock"))
        );
        assert_ne!(a.is_ok(), b.is_ok());
        let saved = store.host_selection().await.unwrap();
        store.job(|db| {db.execute_batch("CREATE TRIGGER reject_selection BEFORE UPDATE ON host_selection BEGIN SELECT RAISE(ABORT,'injected selection storage failure'); END;")?;Ok(())}).await.unwrap();
        assert!(
            other
                .save_host_selection(1, root.join("c.sock"))
                .await
                .is_err()
        );
        assert_eq!(store.host_selection().await.unwrap(), saved);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn invalid_records_and_schema_damage_are_explicit() {
        let root = std::env::temp_dir().join(format!("fp-selected-{}", uuid::Uuid::new_v4()));
        let store = Store::open(root.clone()).await.unwrap();
        for socket in [
            PathBuf::from("relative.sock"),
            PathBuf::from("/tmp/../a.sock"),
            PathBuf::from(format!("/{}", "x".repeat(4096))),
        ] {
            assert!(store.save_host_selection(0, socket).await.is_err());
        }
        for data in [
            "not JSON".into(),
            "\"relative.sock\"".into(),
            "x".repeat(4097),
        ] {
            store.job(move |db| {db.execute("INSERT INTO host_selection VALUES(1,1,?1) ON CONFLICT(singleton) DO UPDATE SET socket=excluded.socket",[data])?;Ok(())}).await.unwrap();
            assert!(store.host_selection().await.is_err());
            assert!(
                store
                    .save_host_selection(1, root.join("valid.sock"))
                    .await
                    .is_err()
            );
        }
        store
            .job(|db| {
                db.execute_batch("DROP TABLE host_selection;")?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(Store::open(root.clone()).await.is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn v10_upgrade_preserves_existing_preferences_and_drafts() {
        let root = std::env::temp_dir().join(format!("fp-selected-{}", uuid::Uuid::new_v4()));
        let store = Store::open(root.clone()).await.unwrap();
        let appearance = store
            .save_appearance(0, Preferences { theme: Theme::Dark })
            .await
            .unwrap();
        store.job(|db| {db.execute_batch("INSERT INTO drafts VALUES('saved',7,'exact legacy data'); DROP TABLE notification_preferences; DROP TABLE workspace_layout; DROP TABLE session_selection; DROP TABLE attention_reviews; DROP TABLE host_selection; PRAGMA user_version=10;")?;Ok(())}).await.unwrap();
        let reopened = Store::open(root.clone()).await.unwrap();
        assert_eq!(reopened.host_selection().await.unwrap(), Saved::default());
        assert_eq!(reopened.appearance().await.unwrap(), appearance);
        reopened
            .job(|db| {
                assert_eq!(
                    db.query_row("SELECT data FROM drafts WHERE key='saved'", [], |r| r
                        .get::<_, String>(0))?,
                    "exact legacy data"
                );
                Ok(())
            })
            .await
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
