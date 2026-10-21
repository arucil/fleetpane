use super::*;
use crate::paseo_import_view::Continuation;
use fleetpane_session::{
    creations::Resolution, creations::Store, paseo_import::continuation::Intent,
};

pub(super) struct State {
    pub store: Store,
    pub intent: Intent,
    pub resolution: Option<Resolution>,
    pub refresh_directory: bool,
}
impl HostView {
    pub(crate) fn prepare_continuation(
        &mut self,
        event: Continuation,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.busy || self.pending.is_some() {
            return Err("Wait for the selected Host operation to settle before preparing another imported context".into());
        }
        self.continuation = Some(State {
            store: event.store,
            intent: event.intent,
            resolution: None,
            refresh_directory: false,
        });
        cx.notify();
        Ok(())
    }

    pub(super) fn take_continuation_refresh(&mut self, command: &Command) -> bool {
        if !matches!(
            command,
            Command::Open { .. } | Command::History(_) | Command::Fork { .. }
        ) {
            return false;
        }
        let Some(state) = &mut self.continuation else {
            return false;
        };
        std::mem::take(&mut state.refresh_directory)
    }

    fn check_continuation(&mut self, reprepare: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(state) = &self.continuation else {
            return;
        };
        let store = state.store.clone();
        let session = state.intent.session_id;
        let expected = state.intent.id;
        self.error = None;
        self.busy = true;
        let task = cx.background_executor().spawn(async move {
            background(async move {
                let intent = if reprepare {
                    store
                        .reprepare_paseo_continuation(session, expected)
                        .await?
                } else {
                    store
                        .paseo_continuation(session)
                        .await?
                        .ok_or_else(|| Error::Input("Saved fork missing".into()))?
                };
                let resolution = intent.resolve().await?;
                Ok((intent, resolution))
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                match result {
                    Ok((intent, resolution)) => {
                        if let Some(state) = &mut this.continuation {
                            state.intent = intent;
                            state.resolution = Some(resolution);
                        }
                    }
                    Err(e) => this.error = Some(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn render_continuation(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = &self.continuation else {
            return div().into_any_element();
        };
        let resolution = state.resolution.as_ref().map(|status| match status {
            Resolution::Prepared => "Prepared; no fork dispatched".into(),
            Resolution::Pending => "Fork accepted; check status again".into(),
            Resolution::Created { thread_id } => format!("Created session: {thread_id}"),
            Resolution::Failed { error } => format!("Fork rejected: {error}"),
            Resolution::Unknown { reason } => format!("Fork result unknown: {reason}"),
        });
        let thread = match &state.resolution {
            Some(Resolution::Created { thread_id }) => Some(thread_id.clone()),
            _ => None,
        };
        let command = state.intent.command();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child("Imported Codex context")
            .child(div().text_xs().child(state.intent.cwd.clone()))
            .when_some(resolution, |panel, text| {
                panel.child(div().text_xs().child(text))
            })
            .child(
                Button::new("dispatch-paseo-fork")
                    .small()
                    .label("Fork saved context")
                    .disabled(
                        self.busy
                            || self.pending.is_some()
                            || !self.owns()
                            || self.draining()
                            || state.intent.attempted,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.command(command.clone(), window, cx)
                    })),
            )
            .child(
                Button::new("check-paseo-fork")
                    .small()
                    .label("Check saved fork status")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.check_continuation(false, window, cx)
                    })),
            )
            .when(!state.intent.attempted, |panel| {
                panel
                    .child(
                        Button::new("reprepare-paseo-fork")
                            .small()
                            .label("Reprepare unsent context")
                            .disabled(self.busy || self.pending.is_some())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.check_continuation(true, window, cx)
                            })),
                    )
                    .child(div().text_xs().child(
                        "Reprepare after a Host change. This saves a new binding without sending.",
                    ))
            })
            .when_some(thread, |panel, id| {
                panel.child(
                    Button::new("open-paseo-fork")
                        .small()
                        .label("Open forked session")
                        .disabled(self.busy || self.pending.is_some() || !self.owns())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.command(
                                Command::Inspect {
                                    thread_id: id.clone(),
                                    resume: true,
                                },
                                window,
                                cx,
                            )
                        })),
                )
            })
            .into_any_element()
    }
}
