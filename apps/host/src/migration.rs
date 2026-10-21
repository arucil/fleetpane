//! CLI signals request the same cooperative Stop as the native import page.
use fleetpane_session::{
    creations::Store,
    paseo_import::{BatchControl, BatchPlan, BatchReport},
};
use tokio::signal::unix::{SignalKind, signal};
use uuid::Uuid;

pub enum Selection {
    Plan(BatchPlan),
    Retry(Uuid),
}

pub async fn run(
    store: &Store,
    selection: Selection,
) -> Result<BatchReport, Box<dyn std::error::Error + Send + Sync>> {
    // Install both listeners before polling a mutation. Tokio keeps process-wide
    // signal handlers installed, so repeated signals cannot kill a settling copy.
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let control = BatchControl::default();
    let work = async {
        match selection {
            Selection::Plan(plan) => store.import_paseo_batch(plan, control.clone()).await,
            Selection::Retry(id) => store.retry_paseo_batch(id, control.clone()).await,
        }
    };
    tokio::pin!(work);
    let received = tokio::select! {
        result = &mut work => return Ok(result?),
        _ = interrupt.recv() => "SIGINT",
        _ = terminate.recv() => "SIGTERM",
    };
    control.cancel();
    eprintln!(
        "{received}: stop requested; finishing the current copy and saving the migration report…"
    );
    // Never drop the worker on a signal. Its normal boundary writes a stopped
    // receipt and releases its locks; stdout still contains one complete report.
    Ok(work.await?)
}
