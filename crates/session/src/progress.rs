//! Presentation-independent interpretation of observed session activity.
//! Idle here describes observed turns, never background resource retirement.
use crate::Thread;
use fleetpane_runtime::{Snapshot, stopping::Stop};

#[derive(Debug, PartialEq, Eq)]
pub enum Progress<'a> {
    Working { turn_id: Option<&'a str> },
    Waiting { requests: usize },
    Stopping(&'a Stop),
    Synchronizing,
    Unknown,
    Archived,
    NotLoaded,
    Idle,
    Unavailable,
}

impl<'a> Progress<'a> {
    pub fn observed(thread: &Thread, snapshot: &'a Snapshot) -> Self {
        if snapshot.fault.is_some() {
            return Self::Unavailable;
        }
        if let Some(stop) = snapshot
            .activity
            .stopping
            .as_ref()
            .and_then(|stops| stops.iter().find(|stop| stop.thread_id == thread.id))
        {
            return Self::Stopping(stop);
        }
        let active = snapshot
            .activity
            .turns
            .iter()
            .find(|turn| turn.thread_id == thread.id);
        let requests = snapshot
            .pending_requests
            .iter()
            .filter(|request| {
                let params = &request["params"];
                params["threadId"].as_str() == Some(&thread.id)
                    && params.get("turnId").is_none_or(|id| {
                        active
                            .and_then(|turn| turn.turn_id.as_deref())
                            .is_none_or(|turn| id.as_str() == Some(turn))
                    })
            })
            .count();
        if requests > 0 {
            return Self::Waiting { requests };
        }
        if let Some(active) = active {
            return Self::Working {
                turn_id: active.turn_id.as_deref(),
            };
        }
        if thread.status.kind == "archived" {
            return Self::Archived;
        }
        if thread.active_turn().is_some() || thread.status.kind == "active" {
            // Snapshot and paged history have different observation boundaries.
            // An old active row must not spin forever or imply completion.
            return Self::Synchronizing;
        }
        if thread
            .turns
            .last()
            .is_some_and(|turn| turn.status == "unknown")
        {
            return Self::Unknown;
        }
        if thread.status.kind == "notLoaded" {
            return Self::NotLoaded;
        }
        Self::Idle
    }

    pub fn label(&self) -> String {
        match self {
            Self::Working { .. } => "Agent working…".into(),
            Self::Waiting { requests } => format!("Waiting for your response · {requests} pending"),
            Self::Stopping(stop) => match stop.phase {
                fleetpane_runtime::stopping::Phase::Rejected if stop.can_retry() => {
                    "Stop rejected · review and retry".into()
                }
                fleetpane_runtime::stopping::Phase::Rejected => {
                    "Stop rejected · review error".into()
                }
                fleetpane_runtime::stopping::Phase::Unknown => "Stop outcome unknown".into(),
                _ => "Stopping…".into(),
            },
            Self::Synchronizing => "Synchronizing turn status…".into(),
            Self::Unknown => "Last turn outcome unknown".into(),
            Self::Archived => "Archived · history only".into(),
            Self::NotLoaded => "Context not loaded".into(),
            Self::Idle => "No running turn observed".into(),
            Self::Unavailable => "Agent status unavailable · Host fault".into(),
        }
    }
}

#[cfg(test)]
mod tests;
