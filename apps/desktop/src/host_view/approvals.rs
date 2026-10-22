use super::*;

impl HostView {
    pub(super) fn render_approval_details(
        &self,
        pending: &Value,
        count: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_acp = pending["method"] == "session/request_permission";
        let choices = fleetpane_runtime::approvals::choices(pending);
        div()
            .px_4().py_2().flex().flex_col().gap_2()
            .border_t_1().border_color(cx.theme().border)
            .child(format!("Provider request · 1 of {count} pending"))
            .child(div().id("approval-details").max_h_24().overflow_y_scroll().text_sm()
                .when(!is_acp, |details| details.child(pending.to_string())))
            .children(is_acp.then(|| self.permission_preview.as_ref().map(|(_, view)| view.clone())).flatten())
            .children(questions::Request::decode(pending).err().map(|error|
                div().text_color(cx.theme().danger).child(error.to_string())))
            .children(choices.as_ref().err().map(|error|
                div().text_color(cx.theme().danger).child(error.clone())))
            .children(choices.ok().flatten().map(|choices| {
                div().flex().flex_col().gap_2()
                    .when(choices.unsupported > 0, |panel| panel.child(
                        div().text_xs().text_color(cx.theme().muted_foreground)
                            .child(format!("{} provider decision(s) require an unsupported response. Inspect the original request above.", choices.unsupported))))
                    .when(choices.decisions.is_empty(), |panel| panel.child(
                        div().text_color(cx.theme().danger).child("No supported approval decision is offered. The request remains pending; Stop turn is available.")))
                    .when(choices.decisions.contains(&fleetpane_runtime::approvals::Decision::AcceptForSession), |panel| panel.child(
                        div().text_xs().child("Allow for session lets later matching actions run without asking again in this session.")))
            }))
            .children(fleetpane_session::acp::Permission::decode(pending).err().map(|error|
                div().text_color(cx.theme().danger).child(error)))
    }

    /// Keep response actions outside the scrolling details and independently of
    /// nested tool-card height. No choice is selected or submitted by default.
    pub(super) fn render_approval_actions(
        &self,
        pending: Value,
        blocked: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut buttons = div().flex().flex_wrap().gap_2();
        let key = pending
            .get("id")
            .map_or_else(|| "missing".into(), Value::to_string);
        if let Ok(Some(choices)) = fleetpane_runtime::approvals::choices(&pending) {
            for (index, decision) in choices.decisions.into_iter().enumerate() {
                let pending = pending.clone();
                buttons = buttons.child(
                    Button::new(SharedString::from(format!("codex-approval:{key}:{index}")))
                        .label(decision.label())
                        .disabled(
                            blocked
                                || self.poll_paused
                                || pending
                                    .pointer("/params/threadId")
                                    .and_then(Value::as_str)
                                    .is_none(),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.answer(pending.clone(), decision, window, cx)
                        })),
                );
            }
        }
        if let Ok(Some(permission)) = fleetpane_session::acp::Permission::decode(&pending) {
            let review_valid = self
                .permission_preview
                .as_ref()
                .is_some_and(|(row, _)| row.error.is_none());
            for option in permission.options {
                let choice = option.option_id;
                let pending = pending.clone();
                buttons = buttons.child(
                    Button::new(SharedString::from(format!("acp-permission:{key}:{choice}")))
                        .label(option.name)
                        .disabled(blocked || self.poll_paused || !review_valid)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.answer_acp(pending.clone(), choice.clone(), window, cx)
                        })),
                );
            }
        }
        buttons
    }
}
