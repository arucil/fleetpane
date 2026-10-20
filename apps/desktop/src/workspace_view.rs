//! Existing local directories. Host/provider lifetime and ownership stay separate.
use crate::host_view::background;
use fleetpane_session::{
    creations::Store,
    diffs::{Content, FileDiff, Scope},
    hosts::Endpoint,
    workspaces::{Kind, Overview, Workspace},
    worktrees::{Job, State},
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
}
enum Reply {
    List(Vec<Workspace>, Vec<Job>),
    Registered(Vec<Workspace>, Overview),
    Inspected(Overview),
    Use(Workspace),
    Job(Job, Vec<Job>, Vec<Workspace>),
    Diff(Overview, FileDiff),
}

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
    base: Entity<InputState>,
    records: Vec<Workspace>,
    jobs: Vec<Job>,
    job: Option<Job>,
    overview: Option<Overview>,
    diff: Option<PresentedDiff>,
    scope: Scope,
    busy: bool,
    error: Option<String>,
}

impl EventEmitter<Workspace> for WorkspaceView {}

impl WorkspaceView {
    pub fn new(
        directory: PathBuf,
        hosts: Vec<Endpoint>,
        host: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Absolute existing directory"));
        let branch = cx.new(|cx| InputState::new(window, cx).placeholder("New branch name"));
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
            base,
            records: vec![],
            jobs: vec![],
            job: None,
            overview: None,
            diff: None,
            scope: Scope::Unstaged,
            busy: false,
            error: None,
        }
    }

    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
        self.run(Action::List(None), window, cx);
    }

    fn run(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        self.diff = None;
        if !matches!(action, Action::List(_) | Action::Diff(..)) {
            self.overview = None;
            self.job = None;
        }
        let directory = self.directory.clone();
        let endpoint = self.hosts[self.host].clone();
        let path = PathBuf::from(self.input.read(cx).value().as_str());
        let branch = self.branch.read(cx).value().to_string();
        let base = self.base.read(cx).value().to_string();
        let task = cx.background_executor().spawn(async move {
            background(async move {
                let store = Store::open(directory).await?;
                match action {
                    Action::List(before) => Ok(Reply::List(
                        store.workspaces(before).await?,
                        store.worktree_jobs(None).await?,
                    )),
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
                        let job = store
                            .plan_worktree(Uuid::new_v4(), source, branch, base)
                            .await?;
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
            let _closed = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                match reply {
                    Ok(Reply::List(records,jobs)) => { this.records = records; this.jobs=jobs; }
                    Ok(Reply::Registered(records, overview)) => {
                        this.records = records;
                        this.overview = Some(overview);
                    }
                    Ok(Reply::Inspected(overview)) => this.overview = Some(overview),
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
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for WorkspaceView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let host_names: Vec<String> = self.hosts.iter().map(|host| host.name.clone()).collect();
        let owner = cx.entity().downgrade();
        let selected = self.host;
        div().size_full().flex().flex_col().gap_3().p_4()
            .child(div().text_lg().child("Projects & workspaces"))
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
            .child(div().flex_1().min_h_0().flex().gap_4()
                .child(div().w_80().flex_shrink_0().flex().flex_col().gap_2()
                    .child(div().flex().gap_2()
                        .child(Button::new("newest-workspaces").small().label("Newest").disabled(self.busy).on_click(cx.listener(|this,_,window,cx|this.run(Action::List(None),window,cx))))
                        .when(self.records.len()==32,|view|view.child(Button::new("older-workspaces").small().label("Older").disabled(self.busy).on_click(cx.listener(|this,_,window,cx| { let before=this.records.last().map(|workspace|workspace.position); this.run(Action::List(before),window,cx); }))))
                    )
                    .child(div().id("workspace-records").flex_1().min_h_0().overflow_y_scroll()
                        .when(self.records.is_empty(),|view|view.child("No saved workspaces."))
                        .children(self.records.iter().enumerate().map(|(index, workspace)| {
                            let id=workspace.id;
                            let path=workspace.registration.directory.path.to_string_lossy();
                            let name=workspace.registration.directory.path.file_name().map(|name|name.to_string_lossy()).unwrap_or_else(||path.clone());
                            let created_branch=self.jobs.iter().find_map(|job|match job.state { State::Completed{workspace_id} if workspace_id==id=>Some(job.plan.branch.as_str()),_=>None });
                            div().py_2().flex().flex_col().gap_1()
                                .child(Button::new(("inspect-workspace",index)).small().ghost().selected(self.overview.as_ref().is_some_and(|overview|overview.workspace.id==id)).label(format!("{} · {}",workspace.registration.endpoint.name,created_branch.unwrap_or(name.as_ref()))).disabled(self.busy).on_click(cx.listener(move |this,_,window,cx|this.run(Action::Inspect(id),window,cx))))
                                .child(div().text_xs().truncate().child(path.into_owned()))
                        }))
                        .when(!self.jobs.is_empty(),|view|view.child(div().mt_3().text_sm().child("Recent worktree creations (up to 32)")))
                        .children(self.jobs.iter().enumerate().map(|(index,job)| {
                            let id=job.plan.id;
                            Button::new(("worktree-job",index)).small().ghost().label(format!("{} · {}",job.plan.branch,match job.state { State::Prepared=>"Prepared",State::Attempted=>"Attempted",State::Completed{..}=>"Completed",State::Failed{..}=>"Failed" })).disabled(self.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Check(id),window,cx)))
                        }))
                    )
                )
                .child(div().flex_1().min_w_0().flex().flex_col().gap_2()
                    .when(self.overview.is_none()&&self.job.is_none(),|view|view.child("Select a saved workspace to inspect its current directory."))
                    .when_some(self.job.as_ref(),|view,job| {
                        let id=job.plan.id;
                        let copy_path=job.plan.path.to_string_lossy().into_owned();
                        view.child(div().text_sm().child(format!("Worktree creation {id}")))
                            .child(div().text_sm().child(format!("Host: {} · new branch: {}",job.plan.source.endpoint.name,job.plan.branch)))
                            .child(div().text_sm().child(format!("Base: {} → {}",job.plan.base_ref,job.plan.commit)))
                            .child(div().text_sm().truncate().child(format!("Worktree: {}",job.plan.path.display())))
                            .child(div().text_sm().truncate().child(format!("Session directory: {}",job.plan.cwd.display())))
                            .child(Button::new("copy-worktree-path").small().label("Copy worktree path").on_click(move|_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(copy_path.clone()))))
                            .child(Button::new("check-worktree-job").small().label("Check saved status").disabled(self.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Check(id),window,cx))))
                            .when(job.state==State::Prepared,|view|view
                                .child("Ready to create this directory and branch at the pinned commit. No setup script or session will start.")
                                .child(Button::new("apply-worktree-plan").small().label("Create this worktree").disabled(self.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Apply(id),window,cx)))))
                            .when(job.state==State::Attempted,|view|view.child("Attempt recorded; outcome is incomplete or still running. Inspect the directory and branch with Git. This attempt cannot be resent."))
                            .when_some(match &job.state { State::Failed{error}=>Some(error),_=>None },|view,error|view.child(div().text_color(cx.theme().danger).child(error.clone())))
                            .when_some(match job.state { State::Completed{workspace_id}=>Some(workspace_id),_=>None },|view,workspace_id|view
                                .child("Completed and registered. Open the workspace to prepare a session.")
                                .child(Button::new("open-created-worktree").small().label("Open workspace").disabled(self.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Inspect(workspace_id),window,cx)))))
                    })
                    .when_some(self.overview.as_ref().filter(|_| self.diff.is_none()),|view,overview| {
                        let id=overview.workspace.id;
                        let registration=&overview.workspace.registration;
                        let copy_path=registration.directory.path.to_string_lossy().into_owned();
                        view.child(div().text_sm().child(format!("Workspace {}",id)))
                            .child(div().text_sm().child(format!("Project {}",overview.workspace.project_id)))
                            .child(div().w_full().min_w_0().text_sm().truncate().child(registration.directory.path.to_string_lossy().into_owned()))
                            .child(div().flex().gap_2()
                                .child(Button::new("refresh-workspace").small().label("Refresh").disabled(self.busy).on_click(cx.listener(move |this,_,window,cx|this.run(Action::Inspect(id),window,cx))))
                                .child(Button::new("use-workspace").small().label(format!("Prepare on {}",registration.endpoint.name)).disabled(self.busy).on_click(cx.listener(move |this,_,window,cx|this.run(Action::Use(id),window,cx))))
                                .child(Button::new("copy-workspace-path").small().label("Copy path").on_click(move|_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(copy_path.clone()))))
                            )
                            .when(overview.git.is_none(),|view|view.child("Ordinary folder; Git inspection was not requested."))
                            .when_some(overview.git.as_ref(),|view,git|view
                                .child(div().text_sm().child("Create an independent worktree from this workspace"))
                                .child(div().flex().gap_2()
                                    .child(div().flex_1().min_w_0().child(div().text_xs().child("New branch")).child(Input::new(&self.branch).disabled(self.busy)))
                                    .child(div().flex_1().min_w_0().child(div().text_xs().child("Base ref or commit")).child(Input::new(&self.base).disabled(self.busy)))
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
