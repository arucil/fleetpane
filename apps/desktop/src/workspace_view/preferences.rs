use super::*;
use fleetpane_session::workspace_preferences::Snapshot;

impl WorkspaceView {
    pub(crate) fn preferences_busy(&self) -> bool {
        self.busy
    }
    pub(crate) fn preferences_epoch(&self) -> u64 {
        self.preferences_epoch
    }
    pub(crate) fn preferences_ids(&self) -> Vec<Uuid> {
        self.records
            .iter()
            .map(|record| record.id)
            .chain(self.overview.as_ref().map(|overview| overview.workspace.id))
            .collect()
    }

    pub(crate) fn apply_preferences(&mut self, snapshot: Snapshot) -> bool {
        let mut changed = false;
        for record in self.records.iter_mut().chain(&mut self.pins).chain(
            self.overview
                .as_mut()
                .map(|overview| &mut overview.workspace),
        ) {
            for fresh in snapshot.workspaces.iter().chain(&snapshot.pins) {
                changed |= record.update_names_from(fresh);
            }
        }
        // A failed pin write requires explicit recovery. Name editors retain
        // both typed text and their original CAS revision until Reload name.
        if self.pin_error.is_none()
            && !self
                .pins
                .iter()
                .map(|w| (w.id, &w.name, &w.project_name))
                .eq(snapshot
                    .pins
                    .iter()
                    .map(|w| (w.id, &w.name, &w.project_name)))
        {
            self.pins = snapshot.pins;
            changed = true;
        }
        changed
    }
}
