//! Search only provider-supplied choices; selection uses the ordinary typed action.
use super::*;
use gpui_kit::component::command::{Command as Palette, CommandItem, CommandState};

#[derive(Clone)]
enum Source {
    Codex,
    Acp(String),
}

#[derive(Clone)]
struct Choice {
    label: String,
    value: Option<String>,
    description: String,
}

pub(super) struct State {
    lease: fleetpane_runtime::Lease,
    session: String,
    title: String,
    source: Source,
    choices: Vec<Choice>,
    palette: Entity<CommandState>,
    return_focus: Option<FocusHandle>,
}

impl HostView {
    pub(super) fn search_models(
        &mut self,
        config: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || !self.can_act() || self.pending.is_some() || self.draining() {
            return;
        }
        let Some(thread) = &self.thread else { return };
        let Some(connection) = &self.connection else {
            return;
        };
        let (source, choices) = if self.is_acp() {
            let Some(option) =
                config.and_then(|id| thread.config_options.iter().find(|o| o.id == id))
            else {
                return;
            };
            if option.kind != "select" || !thread.can_send() {
                return;
            }
            (
                Source::Acp(option.id.clone()),
                option
                    .choices()
                    .into_iter()
                    .map(|(value, name)| Choice {
                        label: name.to_owned(),
                        value: Some(value.to_owned()),
                        description: option.name.clone(),
                    })
                    .collect(),
            )
        } else {
            if config.is_some() {
                return;
            }
            let mut choices = vec![Choice {
                label: "Use session settings".into(),
                value: None,
                description: "No next-message model override".into(),
            }];
            choices.extend(
                self.models
                    .models
                    .iter()
                    .filter(|m| !m.hidden && self.models.complete)
                    .map(|m| Choice {
                        label: m.display_name.clone(),
                        value: Some(m.model.clone()),
                        description: m.description.clone(),
                    }),
            );
            (Source::Codex, choices)
        };
        let palette = cx.new(|cx| CommandState::new(window, cx));
        self.picker = Some(State {
            lease: connection.lease.clone(),
            session: thread.id.clone(),
            title: config
                .and_then(|id| thread.config_options.iter().find(|option| option.id == id))
                .map_or_else(
                    || "Search models · Next message".to_owned(),
                    |option| format!("Search {}", option.name),
                ),
            source,
            choices,
            return_focus: window.focused(cx),
            palette: palette.clone(),
        });
        palette.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn close_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.picker.take() {
            if let Some(focus) = state.return_focus {
                focus.focus(window, cx);
            }
            cx.notify();
        }
    }

    fn confirm_picker(
        &mut self,
        index: usize,
        expected: EntityId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = &self.picker else { return };
        if state.palette.entity_id() != expected {
            return;
        }
        let Some(choice) = state.choices.get(index).cloned() else {
            return;
        };
        let session = state.session.clone();
        let source = state.source.clone();
        if self.busy
            || !self.can_act()
            || self
                .connection
                .as_ref()
                .is_none_or(|connection| connection.lease != state.lease)
            || self.pending.is_some()
            || self.draining()
            || self.thread.as_ref().is_none_or(|t| t.id != session)
        {
            self.error =
                Some("Selection expired; check the selected session and control status.".into());
            self.close_picker(window, cx);
            return;
        }
        let result = match source {
            Source::Codex if !self.is_acp() => {
                let effort = choice
                    .value
                    .as_deref()
                    .and_then(|value| self.models.find(value))
                    .filter(|model| !model.supported_reasoning_efforts.is_empty())
                    .map(|model| model.default_reasoning_effort.clone());
                TurnOptions::new(&self.models, choice.value, effort)
                    .map(|options| {
                        self.choose_options(&session, options, cx);
                    })
                    .map_err(|error| error.to_string())
            }
            Source::Acp(config_id) if self.is_acp() => {
                let value = choice.value.expect("ACP choices always carry a value");
                let valid = self.thread.as_ref().is_some_and(|thread| {
                    thread.can_send()
                        && thread.config_options.iter().any(|option| {
                            option.id == config_id
                                && option.kind == "select"
                                && option.choices().iter().any(|(v, _)| *v == value)
                        })
                });
                if valid {
                    self.command(
                        Command::Config {
                            thread_id: session,
                            config_id,
                            value,
                        },
                        window,
                        cx,
                    );
                    Ok(())
                } else {
                    Err("Provider configuration changed; open the selector again.".into())
                }
            }
            _ => Err("Provider changed; open the selector again.".into()),
        };
        if let Err(error) = result {
            self.error = Some(error);
        }
        self.close_picker(window, cx);
    }

    pub(super) fn render_picker(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let state = self.picker.as_ref().expect("open picker");
        let selected = match &state.source {
            Source::Codex => self
                .turn_options
                .get(&state.session)
                .and_then(TurnOptions::model),
            Source::Acp(id) => self
                .thread
                .as_ref()
                .filter(|thread| thread.id == state.session)
                .and_then(|thread| thread.config_options.iter().find(|option| &option.id == id))
                .and_then(|option| option.current_value.as_str()),
        };
        let items = state
            .choices
            .iter()
            .map(|choice| {
                let label = choice.label.clone();
                let checked = selected == choice.value.as_deref();
                let detail = choice
                    .value
                    .as_deref()
                    .unwrap_or("session defaults")
                    .to_owned();
                CommandItem::new()
                    .label(label.clone())
                    .keywords([detail.clone(), choice.description.clone()])
                    .child(move |_, _| {
                        div()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(div().flex_1().min_w_0().truncate().child(label.clone()))
                                    .when(checked, |row| {
                                        row.child(div().text_xs().child("Selected"))
                                    }),
                            )
                            .child(div().text_xs().truncate().child(detail.clone()))
                    })
            })
            .collect::<Vec<_>>();
        let expected = state.palette.entity_id();
        let owner = cx.entity().downgrade();
        let cancel_owner = owner.clone();
        div().id("provider-choice-search").absolute().inset_0().occlude()
            .bg(cx.theme().background).p_4().flex().flex_col().gap_2()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(div().flex().items_center().gap_2()
                .child(div().flex_1().child(state.title.clone()))
                .child(Button::new("close-choice-search").small().label("Cancel")
                    .on_click(cx.listener(|this, _, window, cx| this.close_picker(window, cx)))))
            .child(div().text_xs().child(format!("{} provider choices · Search name, ID or description", state.choices.len())))
            .when(matches!(state.source, Source::Codex) && !self.models.complete, |view| view.child(
                div().text_xs().child("Model catalog unavailable or incomplete. Only session defaults are selectable; Reload models after closing.")))
            .child(Palette::new(&state.palette).items(items).placeholder("Search provider choices…")
                .max_h((window.viewport_size().height - px(240.)).clamp(px(100.), px(360.)))
                .empty(|_, _, _| div().p_4().child("No matching provider choices"))
                .on_confirm(move |path, window, cx| {
                    if let Some(owner) = owner.upgrade() {
                        owner.update(cx, |this, cx| this.confirm_picker(path.row, expected, window, cx));
                    }
                })
                .on_cancel(move |window, cx| {
                    if let Some(owner) = cancel_owner.upgrade() {
                        window.defer(cx, move |window, cx| owner.update(cx, |this, cx| {
                            if this.picker.as_ref().is_some_and(|state| state.palette.entity_id() == expected) {
                                this.close_picker(window, cx);
                            }
                        }));
                    }
                }))
            .child(div().text_xs().child("↑/↓ select · Enter chooses · Escape clears search, then returns · Message draft is kept"))
            .into_any_element()
    }
}
