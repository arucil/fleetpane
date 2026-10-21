//! Local review receipts never answer a request or modify a terminal turn result.
use super::*;
use crate::creations::Store;
use fleetpane_runtime::attention::{Kind, Record};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::Deserialize;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub endpoint: Endpoint,
    pub host_id: Uuid,
    pub record: Record,
    pub reviewed: bool,
}

impl Item {
    fn validate_input(&self) -> Result<(), Error> {
        Configuration::single(self.endpoint.socket.clone()).map_err(invalid)?;
        if self.host_id.is_nil()
            || self.record.sequence == 0
            || self.record.sequence > i64::MAX as u64
            || [&self.record.thread_id, &self.record.turn_id]
                .iter()
                .any(|id| id.is_empty() || id.len() > 512)
            || self
                .record
                .stop_reason
                .as_ref()
                .is_some_and(|reason| reason.is_empty() || reason.len() > 128)
        {
            return Err(invalid("invalid completion review identity or bounds"));
        }
        Ok(())
    }
    pub fn label(&self) -> String {
        let status = match self.record.kind {
            Kind::Completed => "Turn completed",
            Kind::Failed => "Turn failed",
            Kind::Interrupted => "Turn interrupted",
        };
        match &self.record.stop_reason {
            Some(reason) => format!("{status} · {reason}"),
            None => status.into(),
        }
    }

    pub fn validate(&self, snapshot: &Snapshot) -> Result<(), Error> {
        if snapshot.host_id != self.host_id {
            return Err(invalid("completion belongs to a replaced Host; refresh"));
        }
        if let Some(fault) = &snapshot.fault {
            return Err(invalid(format!("Host is unavailable: {fault}")));
        }
        if !snapshot
            .attention
            .as_ref()
            .is_some_and(|feed| feed.records.contains(&self.record))
        {
            return Err(invalid(
                "completion is no longer in the Host's retained feed; refresh",
            ));
        }
        Ok(())
    }

    pub async fn confirm(&self) -> Result<(), Error> {
        self.validate(&client::snapshot(&self.endpoint.socket).await?)
    }
}

impl Store {
    /// Reads only receipts for these bounded observations, never the whole ledger.
    pub async fn completion_reviews(&self, mut items: Vec<Item>) -> Result<Vec<Item>, Error> {
        if items.len() > 512 {
            return Err(invalid("completion review batch exceeds 512 records"));
        }
        for item in &items {
            item.validate_input()?;
        }
        self.job(move |database| {
            let mut query = database.prepare("SELECT record FROM attention_reviews WHERE socket=?1 AND host_id=?2 AND sequence=?3")?;
            for item in &mut items {
                let saved: Option<String> = query.query_row(params![serde_json::to_string(&item.endpoint.socket)?, item.host_id.to_string(), i64::try_from(item.record.sequence)?], |row| row.get(0)).optional()?;
                item.reviewed = match saved {
                    Some(saved) => {
                        if serde_json::from_str::<Record>(&saved)? != item.record {
                            return Err("completion review identity changed".into());
                        }
                        true
                    }
                    None => false,
                };
            }
            Ok(items)
        }).await
    }

    /// Marks exactly the displayed records. A concurrent new record stays unread.
    pub async fn review_completions(&self, items: Vec<Item>) -> Result<(), Error> {
        if items.is_empty() || items.len() > 512 {
            return Err(invalid("select between 1 and 512 completion records"));
        }
        for item in &items {
            item.validate_input()?;
        }
        self.job(move |database| {
            let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
            for item in items {
                if item.record.sequence == 0 {
                    return Err("completion sequence must be positive".into());
                }
                let key = (serde_json::to_string(&item.endpoint.socket)?, item.host_id.to_string(), i64::try_from(item.record.sequence)?);
                let data = serde_json::to_string(&item.record)?;
                let existing: Option<String> = transaction.query_row("SELECT record FROM attention_reviews WHERE socket=?1 AND host_id=?2 AND sequence=?3", params![key.0, key.1, key.2], |row| row.get(0)).optional()?;
                if existing.as_ref().is_some_and(|old| old != &data) {
                    return Err("completion review identity changed".into());
                }
                if existing.is_none() {
                    let count: i64 = transaction.query_row("SELECT count(*) FROM attention_reviews",[],|row|row.get(0))?;
                    if count >= 32768 { return Err("completion review receipt capacity reached (32768); existing reviews are retained".into()); }
                    transaction.execute("INSERT INTO attention_reviews(socket,host_id,sequence,record) VALUES(?1,?2,?3,?4)", params![key.0,key.1,key.2,data])?;
                }
            }
            transaction.commit()?;
            Ok(())
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item() -> Item {
        Item {
            endpoint: Endpoint {
                name: "Synthetic".into(),
                socket: "/tmp/completion.sock".into(),
            },
            host_id: Uuid::new_v4(),
            reviewed: false,
            record: Record {
                sequence: 1,
                observed_unix_ms: 42,
                thread_id: "same".into(),
                turn_id: "same".into(),
                kind: Kind::Failed,
                stop_reason: None,
            },
        }
    }

    #[test]
    fn source_confirmation_requires_the_original_retained_record_and_available_host() {
        let item = item();
        let mut snapshot = fleetpane_runtime::State::new(2, serde_json::json!({})).snapshot();
        snapshot.host_id = item.host_id;
        snapshot
            .attention
            .as_mut()
            .unwrap()
            .record(item.record.clone());
        assert!(item.validate(&snapshot).is_ok());
        snapshot.fault = Some("provider lost".into());
        assert!(item.validate(&snapshot).is_err());
        snapshot.fault = None;
        snapshot.attention = None;
        assert!(item.validate(&snapshot).is_err());
        assert!(
            item.validate(&fleetpane_runtime::State::new(2, serde_json::json!({})).snapshot())
                .is_err()
        );
    }

    #[tokio::test]
    async fn schema_eleven_upgrade_keeps_existing_state_and_missing_current_table_is_an_error() {
        let root = std::env::temp_dir().join(format!("fp-review-upgrade-{}", Uuid::new_v4()));
        let store = Store::open(root.clone()).await.unwrap();
        store.job(|db|{db.execute_batch("INSERT INTO drafts VALUES('retained',7,'exact draft'); DROP TABLE session_selection; DROP TABLE attention_reviews; PRAGMA user_version=11;")?;Ok(())}).await.unwrap();
        let reopened = Store::open(root.clone()).await.unwrap();
        reopened.review_completions(vec![item()]).await.unwrap();
        reopened
            .job(|db| {
                assert_eq!(
                    db.query_row("SELECT data FROM drafts WHERE key='retained'", [], |row| {
                        row.get::<_, String>(0)
                    })?,
                    "exact draft"
                );
                assert_eq!(
                    db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?,
                    13
                );
                db.execute("DROP TABLE attention_reviews", [])?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(Store::open(root.clone()).await.is_err());
        drop(reopened);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn exact_review_survives_reopen_and_never_changes_failure_or_colliding_sources() {
        let root = std::env::temp_dir().join(format!("fp-reviews-{}", Uuid::new_v4()));
        let store = Store::open(root.clone()).await.unwrap();
        let original = item();
        let mut other = original.clone();
        other.host_id = Uuid::new_v4();
        let mut endpoint = original.clone();
        endpoint.endpoint.socket = "/tmp/other.sock".into();
        let mut later = original.clone();
        later.record.sequence = 2;
        later.record.turn_id = "later".into();
        store
            .review_completions(vec![original.clone()])
            .await
            .unwrap();
        drop(store);
        let store = Store::open(root.clone()).await.unwrap();
        let loaded = store
            .completion_reviews(vec![original.clone(), other, endpoint, later])
            .await
            .unwrap();
        assert_eq!(
            loaded.iter().map(|item| item.reviewed).collect::<Vec<_>>(),
            [true, false, false, false]
        );
        assert_eq!(loaded[0].record.kind, Kind::Failed);
        assert_eq!(loaded[0].label(), "Turn failed");
        let mut changed = original;
        changed.record.kind = Kind::Completed;
        assert!(
            store
                .review_completions(vec![loaded[3].clone(), changed])
                .await
                .is_err()
        );
        assert!(
            !store
                .completion_reviews(vec![loaded[3].clone()])
                .await
                .unwrap()[0]
                .reviewed,
            "batch failure rolls back every review"
        );
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
