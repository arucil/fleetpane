use super::*;
use fleetpane_session::progress::Progress;
use gpui_kit::component::spinner::Spinner;

impl HostView {
    pub(super) fn render_progress(&self, cx: &Context<Self>) -> AnyElement {
        let Some(thread) = &self.thread else {
            return div().into_any_element();
        };
        let (working, label) = if self.poll_paused || self.observer_error.is_some() {
            (
                false,
                "Session updates paused · reload to refresh activity".into(),
            )
        } else if let Some(snapshot) = &self.snapshot {
            let progress = Progress::observed(thread, snapshot);
            (
                matches!(progress, Progress::Working { .. }),
                progress.label(),
            )
        } else {
            (false, "Agent status unavailable".into())
        };
        div()
            .id("agent-progress")
            .flex()
            .items_center()
            .gap_2()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .when(working, |row| row.child(Spinner::new().xsmall()))
            .child(label)
            .into_any_element()
    }
}
