//! Durable at-most-once claims for observed terminal results, not OS delivery receipts.
use super::*;
use crate::attention::{Report, review::Item};
use rusqlite::OptionalExtension;

const MAX_CURSORS: u32 = 256;

/// One complete retained feed from an available Host. Empty supported feeds
/// establish a baseline too; missing/failed observations never advance a cursor.
#[derive(Clone, Debug)]
pub struct Observation {
    pub socket: PathBuf,
    pub host_id: Uuid,
    pub records: Vec<Item>,
}

impl Observation {
    pub fn from_report(report: &Report) -> Vec<Self> {
        report
            .hosts
            .iter()
            .filter_map(|host| {
                if host.error.is_some() || host.completion_evicted.is_none() {
                    return None;
                }
                host.host_id.map(|host_id| Self {
                    socket: host.endpoint.socket.clone(),
                    host_id,
                    records: host.completions.clone(),
                })
            })
            .collect()
    }
}

#[derive(Debug, Serialize)]
pub struct Batch {
    /// Policy read atomically with the claims, including other-window changes.
    pub policy: Saved,
    pub items: Vec<Item>,
}

fn validate(observations: &[Observation]) -> Result<(), Error> {
    if observations.len() > crate::hosts::MAX_HOSTS {
        return Err(Error::Input(
            "Completion alerts: excessive Host batch".into(),
        ));
    }
    let mut hosts = BTreeSet::new();
    for observation in observations {
        crate::hosts::Configuration::single(observation.socket.clone())
            .map_err(|e| Error::Input(e.to_string()))?;
        if observation.host_id.is_nil()
            || !hosts.insert((&observation.socket, observation.host_id))
            || observation.records.len() > fleetpane_runtime::attention::CAPACITY
        {
            return Err(Error::Input(
                "Completion alerts: invalid or duplicate Host feed".into(),
            ));
        }
        let mut sequences = BTreeSet::new();
        for item in &observation.records {
            item.validate_input()?;
            if item.endpoint.socket != observation.socket
                || item.host_id != observation.host_id
                || !sequences.insert(item.record.sequence)
            {
                return Err(Error::Input(
                    "Completion alerts: record identity differs from its feed".into(),
                ));
            }
        }
    }
    Ok(())
}

impl Store {
    /// First sight of an incarnation baselines its feed without notifying old
    /// history. Subsequent reads consume every new record, even muted/reviewed
    /// ones. Claiming never marks a result reviewed or mutates a Host.
    ///
    /// Claim before presentation deliberately prefers a missed toast on crash
    /// over duplicate alerts after a restart. Exhaustion is explicit; cursors
    /// are never evicted in a way that replays old alerts.
    pub async fn claim_completion_alerts(
        &self,
        observations: Vec<Observation>,
    ) -> Result<Batch, Error> {
        validate(&observations)?;
        self.job(move |db| {
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let policy = read(&tx)?;
            let mut items = Vec::new();
            for observation in observations {
                let socket = serde_json::to_string(&observation.socket)?;
                let host = observation.host_id.to_string();
                let previous: Option<i64> = tx.query_row(
                    "SELECT sequence FROM completion_alert_cursors WHERE socket=?1 AND host_id=?2",
                    params![socket, host], |row| row.get(0),
                ).optional()?;
                if previous.is_some_and(|n| n < 0) {
                    return Err("Completion alerts: invalid saved cursor".into());
                }
                let latest = observation.records.iter().map(|item| item.record.sequence).max().unwrap_or(0);
                let latest = i64::try_from(latest)?;
                if previous.is_none() {
                    let count: u32 = tx.query_row("SELECT count(*) FROM completion_alert_cursors", [], |row| row.get(0))?;
                    if count >= MAX_CURSORS {
                        return Err("Completion alerts: saved Host cursor limit reached (256); no alerts claimed".into());
                    }
                }
                if let Some(previous) = previous {
                    let previous = u64::try_from(previous)?;
                    items.extend(observation.records.into_iter().filter(|item| {
                        item.record.sequence > previous && policy.value.result_allowed(item)
                    }));
                }
                if previous.is_some_and(|previous| latest <= previous) {
                    continue;
                }
                tx.execute(
                    "INSERT INTO completion_alert_cursors VALUES(?1,?2,?3) ON CONFLICT(socket,host_id) DO UPDATE SET sequence=max(sequence,excluded.sequence)",
                    params![socket, host, latest],
                )?;
            }
            tx.commit()?;
            Ok(Batch { policy, items })
        }).await
    }
}

#[cfg(test)]
mod tests;
