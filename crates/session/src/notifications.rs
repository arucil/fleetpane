//! Device-local alert policy. Muting never resolves requests or reviews results.
use crate::{
    attention,
    client::Error,
    storage::{StorageError, Store},
};
use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::PathBuf};
use uuid::Uuid;

pub mod delivery;
pub mod pending;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", try_from = "String")]
pub enum Kind {
    Approval,
    Question,
    Other,
    Completed,
    Failed,
    Interrupted,
}

impl TryFrom<String> for Kind {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "approval" => Ok(Self::Approval),
            "question" => Ok(Self::Question),
            "other" => Ok(Self::Other),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "interrupted" => Ok(Self::Interrupted),
            _ => Err(format!("unknown notification event type: {value}")),
        }
    }
}

impl Kind {
    pub const ALL: [Self; 6] = [
        Self::Approval,
        Self::Question,
        Self::Other,
        Self::Completed,
        Self::Failed,
        Self::Interrupted,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Approval => "Approvals",
            Self::Question => "Questions",
            Self::Other => "Other requests",
            Self::Completed => "Completed turns",
            Self::Failed => "Failed turns",
            Self::Interrupted => "Interrupted turns",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub socket: PathBuf,
    pub host_id: Uuid,
    pub thread_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub socket: PathBuf,
    pub directory: PathBuf,
}

impl Workspace {
    fn validate(&self) -> Result<(), Error> {
        crate::hosts::Configuration::single(self.socket.clone())
            .map_err(|e| Error::Input(e.to_string()))?;
        if !self.directory.is_absolute()
            || self.directory.to_str().is_none()
            || self.directory.as_os_str().len() > 4096
            || self.directory.as_os_str().as_encoded_bytes().contains(&0)
            || !self.directory.components().all(|c| {
                matches!(
                    c,
                    std::path::Component::RootDir | std::path::Component::Normal(_)
                )
            })
        {
            return Err(Error::Input(
                "Notification workspace requires an exact absolute directory within 4 KiB".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub muted: bool,
    pub muted_kinds: Vec<Kind>,
    pub muted_hosts: Vec<PathBuf>,
    pub muted_sessions: Vec<Session>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub muted_workspaces: Vec<Workspace>,
}

impl Preferences {
    pub fn validate(&self) -> Result<(), Error> {
        fn unique<T: Ord>(values: &[T], limit: usize) -> bool {
            values.len() <= limit && values.iter().collect::<BTreeSet<_>>().len() == values.len()
        }
        if !unique(&self.muted_kinds, 6)
            || !unique(&self.muted_hosts, 64)
            || !unique(&self.muted_sessions, 512)
            || !unique(&self.muted_workspaces, 64)
        {
            return Err(Error::Input(
                "Notification policy has duplicate or excessive scopes".into(),
            ));
        }
        for workspace in &self.muted_workspaces {
            workspace.validate()?;
        }
        for socket in self
            .muted_hosts
            .iter()
            .chain(self.muted_sessions.iter().map(|s| &s.socket))
        {
            crate::hosts::Configuration::single(socket.clone())
                .map_err(|e| Error::Input(e.to_string()))?;
        }
        if self
            .muted_sessions
            .iter()
            .any(|s| s.host_id.is_nil() || s.thread_id.is_empty() || s.thread_id.len() > 512)
        {
            return Err(Error::Input(
                "Notification session identity is invalid".into(),
            ));
        }
        Ok(())
    }

    pub fn allows(
        &self,
        kind: Kind,
        socket: &std::path::Path,
        host_id: Uuid,
        thread_id: Option<&str>,
    ) -> bool {
        !self.muted
            && !self.muted_kinds.contains(&kind)
            && !self.muted_hosts.iter().any(|p| p == socket)
            && !self.muted_sessions.iter().any(|s| {
                s.socket == socket
                    && s.host_id == host_id
                    && Some(s.thread_id.as_str()) == thread_id
            })
    }

    pub fn pending_allowed(&self, item: &attention::Item) -> bool {
        let kind = match item.kind {
            attention::Kind::Approval => Kind::Approval,
            attention::Kind::Question => Kind::Question,
            attention::Kind::Other => Kind::Other,
        };
        self.workspace_allowed(&item.endpoint.socket, item.cwd.as_deref())
            && self.allows(
                kind,
                &item.endpoint.socket,
                item.host_id,
                item.thread_id.as_deref(),
            )
    }

    pub fn result_allowed(&self, item: &attention::review::Item) -> bool {
        let kind = match item.record.kind {
            fleetpane_runtime::attention::Kind::Completed => Kind::Completed,
            fleetpane_runtime::attention::Kind::Failed => Kind::Failed,
            fleetpane_runtime::attention::Kind::Interrupted => Kind::Interrupted,
        };
        !item.reviewed
            && self.workspace_allowed(&item.endpoint.socket, item.cwd.as_deref())
            && self.allows(
                kind,
                &item.endpoint.socket,
                item.host_id,
                Some(&item.record.thread_id),
            )
    }

    fn workspace_allowed(&self, socket: &std::path::Path, cwd: Option<&str>) -> bool {
        !self.muted_workspaces.iter().any(|w| {
            w.socket == socket && cwd.is_some_and(|cwd| w.directory == std::path::Path::new(cwd))
        })
    }

    pub fn workspace_unknown(&self, socket: &std::path::Path, cwd: Option<&str>) -> bool {
        cwd.is_none() && self.muted_workspaces.iter().any(|w| w.socket == socket)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Saved {
    pub revision: u64,
    pub value: Preferences,
}

fn read(db: &Connection) -> Result<Saved, StorageError> {
    let mut query = db.prepare("SELECT singleton,revision,CASE WHEN length(CAST(data AS BLOB))<=524288 THEN data ELSE NULL END FROM notification_preferences LIMIT 2")?;
    let rows: Vec<(i64, i64, Option<String>)> = query
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    match rows.as_slice() {
        [] => Ok(Saved::default()),
        [(1, revision, Some(data))] if *revision > 0 => {
            let value: Preferences = serde_json::from_str(data)?;
            value.validate()?;
            Ok(Saved {
                revision: u64::try_from(*revision)?,
                value,
            })
        }
        _ => Err("invalid notification policy identity, revision or size".into()),
    }
}

impl Store {
    pub async fn notifications(&self) -> Result<Saved, Error> {
        self.job(|db| read(db)).await
    }
    pub async fn save_notifications(
        &self,
        expected_revision: u64,
        value: Preferences,
    ) -> Result<Saved, Error> {
        value.validate()?;
        let data = serde_json::to_string(&value).map_err(|e| Error::Input(e.to_string()))?;
        if data.len() > 524288 {
            return Err(Error::Input("Notification policy exceeds 512 KiB".into()));
        }
        self.job(move |db| {
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if read(&tx)?.revision != expected_revision { return Err("notification preferences changed in another window; reload before saving".into()); }
            let revision = expected_revision.checked_add(1).and_then(|n| i64::try_from(n).ok()).ok_or("notification revision exhausted")?;
            tx.execute("INSERT INTO notification_preferences VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET revision=excluded.revision,data=excluded.data",params![revision,data])?;
            tx.commit()?;
            Ok(Saved {revision: u64::try_from(revision)?, value})
        }).await
    }
}

#[cfg(test)]
mod tests;
