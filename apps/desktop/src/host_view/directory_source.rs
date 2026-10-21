//! A fleet directory target stays pinned to its observed Host incarnation.
use super::*;

impl HostView {
    pub(crate) fn prepare_directory_session(
        &mut self,
        source: fleetpane_session::fleet_directory::Source,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.attention_target = None;
        self.completion_target = None;
        self.directory_target = Some(source);
        self.open_directory_target(window, cx);
        cx.notify();
    }

    fn open_directory_target(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = &self.directory_target else {
            return;
        };
        if self.busy || self.pending.is_some() || !self.owns() {
            return;
        }
        if !self
            .snapshot
            .as_ref()
            .is_some_and(|state| state.host_id == source.host_id)
        {
            self.error = Some(
                "Directory belongs to a replaced Host. Refresh All sessions before opening.".into(),
            );
            return;
        }
        self.command(
            Command::Inspect {
                thread_id: source.thread_id.clone(),
                resume: false,
            },
            window,
            cx,
        );
    }

    pub(super) fn directory_source_banner(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(source) = &self.directory_target else {
            return div().into_any_element();
        };
        let current = self
            .snapshot
            .as_ref()
            .is_some_and(|state| state.host_id == source.host_id);
        div().px_4().py_2().border_b_1().border_color(cx.theme().border)
            .child(div().text_sm().truncate().child(format!("Directory source · {}session {}", if source.archived { "archived " } else { "" }, source.thread_id)))
            .child(div().flex().gap_2().items_center()
                .child(div().flex_1().min_w_0().text_xs().child(if self.snapshot.is_none(){"Connecting to the original Host…"}else if !current{"Host changed. Refresh All sessions to select a current session."}else if !self.owns(){"Take control explicitly, then open this session. Existing drafts are retained."}else{"Open saved history. Context resume and messages remain explicit."}))
                .child(Button::new("directory-open-target").small().label("Open selected session").disabled(!current || !self.owns() || self.busy || self.pending.is_some()).on_click(cx.listener(|this,_,window,cx|this.open_directory_target(window,cx))))
                .child(Button::new("directory-dismiss-target").small().ghost().label("Dismiss").on_click(cx.listener(|this,_,_,cx|{this.directory_target=None;cx.notify();}))))
            .into_any_element()
    }
}
