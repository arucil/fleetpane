//! Restoring a local view is explicit; it never claims or resumes a provider.
use super::*;
use fleetpane_session::{
    creations::Store,
    session_selection::{Saved, Target},
};

#[derive(Default)]
pub(super) struct State {
    pub busy: bool,
    pub error: Option<String>,
    pub(super) saved: Option<Saved>,
    desired: Option<Target>,
    pub restoring: Option<Target>,
}

impl State {
    pub(super) fn take_restoring(&mut self, command: &Command) -> Option<Target> {
        self.restoring.take_if(|target| {
            matches!(command, Command::Inspect { thread_id, resume: false } if thread_id == &target.thread_id)
        })
    }
}

impl HostView {
    pub(super) fn load_last_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.last_session.busy {
            return;
        }
        self.last_session.busy = true;
        self.last_session.error = None;
        self.last_session.desired = None;
        let socket = self.socket.clone();
        let directory = self.draft.directory.clone();
        let task = cx.background_executor().spawn(async move {
            background(async move {
                Store::open(directory)
                    .await?
                    .session_selection(socket)
                    .await
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.last_session.busy = false;
                match result {
                    Ok(saved) => {
                        if this.workspace_scope.is_some() {
                            cx.notify();
                            return;
                        }
                        this.last_session.saved = Some(saved);
                        this.save_last_session(window, cx);
                    }
                    Err(error) => {
                        this.last_session.saved = None;
                        this.last_session.error =
                            Some(format!("Last session could not be read: {error}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn remember_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspace_scope.is_some() {
            if let (Some(snapshot), Some(thread)) = (&self.snapshot, &self.thread) {
                self.last_session.saved = Some(Saved {
                    revision: 0,
                    target: Some(Target {
                        host_id: snapshot.host_id,
                        thread_id: thread.id.clone(),
                        cwd: thread.cwd.clone(),
                        archived: self.selected_archived,
                    }),
                });
            }
            return;
        }
        let (Some(snapshot), Some(thread)) = (&self.snapshot, &self.thread) else {
            return;
        };
        self.last_session.desired = Some(Target {
            host_id: snapshot.host_id,
            thread_id: thread.id.clone(),
            cwd: thread.cwd.clone(),
            archived: self.selected_archived,
        });
        self.save_last_session(window, cx);
    }

    fn save_last_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.last_session.busy || self.last_session.error.is_some() {
            return;
        }
        let (Some(saved), Some(target)) =
            (&self.last_session.saved, self.last_session.desired.clone())
        else {
            return;
        };
        if saved.target.as_ref() == Some(&target) {
            self.last_session.desired = None;
            return;
        }
        let revision = saved.revision;
        let socket = self.socket.clone();
        let directory = self.draft.directory.clone();
        self.last_session.busy = true;
        let task = cx.background_executor().spawn(async move {
            background(async move {
                Store::open(directory)
                    .await?
                    .save_session_selection(socket, revision, target)
                    .await
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.last_session.busy = false;
                match result {
                    Ok(saved) => {
                        this.last_session.saved = Some(saved);
                        this.save_last_session(window, cx);
                    }
                    Err(error) => {
                        this.last_session.error =
                            Some(format!("Last session was not saved: {error}"))
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn restore_last_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self
            .last_session
            .saved
            .as_ref()
            .and_then(|saved| saved.target.clone())
        else {
            return;
        };
        if self.last_session.busy
            || self.last_session.error.is_some()
            || self.busy
            || self.pending.is_some()
            || !self.owns()
            || !self.draft_can_navigate()
        {
            return;
        }
        if self
            .snapshot
            .as_ref()
            .is_none_or(|snapshot| snapshot.host_id != target.host_id)
        {
            self.error = Some(
                "Saved session belongs to a replaced Host; choose a current session explicitly."
                    .into(),
            );
            cx.notify();
            return;
        }
        self.busy = true;
        let revision = self.revision;
        let connection = self
            .connection
            .clone()
            .expect("explicit current controller");
        let socket = self.socket.clone();
        let checking = target.clone();
        let task = cx
            .background_executor()
            .spawn(async move { background(checking.observe(socket)) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                if this.revision != revision
                    || this
                        .connection
                        .as_ref()
                        .is_none_or(|current| current.lease != connection.lease)
                    || !this.owns()
                    || this.last_session.saved.as_ref().and_then(|saved|saved.target.as_ref())!=Some(&target)
                {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(()) => {
                        if !this.draft_can_navigate() {
                            this.error=Some("Last session was not opened: finish saving the current draft, then open explicitly.".into());cx.notify();return;
                        }
                        this.last_session.restoring = Some(target.clone());
                        this.tree_return = None;
                        this.tree_opening = None;
                        this.directory_target = None;
                        this.attention_target = None;
                        this.completion_target = None;
                        this.command(
                            Command::Inspect {
                                thread_id: target.thread_id,
                                resume: false,
                            },
                            window,
                            cx,
                        );
                        if !this.busy {this.last_session.restoring=None;}
                    }
                    Err(error) => {
                        this.error = Some(format!("Last session was not opened: {error}"))
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn last_session_banner(&self, cx: &mut Context<Self>) -> AnyElement {
        let saved = self
            .last_session
            .saved
            .as_ref()
            .and_then(|saved| saved.target.as_ref());
        let show = saved.is_some_and(|target| {
            self.snapshot
                .as_ref()
                .is_none_or(|snapshot| snapshot.host_id != target.host_id)
                || self.thread.as_ref().is_none_or(|thread| {
                    target.thread_id != thread.id
                        || target.cwd != thread.cwd
                        || target.archived != self.selected_archived
                })
        });
        if !show && self.last_session.error.is_none() && self.last_session.saved.is_some() {
            return div().into_any_element();
        }
        let current = saved.is_some_and(|target| {
            self.snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.host_id == target.host_id)
        });
        let label = self.last_session.error.clone().unwrap_or_else(|| {
            if self.last_session.busy {
                "Loading / saving last session…".into()
            } else if let Some(target) = saved {
                format!(
                    "Last viewed · {}{} · {} · {}",
                    if target.archived { "archived " } else { "" },
                    target.thread_id,
                    target.cwd,
                    if !current {
                        "original Host unavailable or replaced"
                    } else if !self.owns() {
                        "take control explicitly to open"
                    } else {
                        "open saved history; resume and Send remain explicit"
                    }
                )
            } else {
                "Last-session choice is unavailable. Reload to enable saving.".into()
            }
        });
        div()
            .px_4()
            .py_1()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .truncate()
                    .text_color(if self.last_session.error.is_some() {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(label),
            )
            .when(show, |bar| {
                bar.child(
                    Button::new("restore-last-session")
                        .small()
                        .label("Open last session")
                        .disabled(
                            !current
                                || !self.owns()
                                || self.busy
                                || self.pending.is_some()
                                || self.last_session.busy
                                || self.last_session.error.is_some(),
                        )
                        .on_click(
                            cx.listener(|this, _, window, cx| {
                                this.restore_last_session(window, cx)
                            }),
                        ),
                )
            })
            .when(
                self.last_session.error.is_some() || self.last_session.saved.is_none(),
                |bar| {
                    bar.child(
                        Button::new("reload-last-session")
                            .small()
                            .ghost()
                            .label("Reload choice")
                            .disabled(self.last_session.busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.load_last_session(window, cx)
                            })),
                    )
                    .child(
                        Button::new("keep-last-session")
                            .small()
                            .ghost()
                            .label("Keep previous choice")
                            .disabled(self.last_session.busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.last_session = State::default();
                                cx.notify();
                            })),
                    )
                },
            )
            .when(
                self.last_session.saved.is_some()
                    && self.last_session.error.is_none()
                    && self.thread.is_some()
                    && show,
                |bar| {
                    bar.child(
                        Button::new("remember-last-session")
                            .small()
                            .ghost()
                            .label("Remember current")
                            .disabled(self.last_session.busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.remember_session(window, cx)
                                }),
                            ),
                    )
                },
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, State, Target, Uuid};

    #[test]
    fn restoration_is_consumed_only_by_its_exact_inspection_before_reply_validation() {
        let target = Target {
            host_id: Uuid::new_v4(),
            thread_id: "saved".into(),
            cwd: "/example".into(),
            archived: true,
        };
        let mut state = State {
            restoring: Some(target.clone()),
            ..State::default()
        };
        for command in [
            Command::Inspect {
                thread_id: "other".into(),
                resume: false,
            },
            Command::Inspect {
                thread_id: "saved".into(),
                resume: true,
            },
            Command::Models { cursor: None },
        ] {
            assert_eq!(state.take_restoring(&command), None);
            assert_eq!(state.restoring, Some(target.clone()));
        }
        let exact = Command::Inspect {
            thread_id: "saved".into(),
            resume: false,
        };
        assert_eq!(state.take_restoring(&exact), Some(target));
        assert_eq!(state.take_restoring(&exact), None);
    }
}
