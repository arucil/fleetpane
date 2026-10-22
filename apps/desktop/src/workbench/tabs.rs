//! Bounded workspace views and revisioned device-local layout persistence.
mod drag;
mod navigation;
use super::*;
use crate::{file_locations::Location, files_view::FilesView, terminal_view::TerminalView};
use fleetpane_session::{
    workspace_layout::{Layout, MAX_TABS, Panel, Saved, Tab},
    workspaces::Workspace,
};
use uuid::Uuid;

struct WorkspaceTab {
    saved: Tab,
    workspace: Option<Workspace>,
    error: Option<String>,
    host: Option<Entity<HostView>>,
    host_observation: Option<Subscription>,
    file_request: Option<Subscription>,
    files: Option<Entity<FilesView>>,
    file_observation: Option<Subscription>,
    file_back: Option<Subscription>,
    terminals: Option<Entity<TerminalView>>,
    terminal_observation: Option<Subscription>,
    terminal_back: Option<Subscription>,
}
impl WorkspaceTab {
    fn new(saved: Tab, workspace: Option<Workspace>, error: Option<String>) -> Self {
        Self {
            saved,
            workspace,
            error,
            host: None,
            host_observation: None,
            file_request: None,
            files: None,
            file_observation: None,
            file_back: None,
            terminals: None,
            terminal_observation: None,
            terminal_back: None,
        }
    }
}

#[derive(Default)]
pub(super) struct State {
    entries: Vec<WorkspaceTab>,
    selected: Option<Uuid>,
    pub visible: bool,
    saved: Option<Saved>,
    pub busy: bool,
    error: Option<String>,
    saving_paused: bool,
    scroll: ScrollHandle,
}

impl Workbench {
    pub(super) fn tab_export_pending(&self, cx: &App) -> bool {
        self.tabs.entries.iter().any(|entry| {
            entry
                .files
                .as_ref()
                .is_some_and(|files| files.read(cx).export_pending())
        })
    }
    pub(super) fn tabs_active(&self) -> bool {
        self.tabs.visible
            && !self.choosing
            && !self.browsing_workspaces
            && !self.sessions.active
            && !self.attention.active
            && !self.diagnostics.active
    }
    fn layout(&self) -> Layout {
        Layout {
            tabs: self
                .tabs
                .entries
                .iter()
                .map(|entry| entry.saved.clone())
                .collect(),
            selected: self.tabs.selected,
            visible: self.tabs.visible,
        }
    }
    pub(super) fn tabs_close_status(&self, cx: &App) -> Result<(), String> {
        if let Some(error) = &self.tabs.error {
            return Err(error.clone());
        }
        for entry in &self.tabs.entries {
            if let Some(host) = &entry.host {
                host.read(cx).draft_close_status()?;
            }
        }
        Ok(())
    }
    pub(super) fn tabs_ready(&self, cx: &App) -> Result<bool, String> {
        let mut ready = !self.tabs.busy;
        for entry in &self.tabs.entries {
            if let Some(host) = &entry.host {
                ready &= host.read(cx).draft_close_status()?;
            }
            if let Some(terminal) = &entry.terminals {
                ready &= terminal.read(cx).close_status()?;
            }
            if let Some(files) = &entry.files {
                ready &= !files.read(cx).export_pending();
            }
        }
        Ok(ready)
    }
    pub(super) fn load_tabs(&mut self, startup: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.busy {
            return;
        }
        match self.tabs_ready(cx) {
            Ok(true) => {}
            Ok(false) => {
                self.route_notice = Some(
                    "Wait for local drafts and terminal input before reloading the saved layout."
                        .into(),
                );
                cx.notify();
                return;
            }
            Err(error) => {
                self.route_notice = Some(format!("Layout remains open: {error}"));
                cx.notify();
                return;
            }
        }
        for entry in &self.tabs.entries {
            let ready = (|| {
                let host = entry
                    .host
                    .as_ref()
                    .map_or(Ok(true), |host| host.read(cx).tab_close_status())?;
                let terminal = entry
                    .terminals
                    .as_ref()
                    .map_or(Ok(true), |view| view.read(cx).tab_close_status())?;
                Ok::<_, String>(host && terminal)
            })();
            if !matches!(ready, Ok(true)) {
                self.route_notice = Some("Finish local saves and check unresolved operations before reloading the layout.".into());
                cx.notify();
                return;
            }
        }
        let Some(directory) = self.creation_directory.clone() else {
            return;
        };
        self.tabs.busy = true;
        let task = cx.background_executor().spawn(async move {
            background(async move {
                let store = Store::open(directory).await?;
                let saved = store.workspace_layout().await?;
                let mut entries = Vec::new();
                for tab in &saved.layout.tabs {
                    let result = async {
                        let workspace = store.workspace(tab.workspace).await?;
                        tab.check(&workspace)?;
                        workspace.inspect().await?;
                        Ok::<_, Error>(workspace)
                    }
                    .await;
                    entries.push(match result {
                        Ok(workspace) => (tab.clone(), Some(workspace), None),
                        Err(error) => (tab.clone(), None, Some(error.to_string())),
                    });
                }
                Ok((saved, entries))
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.tabs.busy = false;
                match result {
                    Ok((saved, entries)) => {
                        this.tabs.entries = entries
                            .into_iter()
                            .map(|(tab, workspace, error)| WorkspaceTab::new(tab, workspace, error))
                            .collect();
                        this.tabs.selected = saved.layout.selected;
                        this.tabs.visible = saved.layout.visible
                            && (!startup
                                || (!this.host_choice.selected_during_load
                                    && !this.choosing
                                    && !this.browsing_workspaces
                                    && !this.sessions.active
                                    && !this.attention.active
                                    && !this.diagnostics.active));
                        this.tabs.saved = Some(saved);
                        if this.tabs.error.as_ref().is_some_and(|error| {
                            this.close_error.as_deref()
                                == Some(format!("Window remains open: {error}").as_str())
                        }) {
                            this.close_error = None;
                        }
                        this.tabs.error = None;
                        this.tabs.saving_paused = false;
                        if this.tabs.visible
                            && let Some(id) = this.tabs.selected
                        {
                            this.select_workspace_tab(id, window, cx);
                        }
                    }
                    Err(error) => {
                        this.tabs.error =
                            Some(format!("Workspace layout could not be read: {error}"))
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_workspace_tab(
        &mut self,
        workspace: Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.busy && self.tabs.saved.is_none() || self.tabs.error.is_some() {
            return;
        }
        let id = workspace.id;
        if !self
            .tabs
            .entries
            .iter()
            .any(|tab| tab.saved.workspace == id)
        {
            if self.tabs.entries.len() >= MAX_TABS {
                self.route_notice = Some(
                    "32 workspace tabs are open. Close a tab before opening another workspace."
                        .into(),
                );
                cx.notify();
                return;
            }
            self.tabs.entries.push(WorkspaceTab::new(
                Tab::new(&workspace),
                Some(workspace),
                None,
            ));
        }
        self.select_workspace_tab(id, window, cx);
    }

    fn select_workspace_tab(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if self.export_pending(cx) {
            return;
        }
        let Some(index) = self
            .tabs
            .entries
            .iter()
            .position(|entry| entry.saved.workspace == id)
        else {
            return;
        };
        self.tabs.visible = true;
        self.tabs.selected = Some(id);
        self.tabs.scroll.scroll_to_item(index);
        self.choosing = false;
        self.browsing_workspaces = false;
        self.sessions.active = false;
        self.diagnostics.active = false;
        self.leave_attention();
        self.file_navigation = files::State::default();
        self.route_notice = None;
        let socket = self.tabs.entries[index].saved.socket.clone();
        let host_index = self
            .slots
            .iter()
            .position(|slot| slot.endpoint.socket == socket);
        if let Some(host_index) = host_index {
            self.selected = host_index;
        } else {
            self.tabs.entries[index].error=Some("Saved Host is absent from this configuration. Restore its endpoint before using this workspace.".into());
        }
        if self.tabs.entries[index].host.is_none()
            && let (Some(host_index), Some(workspace)) =
                (host_index, self.tabs.entries[index].workspace.clone())
        {
            let directory = self
                .creation_directory
                .clone()
                .expect("local controller directory");
            let control = self.slots[host_index].control.clone();
            let saved = self.tabs.entries[index].saved.clone();
            let view = cx.new(|cx| {
                let mut view = HostView::new(socket, control, directory, window, cx);
                view.scope_workspace(workspace, &saved, window, cx);
                view
            });
            let observation = cx.observe(&view, |_, _, cx| cx.notify());
            let subscription = cx.subscribe_in(
                &view,
                window,
                move |this, _, request: &crate::host_view::FileRequest, window, cx| {
                    this.open_tab_file(id, request, window, cx)
                },
            );
            let entry = &mut self.tabs.entries[index];
            entry.host = Some(view);
            entry.host_observation = Some(observation);
            entry.file_request = Some(subscription);
        }
        let panel = self.tabs.entries[index].saved.panel;
        self.workspace_panel(id, panel, window, cx);
        self.capture_tabs(window, cx);
        cx.notify();
    }

    fn open_tab_file(
        &mut self,
        id: Uuid,
        request: &crate::host_view::FileRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.tabs_active() || self.tabs.selected != Some(id) {
            return;
        }
        let Some(index) = self
            .tabs
            .entries
            .iter()
            .position(|entry| entry.saved.workspace == id)
        else {
            return;
        };
        let entry = &self.tabs.entries[index];
        if entry
            .host
            .as_ref()
            .is_none_or(|host| !host.read(cx).matches_file_request(request))
        {
            return;
        }
        let Some(workspace) = entry.workspace.clone() else {
            return;
        };
        let files = cx.new(|cx| FilesView::target(workspace, request.target.clone(), window, cx));
        self.attach_tab_files(index, files, window, cx);
        self.workspace_panel(id, Panel::Files, window, cx);
    }
    fn attach_tab_files(
        &mut self,
        index: usize,
        files: Entity<FilesView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.tabs.entries[index].saved.workspace;
        let observation = cx.observe(&files, |_, _, cx| cx.notify());
        let back = cx.subscribe_in(&files, window, move |this, _, _: &Location, window, cx| {
            this.workspace_panel(id, Panel::Conversation, window, cx)
        });
        let entry = &mut self.tabs.entries[index];
        entry.files = Some(files);
        entry.file_observation = Some(observation);
        entry.file_back = Some(back);
    }

    fn workspace_panel(
        &mut self,
        id: Uuid,
        panel: Panel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .tabs
            .entries
            .iter()
            .position(|entry| entry.saved.workspace == id)
        else {
            return;
        };
        if let Some(files) = &self.tabs.entries[index].files
            && files.read(cx).export_pending()
        {
            return;
        }
        window.focus(&self.focus, cx);
        self.tabs.entries[index].saved.panel = panel;
        let workspace = self.tabs.entries[index].workspace.clone();
        if let Some(workspace) = workspace {
            if panel == Panel::Files && self.tabs.entries[index].files.is_none() {
                let saved = &self.tabs.entries[index].saved.files;
                let location = Location {
                    directory: saved.directory.clone(),
                    file: saved.file.clone(),
                };
                let files = cx.new(|cx| FilesView::new(workspace.clone(), location, window, cx));
                self.attach_tab_files(index, files, window, cx);
            }
            if panel == Panel::Terminal && self.tabs.entries[index].terminals.is_none() {
                let selected = self.tabs.entries[index].saved.terminal;
                let view = cx.new(|cx| {
                    let mut view = TerminalView::new(workspace, window, cx);
                    if let Some(slot) = self
                        .slots
                        .iter()
                        .find(|slot| slot.endpoint.socket == view.socket())
                    {
                        view.share_control(slot.control.clone());
                    }
                    view.restore_selection(selected);
                    view
                });
                let observation = cx.observe(&view, |_, _, cx| cx.notify());
                let back = cx.subscribe_in(&view, window, move |this, _, _: &(), window, cx| {
                    this.workspace_panel(id, Panel::Conversation, window, cx)
                });
                let entry = &mut self.tabs.entries[index];
                entry.terminals = Some(view);
                entry.terminal_observation = Some(observation);
                entry.terminal_back = Some(back);
            }
        }
        let entry = &self.tabs.entries[index];
        if panel == Panel::Conversation
            && let Some(view) = &entry.host
        {
            view.update(cx, |view, cx| view.focus(window, cx));
        }
        if panel == Panel::Files
            && let Some(files) = &entry.files
        {
            files.update(cx, |view, cx| view.focus(window, cx));
        }
        if panel == Panel::Terminal
            && let Some(terminal) = &entry.terminals
        {
            terminal.update(cx, |view, cx| view.focus_input(window, cx));
        }
        self.capture_tabs(window, cx);
        cx.notify();
    }

    fn close_workspace_tab(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self
            .tabs
            .entries
            .iter()
            .position(|entry| entry.saved.workspace == id)
        else {
            return;
        };
        let entry = &self.tabs.entries[index];
        let ready = (|| {
            if let Some(host) = &entry.host
                && !host.read(cx).tab_close_status()?
            {
                return Ok(false);
            }
            if let Some(terminal) = &entry.terminals
                && !terminal.read(cx).tab_close_status()?
            {
                return Ok(false);
            }
            Ok::<_, String>(
                entry
                    .files
                    .as_ref()
                    .is_none_or(|files| !files.read(cx).export_pending()),
            )
        })();
        match ready {
            Ok(true) => {}
            Ok(false) => {
                self.route_notice=Some("Tab remains open while local saves or accepted input finish. Unresolved operations must be checked first.".into());
                cx.notify();
                return;
            }
            Err(error) => {
                self.route_notice = Some(format!("Tab remains open: {error}"));
                cx.notify();
                return;
            }
        }
        self.tabs.entries.remove(index);
        if self.tabs.selected == Some(id) {
            self.tabs.selected = self
                .tabs
                .entries
                .get(index.min(self.tabs.entries.len().saturating_sub(1)))
                .map(|entry| entry.saved.workspace);
            if let Some(next) = self.tabs.selected {
                self.select_workspace_tab(next, window, cx);
            } else {
                self.tabs.visible = false;
                self.show_host(self.selected, window, cx);
            }
        }
        self.capture_tabs(window, cx);
        cx.notify();
    }

    pub(super) fn capture_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active = self.tabs_active();
        for slot in &self.slots {
            if let Some(view) = &slot.view {
                view.update(cx, |view, _| view.set_view_visible(!active));
            }
        }
        for entry in &mut self.tabs.entries {
            let visible = active && self.tabs.selected == Some(entry.saved.workspace);
            if let Some(host) = &entry.host {
                host.update(cx, |view, _| {
                    view.set_view_visible(visible && entry.saved.panel == Panel::Conversation)
                });
                let (session, width, shown) = host.read(cx).presentation(cx);
                entry.saved.session = session;
                entry.saved.sidebar_width = width;
                entry.saved.sidebar_visible = shown;
            }
            if let Some(files) = &entry.files {
                let location = files.read(cx).saved_location();
                entry.saved.files.directory = location.directory;
                entry.saved.files.file = location.file;
            }
            if let Some(terminal) = &entry.terminals {
                terminal.update(cx, |view, _| {
                    view.set_visible(visible && entry.saved.panel == Panel::Terminal)
                });
                entry.saved.terminal = terminal.read(cx).saved_selection();
            }
        }
        self.save_tabs(window, cx);
    }

    fn save_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.busy || self.tabs.error.is_some() || self.tabs.saving_paused {
            return;
        }
        let Some(saved) = &self.tabs.saved else {
            return;
        };
        let layout = self.layout();
        if saved.layout == layout {
            return;
        }
        if let Err(error) = layout.validate() {
            self.tabs.error = Some(error.to_string());
            cx.notify();
            return;
        }
        let revision = saved.revision;
        let directory = self
            .creation_directory
            .clone()
            .expect("local controller directory");
        self.tabs.busy = true;
        let task = cx.background_executor().spawn(async move {
            background(async move {
                Store::open(directory)
                    .await?
                    .save_workspace_layout(revision, layout)
                    .await
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.tabs.busy = false;
                match result {
                    Ok(saved) => {
                        this.tabs.saved = Some(saved);
                        this.save_tabs(window, cx);
                    }
                    Err(error) => {
                        this.tabs.error = Some(format!("Workspace layout was not saved: {error}"))
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn tabs_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(cx.theme().border)
            .when_some(self.tabs.error.clone(), |bar, error| {
                bar.child(
                    div()
                        .px_4()
                        .py_1()
                        .flex()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .text_xs()
                                .text_color(cx.theme().danger)
                                .child(error),
                        )
                        .child(
                            Button::new("reload-workspace-layout")
                                .small()
                                .label("Reload saved layout")
                                .disabled(self.tabs.busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.load_tabs(false, window, cx)
                                })),
                        )
                        .child(
                            Button::new("keep-saved-layout")
                                .small()
                                .label("Keep saved layout")
                                .disabled(self.tabs.busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.tabs.saving_paused = true;
                                    this.tabs.error = None;
                                    this.close_error = None;
                                    cx.notify();
                                })),
                        ),
                )
            })
            .when(self.tabs.saving_paused, |bar| {
                bar.child(
                    div()
                        .px_4()
                        .py_1()
                        .flex()
                        .gap_2()
                        .child(div().flex_1().text_xs().child(
                            "Layout saving paused in this window; the saved layout is retained.",
                        ))
                        .child(
                            Button::new("resume-workspace-layout")
                                .small()
                                .label("Reload saved layout")
                                .disabled(self.tabs.busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.load_tabs(false, window, cx)
                                })),
                        ),
                )
            })
            .when(!self.tabs.entries.is_empty(), |bar| {
                bar.child(
                    div()
                        .flex()
                        .min_w_0()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .child(self.tab_menu(cx))
                        .child(
                            div()
                                .id("workspace-tabs")
                                .flex_1()
                                .min_w_0()
                                .px_4()
                                .py_1()
                                .flex()
                                .gap_2()
                                .overflow_x_scroll()
                                .track_scroll(&self.tabs.scroll)
                                .children(self.tabs.entries.iter().enumerate().map(
                                    |(index, entry)| {
                                        let id = entry.saved.workspace;
                                        let name = entry
                                            .saved
                                            .directory
                                            .file_name()
                                            .unwrap_or(entry.saved.directory.as_os_str())
                                            .to_string_lossy();
                                        let host = self
                                            .slots
                                            .iter()
                                            .find(|slot| slot.endpoint.socket == entry.saved.socket)
                                            .map_or("Unavailable Host", |slot| {
                                                slot.endpoint.name.as_str()
                                            });
                                        let project = entry
                                            .workspace
                                            .as_ref()
                                            .and_then(|workspace| {
                                                workspace.registration.project.path.file_name()
                                            })
                                            .map(|name| name.to_string_lossy());
                                        let label = match project {
                                            Some(project) if project != name => {
                                                format!("{host} · {project} / {name}")
                                            }
                                            _ => format!("{host} · {name}"),
                                        };
                                        self.workspace_tab_button(index, id, label, cx)
                                    },
                                )),
                        ),
                )
            })
            .into_any_element()
    }

    pub(super) fn tabs_content(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.tabs_active() {
            return None;
        }
        let entry = self
            .tabs
            .entries
            .iter()
            .find(|entry| Some(entry.saved.workspace) == self.tabs.selected)?;
        let id = entry.saved.workspace;
        let content = match entry.saved.panel {
            Panel::Conversation => entry.host.clone().map(IntoElement::into_any_element),
            Panel::Files => entry.files.clone().map(IntoElement::into_any_element),
            Panel::Terminal => entry.terminals.clone().map(IntoElement::into_any_element),
        };
        Some(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(
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
                                .child(entry.saved.directory.to_string_lossy().into_owned()),
                        )
                        .children(
                            [
                                (Panel::Conversation, "Conversation"),
                                (Panel::Files, "Files"),
                                (Panel::Terminal, "Terminal"),
                            ]
                            .into_iter()
                            .map(|(panel, label)| {
                                Button::new(label)
                                    .small()
                                    .ghost()
                                    .label(label)
                                    .selected(entry.saved.panel == panel)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.workspace_panel(id, panel, window, cx)
                                    }))
                            }),
                        ),
                )
                .when_some(entry.error.clone(), |root, error| {
                    root.child(div().p_4().text_color(cx.theme().danger).child(error))
                })
                .when_some(content, |root, content| {
                    root.child(div().flex_1().min_h_0().child(content))
                })
                .into_any_element(),
        )
    }
}
