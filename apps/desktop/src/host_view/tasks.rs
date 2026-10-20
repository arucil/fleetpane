use super::*;
use fleetpane_runtime::tasks::View;

pub(super) struct State {
    loading: bool,
    error: Option<String>,
    expanded: bool,
    generation: u64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            loading: false,
            error: None,
            expanded: true,
            generation: 0,
        }
    }
}

impl HostView {
    pub(super) fn apply_tasks(&mut self, id: &str, completed: Completed) -> Result<(), Error> {
        let view: View =
            serde_json::from_value(completed.result).map_err(|e| Error::Protocol(e.to_string()))?;
        if view.sequence != completed.sequence {
            return Err(Error::Protocol(
                "Task view sequence differs from its read boundary".into(),
            ));
        }
        if let Some(plan) = &view.plan {
            plan.validate().map_err(Error::Protocol)?;
            if plan.thread_id != id {
                return Err(Error::Protocol(
                    "Task view belongs to another session".into(),
                ));
            }
        }
        if let Some(thread) = self.thread.as_mut().filter(|t| t.id == id)
            && thread
                .task_plan
                .as_ref()
                .is_none_or(|old| view.sequence >= old.sequence)
        {
            thread.task_plan = Some(view);
        }
        Ok(())
    }

    pub(super) fn load_tasks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(thread), Some(connection)) = (&self.thread, self.connection.clone()) else {
            return;
        };
        let id = thread.id.clone();
        let host = connection.lease.host_id;
        self.tasks.generation += 1;
        let generation = self.tasks.generation;
        self.tasks.loading = true;
        self.tasks.error = None;
        let command = Command::Tasks {
            thread_id: id.clone(),
        };
        let task = cx
            .background_executor()
            .spawn(async move { background(connection.execute(&command, Uuid::new_v4())) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update(cx, |this, cx| {
                if this.tasks.generation != generation
                    || this.snapshot.as_ref().is_none_or(|s| s.host_id != host)
                    || this.thread.as_ref().is_none_or(|t| t.id != id)
                {
                    return;
                }
                this.tasks.loading = false;
                this.tasks.error = result
                    .and_then(|result| this.apply_tasks(&id, result))
                    .err()
                    .map(|e| e.to_string());
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn render_tasks(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let plan = self
            .thread
            .as_ref()
            .and_then(|t| t.task_plan.as_ref())
            .and_then(|v| v.plan.as_ref());
        let previous = plan.is_some_and(|p| {
            self.thread
                .as_ref()
                .and_then(Thread::active_turn)
                .is_some_and(|turn| turn.id != p.turn_id)
        });
        div()
            .px_4()
            .py_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().text_sm().child(match plan {
                        Some(p) => format!(
                            "{} · {}/{} completed",
                            if previous {
                                "Tasks from previous turn"
                            } else {
                                "Agent tasks"
                            },
                            p.entries.iter().filter(|e| e.status == "completed").count(),
                            p.entries.len()
                        ),
                        None => "Agent tasks".into(),
                    }))
                    .child(
                        Button::new("refresh-agent-tasks")
                            .small()
                            .ghost()
                            .label(if self.tasks.loading {
                                "Loading…"
                            } else {
                                "Refresh"
                            })
                            .disabled(self.tasks.loading || self.thread.is_none())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.load_tasks(window, cx)),
                            ),
                    )
                    .when_some(plan, |header, plan| {
                        let copy = plan.text();
                        header
                            .child(
                                Button::new("copy-agent-tasks")
                                    .small()
                                    .ghost()
                                    .label("Copy")
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            copy.clone(),
                                        ))
                                    }),
                            )
                            .child(
                                Button::new("expand-agent-tasks")
                                    .small()
                                    .ghost()
                                    .label(if self.tasks.expanded {
                                        "Hide tasks"
                                    } else {
                                        "Show tasks"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.tasks.expanded = !this.tasks.expanded;
                                        cx.notify();
                                    })),
                            )
                    }),
            )
            .when_some(self.tasks.error.as_ref(), |row, error| {
                row.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(format!("Cannot read agent tasks: {error}")),
                )
            })
            .when(
                plan.is_none() && !self.tasks.loading && self.tasks.error.is_none(),
                |row| {
                    row.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("No task plan reported or retained by this Host."),
                    )
                },
            )
            .when_some(plan.filter(|_| self.tasks.expanded), |row, plan| {
                row.child(
                    div()
                        .id("agent-task-entries")
                        .max_h(rems(12.))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .when_some(plan.explanation.as_ref(), |list, explanation| {
                            list.child(div().text_sm().child(explanation.clone()))
                        })
                        .when(plan.entries.is_empty(), |list| {
                            list.child(div().text_sm().child("Agent cleared the task list."))
                        })
                        .children(plan.entries.iter().map(|entry| {
                            let status = match entry.status.as_str() {
                                "completed" => "Done",
                                "in_progress" => "In progress",
                                "pending" => "Pending",
                                _ => "Unknown status",
                            };
                            let label = if status == "Unknown status" {
                                format!("Unknown status: {}", entry.status)
                            } else {
                                status.into()
                            };
                            let priority = entry
                                .priority
                                .as_ref()
                                .map(|p| format!(" · {p}"))
                                .unwrap_or_default();
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    div()
                                        .w(rems(9.))
                                        .flex_shrink_0()
                                        .text_xs()
                                        .text_color(if entry.status == "in_progress" {
                                            cx.theme().primary
                                        } else {
                                            cx.theme().muted_foreground
                                        })
                                        .child(format!("{label}{priority}")),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_sm()
                                        .child(entry.content.clone()),
                                )
                        })),
                )
            })
    }
}
