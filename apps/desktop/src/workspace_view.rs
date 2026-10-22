//! Existing local directories. Host/provider lifetime and ownership stay separate.
mod names;
mod pins;
mod preferences;
use crate::file_locations::Locations;
use crate::files_view::FilesView;
use crate::host_view::background;
use crate::paseo_import_view::PaseoImportView;
use crate::terminal_view::TerminalView;
use fleetpane_session::{
    creations::Store,
    diffs::{Content, FileDiff, Scope},
    hosts::Endpoint,
    workspaces::{Kind, Overview, Workspace},
    worktrees::{BranchMode, Job, State},
};
use gpui_kit::component::{
    ActiveTheme, Disableable, Selectable, Sizable,
    button::{Button, ButtonVariants},
    diff::{Diff, DiffFile, DiffState},
    input::{Input, InputState},
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::{prelude::*, *};
use std::path::PathBuf;
use uuid::Uuid;

enum Action {
    List(Option<u64>),
    Register(Kind),
    Inspect(Uuid),
    Use(Uuid),
    Plan(Uuid),
    Apply(Uuid),
    Check(Uuid),
    Diff(Uuid, Scope, String),
    Pin(Uuid, bool),
    OrderPins(Vec<Uuid>, Vec<Uuid>),
    ReloadPins,
    Rename(Uuid, u64, Option<String>),
    ReloadName(Uuid),
    RenameProject(Uuid, u64, Option<String>),
}
enum Reply {
    List(Vec<Workspace>, Vec<Job>, Vec<Workspace>),
    Pinned(Vec<Workspace>),
    Registered(Vec<Workspace>, Overview),
    Inspected(Overview),
    Use(Workspace),
    Job(Job, Vec<Job>, Vec<Workspace>),
    Diff(Overview, FileDiff),
    Name(Workspace),
}

pub(crate) struct NamesUpdated(pub Vec<Workspace>);

struct PresentedDiff {
    file: FileDiff,
    state: Option<Entity<DiffState>>,
    _subscription: Option<Subscription>,
}

pub struct WorkspaceView {
    directory: PathBuf,
    hosts: Vec<Endpoint>,
    host: usize,
    input: Entity<InputState>,
    branch: Entity<InputState>,
    branch_mode: BranchMode,
    base: Entity<InputState>,
    name_input: Entity<InputState>,
    name_target: Option<Workspace>,
    name_is_project: bool,
    name_saving: bool,
    name_error: Option<String>,
    pin_saving: bool,
    pin_error: Option<String>,
    records: Vec<Workspace>,
    pins: Vec<Workspace>,
    jobs: Vec<Job>,
    job: Option<Job>,
    overview: Option<Overview>,
    diff: Option<PresentedDiff>,
    scope: Scope,
    busy: bool,
    preferences_epoch: u64,
    error: Option<String>,
    files: Option<Entity<FilesView>>,
    files_subscription: Option<Subscription>,
    file_locations: Locations,
    terminals: Option<Entity<TerminalView>>,
    terminal_subscription: Option<Subscription>,
    migration: Option<Entity<PaseoImportView>>,
    migration_observation: Option<Subscription>,
    migration_continuation: Option<Subscription>,
}

impl EventEmitter<Workspace> for WorkspaceView {}
impl EventEmitter<NamesUpdated> for WorkspaceView {}
impl EventEmitter<crate::paseo_import_view::Continuation> for WorkspaceView {}

impl WorkspaceView {
    pub(crate) fn update_pins(&mut self, pins: Vec<Workspace>) {
        self.preferences_epoch = self.preferences_epoch.wrapping_add(1);
        self.pins = pins;
    }
    pub(crate) fn name_save_failed(&self) -> bool {
        self.name_error.is_some()
    }
    pub(crate) fn pin_save_failed(&self) -> bool {
        self.pin_error.is_some()
    }
    pub(crate) fn shortcut_blocked(&self, window: &mut Window, cx: &mut App) -> bool {
        [&self.input, &self.branch, &self.base, &self.name_input]
            .iter()
            .any(|input| crate::input_composition::active(input, window, cx))
            || self.files.as_ref().is_some_and(|view| {
                view.update(cx, |view, cx| view.tab_shortcut_blocked(window, cx))
            })
            || self.terminals.as_ref().is_some_and(|view| {
                view.update(cx, |view, cx| view.tab_shortcut_blocked(window, cx))
            })
            || self
                .migration
                .as_ref()
                .is_some_and(|view| view.update(cx, |view, cx| view.composing(window, cx)))
    }

    pub(crate) fn export_pending(&self, cx: &App) -> bool {
        self.files
            .as_ref()
            .is_some_and(|view| view.read(cx).export_pending())
    }
    pub(crate) fn close_status(&self, cx: &App) -> Result<bool, String> {
        if let Some(error) = &self.name_error {
            return Err(format!("Name save: {error}"));
        }
        if let Some(error) = &self.pin_error {
            return Err(format!("Workspace pin save: {error}"));
        }
        if self.name_saving || self.pin_saving {
            return Ok(false);
        }
        if self.export_pending(cx) {
            return Ok(false);
        }
        if self
            .migration
            .as_ref()
            .is_some_and(|view| view.read(cx).is_busy())
        {
            return Ok(false);
        }
        self.terminals
            .as_ref()
            .map_or(Ok(true), |view| view.read(cx).close_status())
    }
    pub fn new(
        directory: PathBuf,
        hosts: Vec<Endpoint>,
        host: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Absolute existing directory"));
        let branch = cx.new(|cx| InputState::new(window, cx).placeholder("Local branch name"));
        let name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Custom display name"));
        let base = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Base ref or commit");
            input.set_value("HEAD", window, cx);
            input
        });
        Self {
            directory,
            hosts,
            host,
            input,
            branch,
            branch_mode: BranchMode::New,
            base,
            name_input,
            name_target: None,
            name_is_project: false,
            name_saving: false,
            name_error: None,
            pin_saving: false,
            pin_error: None,
            records: vec![],
            pins: vec![],
            jobs: vec![],
            job: None,
            overview: None,
            diff: None,
            scope: Scope::Unstaged,
            busy: false,
            preferences_epoch: 0,
            error: None,
            files: None,
            files_subscription: None,
            file_locations: Locations::default(),
            terminals: None,
            terminal_subscription: None,
            migration: None,
            migration_observation: None,
            migration_continuation: None,
        }
    }

    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(files) = &self.files {
            files.update(cx, |view, cx| view.focus(window, cx));
            return;
        }
        self.input.update(cx, |input, cx| input.focus(window, cx));
        self.run(Action::List(None), window, cx);
    }

    fn run(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.preferences_epoch = self.preferences_epoch.wrapping_add(1);
        let name_action = matches!(
            action,
            Action::Rename(..) | Action::RenameProject(..) | Action::ReloadName(..)
        );
        let pin_action = matches!(
            action,
            Action::Pin(..) | Action::OrderPins(..) | Action::ReloadPins
        );
        self.name_saving = name_action;
        self.pin_saving = pin_action;
        self.error = None;
        if !matches!(
            action,
            Action::Pin(..)
                | Action::OrderPins(..)
                | Action::ReloadPins
                | Action::Rename(..)
                | Action::RenameProject(..)
                | Action::ReloadName(..)
        ) {
            self.diff = None;
        }
        if !matches!(
            action,
            Action::List(_)
                | Action::Diff(..)
                | Action::Pin(..)
                | Action::OrderPins(..)
                | Action::ReloadPins
                | Action::Rename(..)
                | Action::RenameProject(..)
                | Action::ReloadName(..)
        ) {
            self.overview = None;
            self.job = None;
        }
        let directory = self.directory.clone();
        let endpoint = self.hosts[self.host].clone();
        let path = PathBuf::from(self.input.read(cx).value().as_str());
        let branch = self.branch.read(cx).value().to_string();
        let branch_mode = self.branch_mode;
        let base = self.base.read(cx).value().to_string();
        let task = cx.background_executor().spawn(async move {
            background(async move {
                let store = Store::open(directory).await?;
                match action {
                    Action::List(before) => Ok(Reply::List(
                        store.workspaces(before).await?,
                        store.worktree_jobs(None).await?,
                        store.pinned_workspaces().await?,
                    )),
                    Action::Pin(id, pinned) => {
                        Ok(Reply::Pinned(store.set_workspace_pinned(id, pinned).await?))
                    }
                    Action::OrderPins(expected, order) => Ok(Reply::Pinned(
                        store.reorder_workspace_pins(expected, order).await?,
                    )),
                    Action::ReloadPins => Ok(Reply::Pinned(store.pinned_workspaces().await?)),
                    Action::Rename(id, revision, name) => Ok(Reply::Name(
                        store.rename_workspace(id, revision, name).await?,
                    )),
                    Action::ReloadName(id) => Ok(Reply::Name(store.workspace(id).await?)),
                    Action::RenameProject(id, revision, name) => {
                        let mut workspace = store.workspace(id).await?;
                        workspace.project_name = store
                            .rename_project(workspace.project_id, revision, name)
                            .await?;
                        Ok(Reply::Name(workspace))
                    }
                    Action::Register(kind) => {
                        let workspace = store.register_workspace(endpoint, path, kind).await?;
                        let overview = workspace.inspect().await?;
                        Ok(Reply::Registered(store.workspaces(None).await?, overview))
                    }
                    Action::Inspect(id) => Ok(Reply::Inspected(
                        store.workspace(id).await?.inspect().await?,
                    )),
                    Action::Use(id) => {
                        let workspace = store.workspace(id).await?;
                        workspace.inspect().await?;
                        Ok(Reply::Use(workspace))
                    }
                    Action::Plan(source) => {
                        let id = Uuid::new_v4();
                        let job = match branch_mode {
                            BranchMode::New => {
                                store.plan_worktree(id, source, branch, base).await?
                            }
                            BranchMode::Existing => {
                                store.plan_existing_worktree(id, source, branch).await?
                            }
                        };
                        Ok(Reply::Job(
                            job,
                            store.worktree_jobs(None).await?,
                            store.workspaces(None).await?,
                        ))
                    }
                    Action::Apply(id) => {
                        let job = store.apply_worktree(id).await?;
                        Ok(Reply::Job(
                            job,
                            store.worktree_jobs(None).await?,
                            store.workspaces(None).await?,
                        ))
                    }
                    Action::Diff(id, scope, path) => {
                        let workspace = store.workspace(id).await?;
                        let file = workspace.diff(scope, &path).await?;
                        Ok(Reply::Diff(workspace.inspect().await?, file))
                    }
                    Action::Check(id) => {
                        let job = store.worktree_job(id).await?;
                        Ok(Reply::Job(
                            job,
                            store.worktree_jobs(None).await?,
                            store.workspaces(None).await?,
                        ))
                    }
                }
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let reply = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                this.name_saving = false;
                this.pin_saving = false;
                match reply {
                    Ok(Reply::List(records,jobs,pins)) => { cx.emit(NamesUpdated(records.iter().chain(&pins).cloned().collect())); this.records = records; this.jobs=jobs; this.pins=pins; }
                    Ok(Reply::Pinned(pins)) => {
                        this.pin_error=None;
                        cx.emit(NamesUpdated(pins.clone()));
                        this.pins=pins;
                    },
                    Ok(Reply::Registered(records, overview)) => {
                        this.records = records;
                        this.overview = Some(overview);
                    }
                    Ok(Reply::Inspected(overview)) => {
                        cx.emit(NamesUpdated(vec![overview.workspace.clone()]));
                        this.overview = Some(overview);
                    }
                    Ok(Reply::Name(workspace)) => {
                        for record in this.records.iter_mut().chain(&mut this.pins) {
                            record.update_names_from(&workspace);
                        }
                        if let Some(overview) = &mut this.overview {
                            overview.workspace.update_names_from(&workspace);
                        }
                        this.name_target=Some(workspace.clone());
                        this.set_name_input(&workspace, window, cx);
                        this.name_error = None;
                        cx.emit(NamesUpdated(vec![workspace]));
                    }
                    Ok(Reply::Job(job,jobs,records)) => { this.job=Some(job); this.jobs=jobs; this.records=records; }
                    Ok(Reply::Diff(overview, file)) => {
                        let state = match &file.content {
                            Content::Patch { text } => match DiffFile::parse(text) {
                                Ok(files) => Some(cx.new(|cx| DiffState::new(files, cx))),
                                Err(error) => { this.error = Some(format!("Cannot render this patch: {error}. Copy patch to inspect the raw Git output.")); None }
                            },
                            _ => None,
                        };
                        let subscription = state.as_ref().map(|state|cx.observe(state,|_,_,cx|cx.notify()));
                        this.overview = Some(overview);
                        this.diff = Some(PresentedDiff { file, state, _subscription: subscription });
                    }
                    Ok(Reply::Use(workspace)) => {
                        if this.hosts.iter().any(|host| {
                            host.socket == workspace.registration.endpoint.socket
                        }) {
                            cx.emit(workspace);
                        } else {
                            this.error = Some(format!(
                                "Saved Host {} is absent from this configuration. Restore its endpoint to prepare this workspace.",
                                workspace.registration.endpoint.name
                            ));
                        }
                    }
                    Err(error) if name_action => this.name_error = Some(error.to_string()),
                    Err(error) if pin_action => this.pin_error = Some(error.to_string()),
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn set_name_input(&self, workspace: &Workspace, window: &mut Window, cx: &mut Context<Self>) {
        let value = if self.name_is_project {
            &workspace.project_name
        } else {
            &workspace.name
        }
        .value
        .clone()
        .unwrap_or_default();
        self.name_input
            .update(cx, |input, cx| input.set_value(value, window, cx));
    }
}

impl WorkspaceView {
    fn open_terminals(
        &mut self,
        workspace: Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.new(|cx| TerminalView::new(workspace, window, cx));
        self.terminal_subscription = Some(cx.subscribe_in(&view, window, |this, _, _, _, cx| {
            this.terminals = None;
            this.terminal_subscription = None;
            cx.notify();
        }));
        self.terminals = Some(view);
        cx.notify();
    }

    fn open_files(&mut self, workspace: Workspace, window: &mut Window, cx: &mut Context<Self>) {
        let location = self.file_locations.get(&workspace);
        let view = cx.new(|cx| FilesView::new(workspace.clone(), location, window, cx));
        self.files_subscription =
            Some(
                cx.subscribe_in(&view, window, move |this, _, location, window, cx| {
                    this.file_locations.save(&workspace, location.clone());
                    this.files = None;
                    this.files_subscription = None;
                    this.input.update(cx, |input, cx| input.focus(window, cx));
                    cx.notify();
                }),
            );
        self.files = Some(view);
        cx.notify();
    }

    fn grouped_workspaces(&self, cx: &Context<Self>) -> Vec<Div> {
        let mut groups: Vec<Vec<(usize, &Workspace)>> = Vec::new();
        for (index, workspace) in self.records.iter().enumerate() {
            if let Some(group) = groups
                .iter_mut()
                .find(|group| group[0].1.project_id == workspace.project_id)
            {
                group.push((index, workspace));
            } else {
                groups.push(vec![(index, workspace)]);
            }
        }
        groups
            .into_iter()
            .map(|group| {
                let workspace = group[0].1;
                let project = workspace
                    .registration
                    .project
                    .path
                    .to_string_lossy()
                    .into_owned();
                div()
                    .mt_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .truncate()
                                    .child(format!(
                                        "{} · {}",
                                        workspace.registration.endpoint.name,
                                        workspace.project_display_name()
                                    )),
                            )
                            .child(
                                Button::new(SharedString::from(format!(
                                    "rename-project-{}",
                                    workspace.project_id
                                )))
                                .small()
                                .label("Rename project")
                                .disabled(self.busy || self.name_error.is_some())
                                .on_click(cx.listener({
                                    let workspace = workspace.clone();
                                    move |this, _, window, cx| {
                                        this.name_is_project = true;
                                        this.name_target = Some(workspace.clone());
                                        this.set_name_input(&workspace, window, cx);
                                        this.name_input
                                            .update(cx, |input, cx| input.focus(window, cx));
                                        cx.notify();
                                    }
                                })),
                            ),
                    )
                    .child(div().text_xs().truncate().child(project))
                    .children(
                        group.into_iter().map(|(index, workspace)| {
                            self.workspace_row(workspace, index, false, cx)
                        }),
                    )
            })
            .collect()
    }

    fn workspace_row(
        &self,
        workspace: &Workspace,
        index: usize,
        shortcut: bool,
        cx: &Context<Self>,
    ) -> Div {
        let id = workspace.id;
        let pinned = self.pins.iter().any(|workspace| workspace.id == id);
        let path = workspace.registration.directory.path.to_string_lossy();
        let name = workspace
            .registration
            .directory
            .path
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_else(|| path.clone());
        let branch = self.jobs.iter().find_map(|job| match job.state {
            State::Completed { workspace_id } if workspace_id == id => {
                Some(job.plan.branch.as_str())
            }
            _ => None,
        });
        div()
            .py_2()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div().flex_1().min_w_0().child(
                            Button::new((
                                if shortcut {
                                    "inspect-pinned"
                                } else {
                                    "inspect-workspace"
                                },
                                index,
                            ))
                            .small()
                            .ghost()
                            .w_full()
                            .selected(
                                self.overview
                                    .as_ref()
                                    .is_some_and(|overview| overview.workspace.id == id),
                            )
                            .label(format!(
                                "{} · {}",
                                workspace.registration.endpoint.name,
                                workspace
                                    .name
                                    .value
                                    .as_deref()
                                    .or(branch)
                                    .unwrap_or(name.as_ref())
                            ))
                            .disabled(self.busy)
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.run(Action::Inspect(id), window, cx)
                                },
                            )),
                        ),
                    )
                    .child(
                        Button::new((
                            if shortcut {
                                "rename-pinned"
                            } else {
                                "rename-workspace"
                            },
                            index,
                        ))
                        .small()
                        .label("Rename")
                        .disabled(self.busy || self.name_error.is_some())
                        .on_click(cx.listener({
                            let workspace = workspace.clone();
                            move |this, _, window, cx| {
                                this.name_is_project = false;
                                this.name_target = Some(workspace.clone());
                                this.set_name_input(&workspace, window, cx);
                                this.name_input
                                    .update(cx, |input, cx| input.focus(window, cx));
                                cx.notify();
                            }
                        })),
                    )
                    .child(
                        Button::new((
                            if shortcut {
                                "unpin-shortcut"
                            } else {
                                "pin-workspace"
                            },
                            index,
                        ))
                        .small()
                        .label(if pinned { "Unpin" } else { "Pin" })
                        .disabled(self.busy || self.pin_error.is_some())
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.run(Action::Pin(id, !pinned), window, cx)
                            },
                        )),
                    ),
            )
            .child(div().text_xs().truncate().child(path.into_owned()))
            .when(shortcut, |view| {
                view.child(self.pin_controls(workspace, index, cx))
            })
            .when(shortcut, |view| {
                view.child(
                    Button::new(("open-pinned-workspace", index))
                        .small()
                        .label("Use workspace")
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.run(Action::Use(id), window, cx)
                        })),
                )
            })
    }
}

impl Render for WorkspaceView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(migration) = &self.migration {
            let busy = migration.read(cx).is_busy();
            return div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    Button::new("leave-paseo-import")
                        .small()
                        .label("Back to workspaces")
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.migration = None;
                            this.migration_observation = None;
                            this.run(Action::List(None), window, cx);
                        })),
                )
                .child(div().flex_1().min_h_0().child(migration.clone()));
        }
        if let Some(files) = &self.files {
            return div().size_full().child(files.clone());
        }
        if let Some(terminals) = &self.terminals {
            return div().size_full().child(terminals.clone());
        }
        let host_names: Vec<String> = self.hosts.iter().map(|host| host.name.clone()).collect();
        let owner = cx.entity().downgrade();
        let selected = self.host;
        div().size_full().flex().flex_col().gap_3().p_4()
            .child(div().text_lg().child("Projects & workspaces"))
            .child(Button::new("open-paseo-import").small().label("Import from Paseo").disabled(self.busy).on_click(cx.listener(|this,_,window,cx| {
                let view=cx.new(|cx|PaseoImportView::new(this.directory.clone(),this.hosts[this.host].clone(),window,cx));
                this.migration_observation=Some(cx.observe(&view,|_,_,cx|cx.notify()));
                this.migration_continuation=Some(cx.subscribe(&view,|_,_,event:&crate::paseo_import_view::Continuation,cx|cx.emit(event.clone())));
                this.migration=Some(view);cx.notify();
            })))
            .child(div().text_sm().child("Register an existing Git workspace or ordinary folder. Registration and Refresh do not create a worktree or start an agent."))
            .child(div().flex().items_center().gap_2()
                .child(Button::new("workspace-host").small().label(self.hosts[self.host].name.clone()).disabled(self.busy).dropdown_menu_with_anchor(Anchor::BottomLeft,move |mut menu,_,_| {
                    for (index,name) in host_names.iter().enumerate() {
                        let owner=owner.clone();
                        menu=menu.item(PopupMenuItem::new(name.clone()).checked(index==selected).on_click(move |_,_,cx| {
                            if let Some(entity)=owner.upgrade() { entity.update(cx,|this,cx| { if !this.busy { this.host=index; cx.notify(); } }); }
                        }));
                    }
                    menu
                }))
                .child(div().flex_1().min_w_0().child(Input::new(&self.input).disabled(self.busy)))
                .child(Button::new("register-git-workspace").small().label("Save Git workspace").disabled(self.busy).on_click(cx.listener(|this,_,window,cx| this.run(Action::Register(Kind::Git),window,cx))))
                .child(Button::new("register-folder-workspace").small().label("Save folder").disabled(self.busy).on_click(cx.listener(|this,_,window,cx| this.run(Action::Register(Kind::Folder),window,cx))))
            )
            .when(self.busy,|view|view.child(div().text_sm().child("Working…")))
            .children(self.error.iter().map(|error|div().text_sm().text_color(cx.theme().danger).child(error.clone())))
            .child(self.pin_status(cx))
            .child(div().flex_1().min_h_0().flex().gap_4()
                .child(div().w_80().flex_shrink_0().flex().flex_col().gap_2()
                    .child(div().flex().gap_2()
                        .child(Button::new("newest-workspaces").small().label("Newest").disabled(self.busy).on_click(cx.listener(|this,_,window,cx|this.run(Action::List(None),window,cx))))
                        .when(self.records.len()==32,|view|view.child(Button::new("older-workspaces").small().label("Older").disabled(self.busy).on_click(cx.listener(|this,_,window,cx| { let before=this.records.last().map(|workspace|workspace.position); this.run(Action::List(before),window,cx); }))))
                    )
                    .child(div().id("workspace-records").flex_1().min_h_0().overflow_y_scroll()
                        .when(!self.pins.is_empty(), |view| view
                            .child(div().text_sm().child(format!("Pinned · {} / 32", self.pins.len())))
                            .children(self.pins.iter().enumerate().map(|(index, workspace)| self.pinned_row(workspace, index, cx)))
                            .child(div().mt_3().text_sm().child("Saved workspaces")))
                        .when(self.records.is_empty(),|view|view.child("No saved workspaces."))
                        .children(self.grouped_workspaces(cx))
                        .when(!self.jobs.is_empty(),|view|view.child(div().mt_3().text_sm().child("Recent worktree creations (up to 32)")))
                        .children(self.jobs.iter().enumerate().map(|(index,job)| {
                            let id=job.plan.id;
                            Button::new(("worktree-job",index)).small().ghost().label(format!("{} · {}",job.plan.branch,match job.state { State::Prepared=>"Prepared",State::Attempted=>"Attempted",State::Completed{..}=>"Completed",State::Failed{..}=>"Failed" })).disabled(self.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Check(id),window,cx)))
                        }))
                    )
                )
                .child(div().flex_1().min_w_0().min_h_0().flex().flex_col().gap_2()
                    .when(self.name_target.is_some(),|view|view.child(div().id("workspace-name-editor").flex_1().min_h_0().overflow_y_scroll().child(self.name_editor(cx))))
                    .when(self.name_target.is_none()&&self.overview.is_none()&&self.job.is_none(),|view|view.child("Select a saved workspace to inspect its current directory."))
                    .when_some(self.job.as_ref().filter(|_|self.name_target.is_none()),|view,job| {
                        let id=job.plan.id;
                        let copy_path=job.plan.path.to_string_lossy().into_owned();
                        view.child(div().text_sm().child(format!("Worktree creation {id}")))
                            .child(div().text_sm().child(format!("Host: {} · {} branch: {}",job.plan.source.endpoint.name,if job.plan.branch_mode==BranchMode::New { "new" } else { "existing local" },job.plan.branch)))
                            .child(div().text_sm().child(format!("{}: {} → {}",if job.plan.branch_mode==BranchMode::New { "Base" } else { "Expected tip" },job.plan.base_ref,job.plan.commit)))
                            .child(div().text_sm().truncate().child(format!("Worktree: {}",job.plan.path.display())))
                            .child(div().text_sm().truncate().child(format!("Session directory: {}",job.plan.cwd.display())))
                            .child(Button::new("copy-worktree-path").small().label("Copy worktree path").on_click(move|_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(copy_path.clone()))))
                            .child(Button::new("check-worktree-job").small().label("Check saved status").disabled(self.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Check(id),window,cx))))
                            .when(job.state==State::Prepared,|view|view
                                .child(if job.plan.branch_mode==BranchMode::New { "Ready to create this directory and branch at the pinned commit. No setup script or session will start." } else { "Ready to check out this local branch. Its tip and availability must still match this preview. No setup script or session will start." })
                                .child(Button::new("apply-worktree-plan").small().label("Create this worktree").disabled(self.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Apply(id),window,cx)))))
                            .when(job.state==State::Attempted,|view|view.child("Attempt recorded; outcome is incomplete or still running. Inspect the directory and branch with Git. This attempt cannot be resent."))
                            .when_some(match &job.state { State::Failed{error}=>Some(error),_=>None },|view,error|view.child(div().text_color(cx.theme().danger).child(error.clone())))
                            .when_some(match job.state { State::Completed{workspace_id}=>Some(workspace_id),_=>None },|view,workspace_id|view
                                .child("Completed and registered. Open the workspace to prepare a session.")
                                .child(Button::new("open-created-worktree").small().label("Open workspace").disabled(self.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Inspect(workspace_id),window,cx)))))
                    })
                    .when_some(self.overview.as_ref().filter(|_| self.diff.is_none() && self.name_target.is_none()),|view,overview| {
                        let id=overview.workspace.id;
                        let workspace=overview.workspace.clone();
                        let registration=&overview.workspace.registration;
                        let copy_path=registration.directory.path.to_string_lossy().into_owned();
                        let terminal_workspace=workspace.clone();
                        view.child(Button::new("browse-workspace-files").small().label("Browse files").disabled(self.busy).on_click(cx.listener(move|this,_,window,cx|this.open_files(workspace.clone(),window,cx))))
                            .child(Button::new("open-workspace-terminals").small().label("Terminals").disabled(self.busy).on_click(cx.listener(move|this,_,window,cx|this.open_terminals(terminal_workspace.clone(),window,cx))))
                            .child(div().text_sm().child(format!("Workspace {}",id)))
                            .child(div().text_sm().child(format!("Project {}",overview.workspace.project_id)))
                            .child(div().w_full().min_w_0().text_sm().truncate().child(registration.directory.path.to_string_lossy().into_owned()))
                            .child(div().flex().gap_2()
                                .child(Button::new("refresh-workspace").small().label("Refresh").disabled(self.busy).on_click(cx.listener(move |this,_,window,cx|this.run(Action::Inspect(id),window,cx))))
                                .child(Button::new("use-workspace").small().label("Open workspace").disabled(self.busy).on_click(cx.listener(move |this,_,window,cx|this.run(Action::Use(id),window,cx))))
                                .child(Button::new("copy-workspace-path").small().label("Copy path").on_click(move|_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(copy_path.clone()))))
                            )
                            .when(overview.git.is_none(),|view|view.child("Ordinary folder; Git inspection was not requested."))
                            .when_some(overview.git.as_ref(),|view,git|view
                                .child(div().text_sm().child("Create an independent worktree from this workspace"))
                                .child(div().flex().gap_2()
                                    .child(Button::new("worktree-new-branch").small().label("New branch").selected(self.branch_mode==BranchMode::New).disabled(self.busy).on_click(cx.listener(|this,_,_,cx| { this.branch_mode=BranchMode::New; cx.notify(); })))
                                    .child(Button::new("worktree-existing-branch").small().label("Existing local branch").selected(self.branch_mode==BranchMode::Existing).disabled(self.busy).on_click(cx.listener(|this,_,_,cx| { this.branch_mode=BranchMode::Existing; cx.notify(); }))))
                                .child(div().flex().gap_2()
                                    .child(div().flex_1().min_w_0().child(div().text_xs().child(if self.branch_mode==BranchMode::New { "New branch" } else { "Existing local branch" })).child(Input::new(&self.branch).disabled(self.busy)))
                                    .when(self.branch_mode==BranchMode::New,|view|view.child(div().flex_1().min_w_0().child(div().text_xs().child("Base ref or commit")).child(Input::new(&self.base).disabled(self.busy))))
                                    .child(Button::new("plan-worktree").small().label("Preview worktree").disabled(self.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Plan(id),window,cx)))))
                                .child(div().text_sm().child(format!("{} · HEAD {}",git.branch.as_deref().unwrap_or("Detached HEAD"),git.head.as_deref().unwrap_or("unborn"))))
                                .child(div().text_sm().child(format!("{} · {} changed paths",if git.linked_worktree { "Linked worktree" } else { "Main checkout" },git.changes.len())))
                                .child(div().w_full().min_w_0().text_xs().truncate().child(format!("Repository: {}",git.common_directory.display())))
                                .child(div().w_full().min_w_0().text_xs().truncate().child(format!("Worktree root: {}",git.root.display())))
                                .child(div().flex().gap_2()
                                    .child(Button::new("unstaged-diff").small().label("Unstaged").selected(self.scope==Scope::Unstaged).disabled(self.busy).on_click(cx.listener(|this,_,_,cx| { this.scope=Scope::Unstaged; cx.notify(); })))
                                    .child(Button::new("staged-diff").small().label("Staged").selected(self.scope==Scope::Staged).disabled(self.busy).on_click(cx.listener(|this,_,_,cx| { this.scope=Scope::Staged; cx.notify(); })))
                                )
                                .child(div().text_xs().child("Choose a changed file to compare. Paths are relative to the worktree root."))
                                .child(uniform_list("workspace-changes",git.changes.len(),cx.processor(|this,range: std::ops::Range<usize>,_,cx| {
                                    let git=this.overview.as_ref().and_then(|overview|overview.git.as_ref()).expect("Git changes list");
                                    let id=this.overview.as_ref().expect("selected workspace").workspace.id;
                                    let scope=this.scope;
                                    let busy=this.busy;
                                    range.map(|index| { let change=&git.changes[index];
                                        let selected_path=change.path.clone();
                                        let path=change.path.escape_debug().to_string();
                                        let label=match &change.original_path { Some(original)=>format!("{}  {} → {path}",change.status,original.escape_debug()),None=>format!("{}  {path}",change.status) };
                                        div().id(("changed-file",index)).h_8().px_2().flex().items_center().text_sm().overflow_hidden().cursor_pointer().child(label)
                                            .on_click(cx.listener(move|this,_,window,cx| { if !busy { this.run(Action::Diff(id,scope,selected_path.clone()),window,cx); } }))
                                    }).collect::<Vec<_>>()
                                })).flex_1().min_h_0())
                            )
                    })
                    .when_some(self.diff.as_ref(),|view,presented| {
                        let file=&presented.file;
                        let id=file.workspace_id;
                        let scope=file.scope;
                        let path=file.change.path.clone();
                        let patch=match &file.content { Content::Patch{text}=>Some(text.clone()),Content::Binary{summary}=>Some(summary.clone()),_=>None };
                        view.child(div().text_sm().truncate().child(file.change.path.escape_debug().to_string()))
                            .child(div().text_sm().child(scope.label()))
                            .child(div().text_xs().truncate().child(format!("Observed HEAD: {}",file.head.as_deref().unwrap_or("unborn (empty tree)"))))
                            .child(div().flex().gap_2()
                                .child(Button::new("back-to-changes").small().label("Back to changes").on_click(cx.listener(|this,_,_,cx| { this.diff=None; this.error=None; cx.notify(); })))
                                .child(Button::new("refresh-diff").small().label("Refresh diff").disabled(self.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Diff(id,scope,path.clone()),window,cx))))
                                .when_some(patch,|view,patch|view.child(Button::new("copy-patch").small().label("Copy patch").on_click(move|_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(patch.clone())))))
                            )
                            .when_some(presented.state.as_ref(),|view,state|view.child(Diff::new(state).w_full().flex_1().min_h_0().soft_wrap(true)))
                            .when(matches!(file.content,Content::Empty),|view|view.child("No changes in this comparison. Choose the other scope or refresh the changed-file list."))
                            .when(matches!(file.content,Content::Binary{..}),|view|view.child("Binary file changed. Text rendering is unavailable; Copy patch includes Git metadata."))
                            .when_some(match &file.content { Content::Unavailable{reason}=>Some(reason),_=>None },|view,reason|view.child(reason.clone()))
                    })
                )
            )
    }
}
