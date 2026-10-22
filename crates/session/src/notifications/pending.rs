//! Durable at-most-once reminders for exact Host request occurrences.
use super::*;
use crate::attention::{Item, Report};
use rusqlite::OptionalExtension;

const MAX_CURSORS: u32 = 256;

#[derive(Clone, Debug)]
pub struct Observation {
    pub socket: PathBuf,
    pub host_id: Uuid,
    pub sequence: u64,
    pub items: Vec<Item>,
}

impl Observation {
    pub fn from_report(report: &Report) -> Vec<Self> {
        report
            .hosts
            .iter()
            .filter_map(|host| {
                if host.error.is_some() {
                    return None;
                }
                Some(Self {
                    socket: host.endpoint.socket.clone(),
                    host_id: host.host_id?,
                    sequence: host.pending_sequence?,
                    items: host.items.clone(),
                })
            })
            .collect()
    }
}

#[derive(Debug, Serialize)]
pub struct Batch {
    pub policy: Saved,
    pub items: Vec<Item>,
}

fn validate(observations: &[Observation]) -> Result<(), Error> {
    if observations.len() > crate::hosts::MAX_HOSTS {
        return Err(Error::Input(
            "Pending reminders: excessive Host batch".into(),
        ));
    }
    let mut hosts = BTreeSet::new();
    for observation in observations {
        crate::hosts::Configuration::single(observation.socket.clone())
            .map_err(|error| Error::Input(error.to_string()))?;
        if observation.host_id.is_nil()
            || !hosts.insert((&observation.socket, observation.host_id))
            || observation.items.len() > 64
        {
            return Err(Error::Input(
                "Pending reminders: invalid or duplicate Host feed".into(),
            ));
        }
        let mut sequences = BTreeSet::new();
        let mut ids = Vec::new();
        for item in &observation.items {
            let sequence = item.request_sequence.ok_or_else(|| {
                Error::Input("Pending reminders: missing occurrence sequence".into())
            })?;
            if sequence == 0
                || sequence > observation.sequence
                || !sequences.insert(sequence)
                || ids.contains(&&item.id)
                || item.observed_unix_ms.is_none()
                || item.endpoint.socket != observation.socket
                || item.host_id != observation.host_id
                || item.request.get("id") != Some(&item.id)
                || item
                    .request
                    .get("method")
                    .and_then(serde_json::Value::as_str)
                    != Some(item.method.as_str())
                || item
                    .request
                    .pointer("/params/threadId")
                    .and_then(serde_json::Value::as_str)
                    .filter(|s| !s.is_empty())
                    != item.thread_id.as_deref()
                || serde_json::to_vec(&item.request)
                    .map_err(|error| Error::Input(error.to_string()))?
                    .len()
                    > fleetpane_runtime::MAX_PROVIDER_FRAME
            {
                return Err(Error::Input(
                    "Pending reminders: invalid request occurrence".into(),
                ));
            }
            ids.push(&item.id);
            let expected_kind = match item.method.as_str() {
                "session/request_permission"
                | "item/commandExecution/requestApproval"
                | "item/fileChange/requestApproval" => attention::Kind::Approval,
                crate::questions::METHOD => attention::Kind::Question,
                _ => attention::Kind::Other,
            };
            if item.kind != expected_kind {
                return Err(Error::Input(
                    "Pending reminders: request kind differs from method".into(),
                ));
            }
        }
    }
    Ok(())
}

impl Store {
    /// Baseline first contact silently, then consume all new occurrences,
    /// including muted or unroutable ones. Never answer or acquire control.
    pub async fn claim_pending_alerts(
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
                    "SELECT sequence FROM pending_alert_cursors WHERE socket=?1 AND host_id=?2",
                    params![socket, host], |row| row.get(0),
                ).optional()?;
                if previous.is_some_and(|n| n < 0) {
                    return Err("Pending reminders: invalid saved cursor".into());
                }
                let latest = i64::try_from(observation.sequence)?;
                if previous.is_none() {
                    let count: u32 = tx.query_row("SELECT count(*) FROM pending_alert_cursors", [], |row| row.get(0))?;
                    if count >= MAX_CURSORS {
                        return Err("Pending reminders: saved Host cursor limit reached (256); no alerts claimed".into());
                    }
                }
                if let Some(previous) = previous {
                    let previous = u64::try_from(previous)?;
                    items.extend(observation.items.into_iter().filter(|item| {
                        item.request_sequence.is_some_and(|n| n > previous)
                            && item.thread_id.is_some() && policy.value.pending_allowed(item)
                    }));
                }
                if previous.is_some_and(|previous| latest <= previous) {
                    continue;
                }
                tx.execute(
                    "INSERT INTO pending_alert_cursors VALUES(?1,?2,?3) ON CONFLICT(socket,host_id) DO UPDATE SET sequence=max(sequence,excluded.sequence)",
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
