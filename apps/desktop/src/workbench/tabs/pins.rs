//! Pin the selected saved UUID even when no catalog row is mounted.
use super::*;

#[derive(Default)]
pub(super) struct State {
    pub busy: bool,
    held: bool,
    error: Option<String>,
    notice: Option<String>,
}

impl State {
    /// X11 in Kit 0.7.3 reports repeated presses with `is_held: false`.
    /// Track the physical key even when a popup/input owns its first press.
    pub(super) fn key_down(&mut self, key: &str) -> bool {
        if !key.eq_ignore_ascii_case("p") {
            return false;
        }
        std::mem::replace(&mut self.held, true)
    }

    pub(super) fn key_up(&mut self, key: &str) {
        if key.eq_ignore_ascii_case("p") {
            self.reset_key();
        }
    }

    pub(super) fn reset_key(&mut self) {
        self.held = false;
    }

    pub(super) fn close_status(&self) -> Result<(), String> {
        match &self.error {
            Some(error) => Err(format!("Workspace pin save: {error}")),
            None => Ok(()),
        }
    }
}

impl Workbench {
    pub(super) fn toggle_workspace_pin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.pins.busy || self.closing {
            return;
        }
        let Some(id) = self.tabs.selected else { return };
        let Some(entry) = self
            .tabs
            .entries
            .iter()
            .find(|entry| entry.saved.workspace == id)
        else {
            return;
        };
        let label = entry
            .workspace
            .as_ref()
            .map(|workspace| {
                format!(
                    "{} · {}",
                    workspace.registration.endpoint.name,
                    workspace.display_name()
                )
            })
            .unwrap_or_else(|| entry.saved.directory.to_string_lossy().into_owned());
        let Some(directory) = self.creation_directory.clone() else {
            return;
        };
        self.tabs.pins.busy = true;
        self.tabs.pins.notice = None;
        let task = cx.background_executor().spawn(async move {
            background(async move {
                Store::open(directory)
                    .await?
                    .toggle_workspace_pinned(id)
                    .await
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let reply = task.await;
            let _closed = this.update_in(cx, |this, _, cx| {
                this.tabs.pins.busy = false;
                match reply {
                    Ok(pins) => {
                        let pinned = pins.iter().any(|workspace| workspace.id == id);
                        this.tabs.pins.notice = Some(format!(
                            "{} · {label}",
                            if pinned {
                                "Workspace pinned"
                            } else {
                                "Workspace unpinned"
                            }
                        ));
                        this.tabs.pins.error = None;
                        if this.close_error.as_ref().is_some_and(|error| {
                            error.starts_with("Window remains open: Workspace pin save:")
                        }) {
                            this.close_error = None;
                        }
                        this.update_workspace_names(&pins);
                        if let Some(view) = &this.workspace_view {
                            view.update(cx, |view, cx| {
                                view.update_pins(pins);
                                cx.notify();
                            });
                        }
                    }
                    Err(error) => this.tabs.pins.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn pin_status(&self, cx: &Context<Self>) -> Div {
        let state = &self.tabs.pins;
        div()
            .when(state.busy, |view| {
                view.child(div().px_4().text_sm().child("Saving workspace pin…"))
            })
            .when_some(state.notice.clone(), |view, notice| {
                view.child(div().px_4().text_sm().truncate().child(notice))
            })
            .when_some(state.error.clone(), |view, error| {
                view.child(
                    div()
                        .id("workspace-pin-error")
                        .max_h(px(88.))
                        .overflow_y_scroll()
                        .px_4()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().danger)
                                .child(format!("Workspace pin save: {error}")),
                        )
                        .child(
                            Button::new("keep-saved-pins")
                                .small()
                                .label("Keep saved pins")
                                .disabled(state.busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.tabs.pins.error = None;
                                    if this.close_error.as_ref().is_some_and(|error| {
                                        error
                                            .starts_with("Window remains open: Workspace pin save:")
                                    }) {
                                        this.close_error = None;
                                    }
                                    cx.notify();
                                })),
                        ),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::State;

    #[test]
    fn one_pin_action_per_press_even_without_platform_repeat_flag() {
        let mut state = State::default();
        assert!(!state.key_down("p"));
        for _ in 0..50 {
            assert!(state.key_down("P"));
        }
        state.key_up("shift");
        assert!(state.key_down("p"));
        state.key_up("p");
        assert!(!state.key_down("P"));
        state.reset_key(); // Focus loss may prevent delivery of the key-up.
        assert!(!state.key_down("p"));
    }
}
