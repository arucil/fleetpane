use super::*;

impl HostView {
    pub(super) fn reported_session_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.is_acp() || self.thread.is_none() {
            return div().into_any_element();
        }
        let thread = self.thread.as_ref().expect("selected session");
        let data = serde_json::json!({
            "threadId":thread.id,
            "source":"provider-session-settings",
            "model":thread.model,
            "modelProvider":thread.model_provider,
            "reasoningEffort":thread.reasoning_effort,
        });
        let raw = serde_json::to_string_pretty(&data).expect("typed session settings serialize");
        div()
            .px_4()
            .pb_2()
            .flex()
            .items_start()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "Provider session settings · Model: {} · Provider: {} · Effort: {}",
                        thread.model.as_deref().unwrap_or("Unavailable"),
                        thread.model_provider.as_deref().unwrap_or("Unavailable"),
                        thread
                            .reasoning_effort
                            .as_deref()
                            .unwrap_or("Unset or unavailable")
                    ))
                    .child(
                        "Current or persisted settings; individual turns may use another model.",
                    ),
            )
            .child(
                Button::new("copy-session-settings")
                    .small()
                    .ghost()
                    .label("Copy settings")
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(raw.clone()))
                    }),
            )
            .into_any_element()
    }
}
