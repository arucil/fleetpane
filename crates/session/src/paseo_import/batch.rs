//! Sequential migration from a pinned selection, with cooperative cancellation.
use super::{Catalog, Roots, discover, invalid};
use crate::{client::Error, hosts::Endpoint, storage::Store};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use uuid::Uuid;
mod receipts;
pub use receipts::{BatchPage, BatchReceipt, BatchState, BatchSummary};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub workspace_id: String,
    pub revision: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BatchPlan {
    pub home: PathBuf,
    pub workspaces: Vec<Selection>,
    pub endpoint: Endpoint,
    pub roots: Roots,
}

impl BatchPlan {
    pub fn selected(
        catalog: &Catalog,
        ids: &BTreeSet<String>,
        endpoint: Endpoint,
        roots: Roots,
    ) -> Result<Self, Error> {
        let workspaces: Vec<_> = catalog
            .candidates
            .iter()
            .filter(|c| ids.contains(&c.source.workspace_id))
            .map(|c| Selection {
                workspace_id: c.source.workspace_id.clone(),
                revision: c.revision.clone(),
            })
            .collect();
        if workspaces.len() != ids.len() {
            return Err(invalid("selected workspace absent from preview"));
        }
        let plan = Self {
            home: catalog.home.clone(),
            workspaces,
            endpoint,
            roots,
        };
        plan.validate()?;
        Ok(plan)
    }

    /// Uses the same bounded, no-follow JSON boundary as source metadata.
    pub fn read(path: &std::path::Path) -> Result<Self, Error> {
        let (plan, _): (Self, _) = super::read_json(path)?;
        plan.validate()?;
        Ok(plan)
    }

    fn validate(&self) -> Result<(), Error> {
        crate::hosts::Configuration::single(self.endpoint.socket.clone()).map_err(invalid)?;
        if !self.home.is_absolute()
            || !self.roots.codex.is_absolute()
            || !self.roots.dsh.is_absolute()
        {
            return Err(invalid("batch source and provider roots must be absolute"));
        }
        if self.workspaces.is_empty() || self.workspaces.len() > super::MAX_RECORDS {
            return Err(invalid("select between 1 and 20,000 workspaces"));
        }
        let mut ids = BTreeSet::new();
        for selected in &self.workspaces {
            if selected.workspace_id.is_empty()
                || !ids.insert(&selected.workspace_id)
                || selected.revision.len() != 64
                || !selected.revision.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(invalid(
                    "duplicate/empty selection or invalid preview revision",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct BatchProgress {
    pub workspaces_done: usize,
    pub workspaces_total: usize,
    pub histories_available: usize,
    pub histories_failed: usize,
    pub current_workspace: Option<String>,
    pub current_session: Option<String>,
}

/// One shared progress slot, not an accumulating event queue. Cancellation never
/// drops an in-flight file copy or transaction; it prevents the next unit.
#[derive(Clone, Default)]
pub struct BatchControl {
    cancelled: Arc<AtomicBool>,
    progress: Arc<Mutex<BatchProgress>>,
}
impl BatchControl {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
    pub fn progress(&self) -> BatchProgress {
        self.progress
            .lock()
            .expect("batch progress poisoned")
            .clone()
    }
    fn publish(&self, progress: &BatchProgress) {
        *self.progress.lock().expect("batch progress poisoned") = progress.clone();
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkspaceResult {
    pub source_id: String,
    pub name: String,
    pub imported_id: Option<Uuid>,
    pub reused: bool,
    pub registration_error: Option<String>,
    pub error: Option<String>,
    pub session_count: usize,
    pub histories_available: usize,
    pub history_failures: Vec<(Uuid, String)>,
    pub history_warnings: Vec<(Uuid, String)>,
    pub histories_finished: bool,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct BatchReport {
    pub batch_id: Uuid,
    pub progress: BatchProgress,
    pub cancelled: bool,
    pub results: Vec<WorkspaceResult>,
}

impl Store {
    /// Revalidate the selected revisions against one fresh source snapshot,
    /// then commit each workspace and publish each transcript independently.
    /// Reusing a plan resumes saved imports; it does not synchronize changes.
    pub async fn import_paseo_batch(
        &self,
        plan: BatchPlan,
        control: BatchControl,
    ) -> Result<BatchReport, Error> {
        plan.validate()?;
        let store = self.clone();
        // Awaiter cancellation must not release the writer lock while a
        // spawn_blocking copy/transaction is still finishing. Stop is explicit.
        tokio::spawn(async move { store.execute_paseo_batch(plan, control).await })
            .await
            .map_err(invalid)?
    }

    async fn execute_paseo_batch(
        &self,
        plan: BatchPlan,
        control: BatchControl,
    ) -> Result<BatchReport, Error> {
        plan.validate()?;
        let (id, _writer) = self.begin_paseo_batch(plan.clone()).await?;
        let result = self.run_paseo_batch(id, plan, control).await;
        match &result {
            Ok(report) => self.finish_paseo_batch(report).await?,
            Err(error) => {
                if let Err(save_error) = self.fail_paseo_batch(id, error.to_string()).await {
                    return Err(invalid(format!(
                        "{error}; saving batch failure also failed: {save_error}"
                    )));
                }
            }
        }
        result
    }

    async fn run_paseo_batch(
        &self,
        id: Uuid,
        plan: BatchPlan,
        control: BatchControl,
    ) -> Result<BatchReport, Error> {
        let mut progress = BatchProgress {
            workspaces_total: plan.workspaces.len(),
            ..Default::default()
        };
        control.publish(&progress);
        let mut report = BatchReport {
            batch_id: id,
            progress: progress.clone(),
            cancelled: false,
            results: Vec::new(),
        };
        if control.is_cancelled() {
            report.cancelled = true;
            return Ok(report);
        }
        let fresh = discover(plan.home.clone()).await?;
        let mut candidates: BTreeMap<_, _> = fresh
            .candidates
            .into_iter()
            .map(|c| (c.source.workspace_id.clone(), c))
            .collect();
        for selected in plan.workspaces {
            if control.is_cancelled() {
                report.cancelled = true;
                break;
            }
            let candidate = candidates.remove(&selected.workspace_id);
            let mut result = WorkspaceResult {
                source_id: selected.workspace_id.clone(),
                name: candidate
                    .as_ref()
                    .map(|c| c.source.name().to_owned())
                    .unwrap_or_else(|| selected.workspace_id.clone()),
                imported_id: None,
                reused: false,
                registration_error: None,
                error: None,
                session_count: 0,
                histories_available: 0,
                history_failures: Vec::new(),
                history_warnings: Vec::new(),
                histories_finished: false,
            };
            progress.current_workspace = Some(result.name.clone());
            progress.current_session = None;
            self.checkpoint_paseo_batch(id, &progress, Some((report.results.len(), &result)))
                .await?;
            control.publish(&progress);
            match candidate {
                Some(candidate) if candidate.revision == selected.revision => {
                    match self
                        .save_paseo_candidate(fresh.home.clone(), candidate, plan.endpoint.clone())
                        .await
                    {
                        Ok((saved, reused)) => {
                            result.imported_id = Some(saved.id);
                            result.reused = reused;
                            result.session_count = saved.session_count;
                            result.registration_error = saved.registration_error;
                            self.checkpoint_paseo_batch(
                                id,
                                &progress,
                                Some((report.results.len(), &result)),
                            )
                            .await?;
                            let mut before = None;
                            'pages: loop {
                                if control.is_cancelled() {
                                    report.cancelled = true;
                                    break;
                                }
                                let sessions = self.paseo_sessions(saved.id, before).await?;
                                if sessions.is_empty() {
                                    result.histories_finished = true;
                                    break;
                                }
                                before = sessions.last().map(|s| s.position);
                                for session in sessions {
                                    if control.is_cancelled() {
                                        report.cancelled = true;
                                        break 'pages;
                                    }
                                    progress.current_session = Some(session.agent.id.clone());
                                    self.checkpoint_paseo_batch(
                                        id,
                                        &progress,
                                        Some((report.results.len(), &result)),
                                    )
                                    .await?;
                                    control.publish(&progress);
                                    match self
                                        .snapshot_paseo_history(session.id, plan.roots.clone())
                                        .await
                                    {
                                        Ok(status) => {
                                            result.histories_available += 1;
                                            progress.histories_available += 1;
                                            if let Some(warning) = status.warning {
                                                result.history_warnings.push((session.id, warning));
                                            }
                                        }
                                        Err(e) => {
                                            result.history_failures.push((
                                                session.id,
                                                format!(
                                                    "{} ({}): {e}",
                                                    session
                                                        .agent
                                                        .title
                                                        .as_deref()
                                                        .unwrap_or(&session.agent.id),
                                                    session.agent.id
                                                ),
                                            ));
                                            progress.histories_failed += 1;
                                        }
                                    }
                                    self.checkpoint_paseo_batch(
                                        id,
                                        &progress,
                                        Some((report.results.len(), &result)),
                                    )
                                    .await?;
                                    control.publish(&progress);
                                }
                            }
                        }
                        Err(e) => result.error = Some(e.to_string()),
                    }
                }
                _ => {
                    result.error = Some(
                        "Paseo workspace changed or disappeared; preview again before importing"
                            .into(),
                    )
                }
            }
            if !report.cancelled {
                progress.workspaces_done += 1;
            }
            self.checkpoint_paseo_batch(id, &progress, Some((report.results.len(), &result)))
                .await?;
            report.results.push(result);
            control.publish(&progress);
            if report.cancelled {
                break;
            }
        }
        progress.current_workspace = None;
        progress.current_session = None;
        control.publish(&progress);
        report.progress = progress;
        Ok(report)
    }
}
