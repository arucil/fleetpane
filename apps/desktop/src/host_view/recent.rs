use super::*;

#[derive(Default)]
pub(super) struct State {
    paths: Vec<PathBuf>,
    pub busy: bool,
    pending_directory: Option<PathBuf>,
    error: Option<String>,
    session_paths: Vec<String>,
    session_directory_count: usize,
    session_error: Option<String>,
}

pub(super) enum Action {
    Load,
    Remember(PathBuf),
    Clear,
}

impl HostView {
    pub(super) fn update_session_directories(&mut self) {
        match self.directory.workspace_suggestions() {
            Ok(suggestions) => {
                self.recent.session_paths =
                    suggestions.paths.into_iter().map(str::to_owned).collect();
                self.recent.session_directory_count = suggestions.total;
                self.recent.session_error = None;
            }
            Err(error) => {
                self.recent.session_paths.clear();
                self.recent.session_directory_count = 0;
                self.recent.session_error = Some(error.to_string());
            }
        }
    }

    fn render_session_directories(&self, cx: &Context<Self>) -> impl IntoElement {
        let paths = self.recent.session_paths.clone();
        let total = self.recent.session_directory_count;
        let owner = cx.entity().downgrade();
        let prepared = self.creation_target.is_some()
            || self.creation_intent.is_some()
            || self.workspace_target.is_some();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                Button::new("session-directories")
                    .small()
                    .ghost()
                    .label(format!("Loaded session directories · {total}"))
                    .disabled(self.busy || prepared || self.recent.session_error.is_some())
                    .dropdown_menu_with_anchor(Anchor::BottomLeft, move |mut menu, _, _| {
                        menu = menu.label("Current Host · current filters · loaded pages");
                        for path in &paths {
                            let owner = owner.clone();
                            let path = path.clone();
                            menu = menu.item(PopupMenuItem::new(path.clone()).on_click(
                                move |_, window, cx| {
                                    if let Some(entity) = owner.upgrade() {
                                        entity.update(cx, |this, cx| {
                                            if this.busy
                                                || this.creation_target.is_some()
                                                || this.creation_intent.is_some()
                                                || this.workspace_target.is_some()
                                            {
                                                return;
                                            }
                                            this.workspace.update(cx, |input, cx| {
                                                input.set_value(path.clone(), window, cx)
                                            });
                                        });
                                    }
                                },
                            ));
                        }
                        if total > paths.len() {
                            menu = menu.label(format!(
                                "Showing first {} of {total} directories",
                                paths.len()
                            ));
                        }
                        if total == 0 {
                            menu = menu.label("No directories in loaded sessions");
                        }
                        menu
                    }),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("From loaded sessions on this Host; filters and paging apply."),
            )
            .children(self.recent.session_error.iter().map(|error| {
                div()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(error.clone())
            }))
    }

    pub(super) fn update_recent(
        &mut self,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.recent.busy {
            if let Action::Remember(path) = action {
                self.recent.pending_directory = Some(path);
            }
            return;
        }
        self.recent.busy = true;
        self.recent.error = None;
        let directory = self.draft.directory.clone();
        let socket = self.socket.clone();
        let saving = matches!(action, Action::Remember(_));
        let task = cx.background_executor().spawn(async move {
            background(async move {
                let store = fleetpane_session::creations::Store::open(directory).await?;
                match action {
                    Action::Load => store.recent_directories(socket).await,
                    Action::Remember(cwd) => store.remember_directory(socket, cwd).await,
                    Action::Clear => {
                        store.clear_recent_directories(socket).await?;
                        Ok(Vec::new())
                    }
                }
            })
        });
        cx.spawn_in(window,async move|this,cx| {
            let result=task.await;
            let _closed=this.update_in(cx,|this,window,cx| {
                this.recent.busy=false;
                match result {
                    Ok(paths)=>this.recent.paths=paths,
                    Err(error)=>this.recent.error=Some(if saving {
                        format!("Session created, but its recent directory could not be saved: {error}")
                    } else {format!("Recent directories: {error}")}),
                }
                if let Some(path) = this.recent.pending_directory.take() {
                    this.update_recent(Action::Remember(path), window, cx);
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    pub(super) fn render_recent(&self, cx: &Context<Self>) -> impl IntoElement {
        let paths = self.recent.paths.clone();
        let owner = cx.entity().downgrade();
        let prepared = self.creation_target.is_some()
            || self.creation_intent.is_some()
            || self.workspace_target.is_some();
        let busy = self.recent.busy;
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(self.render_session_directories(cx))
            .child(
                Button::new("recent-directories")
                    .small()
                    .ghost()
                    .label(if busy {
                        "Recent directories · loading".into()
                    } else {
                        format!("Recent directories · {}", paths.len())
                    })
                    .disabled(busy || self.busy)
                    .dropdown_menu_with_anchor(Anchor::BottomLeft, move |mut menu, _, _| {
                        for path in &paths {
                            let owner = owner.clone();
                            let path = path.clone();
                            menu = menu.item(
                                PopupMenuItem::new(path.to_string_lossy().into_owned())
                                    .disabled(prepared)
                                    .on_click(move |_, window, cx| {
                                        if let Some(entity) = owner.upgrade() {
                                            entity.update(cx, |this, cx| {
                                                this.workspace.update(cx, |input, cx| {
                                                    input.set_value(
                                                        path.to_string_lossy().into_owned(),
                                                        window,
                                                        cx,
                                                    )
                                                });
                                            });
                                        }
                                    }),
                            );
                        }
                        if !paths.is_empty() {
                            menu = menu.separator();
                        }
                        let reload = owner.clone();
                        menu = menu.item(PopupMenuItem::new("Reload suggestions").on_click(
                            move |_, window, cx| {
                                if let Some(entity) = reload.upgrade() {
                                    entity.update(cx, |this, cx| {
                                        this.update_recent(Action::Load, window, cx)
                                    });
                                }
                            },
                        ));
                        if !paths.is_empty() {
                            let owner = owner.clone();
                            menu =
                                menu.item(PopupMenuItem::new("Forget these suggestions").on_click(
                                    move |_, window, cx| {
                                        if let Some(entity) = owner.upgrade() {
                                            entity.update(cx, |this, cx| {
                                                this.update_recent(Action::Clear, window, cx)
                                            });
                                        }
                                    },
                                ));
                        }
                        menu
                    }),
            )
            .when(prepared, |view| {
                view.child(
                    div()
                        .text_xs()
                        .child("Prepared workspace: keep its bound directory."),
                )
            })
            .children(self.recent.error.iter().map(|error| {
                div()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(error.clone())
            }))
    }
}
