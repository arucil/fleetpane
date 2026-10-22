//! Observe shared local preferences without touching Host views or editor state.
use super::*;

#[derive(Default)]
pub(super) struct State {
    refreshing: bool,
    // Invalidate observations made before a local pin operation completed.
    epoch: u64,
    error: Option<String>,
}

impl State {
    pub(super) fn changed(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
    }
}

impl Workbench {
    pub(super) fn refresh_workspace_preferences(
        &mut self,
        explicit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_preferences.refreshing
            || self.closing
            || self.workspace_preferences_busy()
            || (!explicit && self.workspace_preferences.error.is_some())
            || self
                .workspace_view
                .as_ref()
                .is_some_and(|view| view.read(cx).preferences_busy())
        {
            return;
        }
        let Some(directory) = self.creation_directory.clone() else {
            return;
        };
        let mut ids = self.observed_workspace_ids();
        if let Some(view) = &self.workspace_view {
            ids.extend(view.read(cx).preferences_ids());
        } else if ids.is_empty() {
            return;
        }
        ids.sort_unstable();
        ids.dedup();
        let epoch = self.workspace_preferences.epoch;
        let catalog_epoch = self
            .workspace_view
            .as_ref()
            .map(|view| view.read(cx).preferences_epoch());
        self.workspace_preferences.refreshing = true;
        let task = cx.background_executor().spawn(async move {
            background(async move {
                Store::open(directory)
                    .await?
                    .workspace_preferences(ids)
                    .await
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, _, cx| {
                this.workspace_preferences.refreshing = false;
                if this.closing
                    || this.workspace_preferences_busy()
                    || epoch != this.workspace_preferences.epoch
                    || catalog_epoch
                        != this
                            .workspace_view
                            .as_ref()
                            .map(|view| view.read(cx).preferences_epoch())
                    || this
                        .workspace_view
                        .as_ref()
                        .is_some_and(|view| view.read(cx).preferences_busy())
                {
                    return;
                }
                match result {
                    Ok(snapshot) => {
                        let mut changed = this.workspace_preferences.error.take().is_some();
                        changed |= this.update_workspace_names(&snapshot.workspaces);
                        changed |= this.update_workspace_names(&snapshot.pins);
                        if let Some(view) = &this.workspace_view {
                            view.update(cx, |view, cx| {
                                if view.apply_preferences(snapshot) {
                                    changed = true;
                                    cx.notify();
                                }
                            });
                        }
                        if changed {
                            cx.notify();
                        }
                    }
                    Err(error) => {
                        this.workspace_preferences.error = Some(error.to_string());
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    pub(super) fn workspace_preferences_status(&self, cx: &Context<Self>) -> Stateful<Div> {
        div().id("workspace-preferences-status").when_some(
            self.workspace_preferences.error.as_ref(),
            |view, error| {
                view.px_4()
                    .py_1()
                    .max_h(px(100.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .text_sm()
                            .text_color(cx.theme().danger)
                            .child(format!("Workspace updates paused: {error}")),
                    )
                    .child(
                        Button::new("reload-workspace-preferences")
                            .small()
                            .label("Reload workspace updates")
                            .disabled(self.workspace_preferences.refreshing)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.refresh_workspace_preferences(true, window, cx)
                            })),
                    )
            },
        )
    }
}
