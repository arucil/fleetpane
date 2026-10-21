//! Source navigation retains the existing draft and explicit ownership boundary.
use super::*;
use fleetpane_session::attention::Item;

impl HostView {
    pub(crate) fn prepare_attention(
        &mut self,
        item: Item,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tree_return = None;
        self.tree_opening = None;
        self.directory_target = None;
        self.completion_target = None;
        self.attention_target = Some(item);
        if self.owns() {
            self.open_attention_target(window, cx);
        }
        cx.notify();
    }

    fn open_attention_target(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.attention_target.as_ref() else {
            return;
        };
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        if let Err(error) = item.validate(snapshot) {
            self.error = Some(error.to_string());
            cx.notify();
            return;
        }
        let thread_id = item.thread_id.clone().expect("validated session target");
        if self
            .thread
            .as_ref()
            .is_some_and(|thread| thread.id == thread_id)
        {
            self.focus(window, cx);
            return;
        }
        self.command(
            Command::Inspect {
                thread_id,
                resume: false,
            },
            window,
            cx,
        );
    }

    pub(super) fn attention_banner(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(item) = self.attention_target.as_ref() else {
            return div().into_any_element();
        };
        let validation = self
            .snapshot
            .as_ref()
            .map(|snapshot| item.validate(snapshot));
        let valid = matches!(validation, Some(Ok(())));
        let message=match validation {
            None=>"Connecting to the request's Host…".into(),
            Some(Err(error))=>error.to_string(),
            Some(Ok(())) if !self.owns()=>"Take control, then open the pending session. No answer has been sent.".into(),
            Some(Ok(()))=>"Open the original pending session; its existing review controls keep the original request ID.".into(),
        };
        div()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(div().text_sm().truncate().child(format!(
                "Attention source · session {} · request {}",
                item.thread_id.as_deref().unwrap_or("unavailable"),
                item.id
            )))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().min_w_0().text_sm().child(message))
                    .child(
                        Button::new("attention-open-session")
                            .small()
                            .label("Open pending session")
                            .disabled(!valid || !self.owns() || self.busy || self.pending.is_some())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_attention_target(window, cx)
                            })),
                    )
                    .child(
                        Button::new("attention-dismiss")
                            .small()
                            .ghost()
                            .label("Dismiss source")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.attention_target = None;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }
}
