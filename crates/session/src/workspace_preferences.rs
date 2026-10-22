//! One bounded, consistent observation of device-local labels and pin order.
use crate::{
    client::Error,
    storage::Store,
    workspace_pins,
    workspaces::{self, Workspace},
};
use std::collections::HashSet;
use uuid::Uuid;

// 32 open tabs, a 32-entry catalog page and its separately inspected workspace.
pub const MAX_OBSERVED_WORKSPACES: usize = 65;

#[derive(serde::Serialize)]
pub struct Snapshot {
    pub workspaces: Vec<Workspace>,
    pub pins: Vec<Workspace>,
}

impl Store {
    /// Only saved identities are read. Missing paths and offline Hosts do not
    /// affect this read, and a corrupt row refuses the entire observation.
    pub async fn workspace_preferences(&self, ids: Vec<Uuid>) -> Result<Snapshot, Error> {
        self.job(move |database| {
            if ids.len() > MAX_OBSERVED_WORKSPACES
                || ids.iter().collect::<HashSet<_>>().len() != ids.len()
            {
                return Err(
                    "workspace preference observation needs at most 65 distinct saved UUIDs".into(),
                );
            }
            let transaction = database.transaction()?;
            let workspaces = ids
                .into_iter()
                .map(|id| workspaces::read(&transaction, id))
                .collect::<Result<Vec<_>, _>>()?;
            let pins = workspace_pins::read(&transaction)?;
            transaction.commit()?;
            Ok(Snapshot { workspaces, pins })
        })
        .await
    }
}

#[cfg(test)]
mod tests;
