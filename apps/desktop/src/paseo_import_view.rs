//! Explicit local import and independently durable history browsing.
use crate::{
    conversation_item::{ItemView, Row},
    host_view::background,
};
use fleetpane_session::{
    client::Error,
    creations::Store,
    hosts::Endpoint,
    paseo_import::{
        self, BatchControl, BatchPage, BatchPlan, BatchProgress, BatchReport, BatchSummary,
        Catalog, HistoryPage, ImportedWorkspace, MigrationReport, Roots, ScratchReport, Session,
    },
};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
};
use gpui_kit::{prelude::*, *};
use std::{collections::BTreeSet, path::PathBuf, sync::Arc, time::Duration};
use uuid::Uuid;
mod batches;
mod scratch;

enum Action {
    Continue(Uuid),
    Preview(PathBuf),
    Import(Arc<Catalog>, String),
    Saved,
    Sessions(Uuid, Option<u64>),
    CopyWorkspace(Uuid, Roots),
    CopySession(Uuid, Roots),
    History(Uuid, u64),
    Batch(BatchPlan, BatchControl),
    Batches(Option<u64>),
    BatchResults(Uuid, Option<u64>),
    RetryBatch(Uuid, BatchControl),
    Scratch(bool),
}
enum Reply {
    Continuation(Continuation),
    Catalog(Catalog),
    Imported(Box<ImportedWorkspace>, Vec<ImportedWorkspace>),
    Saved(Vec<ImportedWorkspace>, Vec<BatchSummary>),
    Sessions(Uuid, Vec<Session>),
    Report(MigrationReport),
    History(Box<HistoryPage>),
    Batch(Box<BatchReport>, Vec<ImportedWorkspace>, Vec<BatchSummary>),
    Batches(Vec<BatchSummary>),
    BatchResults(Box<BatchPage>),
    Scratch(ScratchReport),
}

#[derive(Clone)]
pub struct Continuation {
    pub store: Store,
    pub intent: paseo_import::continuation::Intent,
}
impl EventEmitter<Continuation> for PaseoImportView {}

pub struct PaseoImportView {
    directory: PathBuf,
    endpoint: Endpoint,
    home: Entity<InputState>,
    codex: Entity<InputState>,
    dsh: Entity<InputState>,
    catalog: Option<Arc<Catalog>>,
    filter: Entity<InputState>,
    _filter_subscription: Subscription,
    visible: Vec<usize>,
    selection: BTreeSet<String>,
    batch_control: Option<BatchControl>,
    batch_poll: Option<Task<()>>,
    batch_progress: BatchProgress,
    batch_report: Option<BatchReport>,
    batches: Vec<BatchSummary>,
    batch_page: Option<BatchPage>,
    scratch: Option<ScratchReport>,
    saved: Vec<ImportedWorkspace>,
    parent: Option<Uuid>,
    sessions: Vec<Session>,
    selected: Option<Uuid>,
    page: Option<HistoryPage>,
    cards: Vec<Entity<ItemView>>,
    busy: bool,
    error: Option<String>,
    notice: Option<String>,
    scroll: ScrollHandle,
}
impl PaseoImportView {
    pub(crate) fn composing(&self, window: &mut Window, cx: &mut App) -> bool {
        [&self.home, &self.codex, &self.dsh, &self.filter]
            .iter()
            .any(|input| crate::input_composition::active(input, window, cx))
    }

    pub fn new(
        directory: PathBuf,
        endpoint: Endpoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let home = cx.new(|cx| InputState::new(window, cx).placeholder("Absolute Paseo home"));
        let codex = cx.new(|cx| InputState::new(window, cx).placeholder("Codex history root"));
        let dsh =
            cx.new(|cx| InputState::new(window, cx).placeholder("DeepSeek Harness history root"));
        let filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter workspace name or path…"));
        let filter_subscription = cx.subscribe(&filter, |_, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        if let Some(base) = std::env::var_os("HOME") {
            let path = std::env::var_os("PASEO_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(base).join(".paseo"));
            home.update(cx, |input, cx| {
                input.set_value(path.to_string_lossy(), window, cx)
            });
        }
        if let Ok(roots) = Roots::environment() {
            codex.update(cx, |input, cx| {
                input.set_value(roots.codex.to_string_lossy(), window, cx)
            });
            dsh.update(cx, |input, cx| {
                input.set_value(roots.dsh.to_string_lossy(), window, cx)
            });
        }
        let mut this = Self {
            directory,
            endpoint,
            home,
            codex,
            dsh,
            catalog: None,
            filter,
            _filter_subscription: filter_subscription,
            visible: Vec::new(),
            selection: BTreeSet::new(),
            batch_control: None,
            batch_poll: None,
            batch_progress: BatchProgress::default(),
            batch_report: None,
            batches: Vec::new(),
            batch_page: None,
            scratch: None,
            saved: Vec::new(),
            parent: None,
            sessions: Vec::new(),
            selected: None,
            page: None,
            cards: Vec::new(),
            busy: false,
            error: None,
            notice: None,
            scroll: ScrollHandle::new(),
        };
        this.run(Action::Saved, window, cx);
        this
    }
    pub fn is_busy(&self) -> bool {
        self.busy
    }
    fn roots(&self, cx: &App) -> Roots {
        Roots {
            codex: PathBuf::from(self.codex.read(cx).value().as_str()),
            dsh: PathBuf::from(self.dsh.read(cx).value().as_str()),
        }
    }
    fn start_batch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(catalog) = &self.catalog else {
            return;
        };
        let plan = match BatchPlan::selected(
            catalog,
            &self.selection,
            self.endpoint.clone(),
            self.roots(cx),
        ) {
            Ok(plan) => plan,
            Err(e) => {
                self.error = Some(e.to_string());
                cx.notify();
                return;
            }
        };
        self.begin_batch(Action::Batch(plan, BatchControl::default()), window, cx);
    }
    fn begin_batch(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let (control, total) = match &action {
            Action::Batch(plan, control) => (control.clone(), plan.workspaces.len()),
            Action::RetryBatch(_, control) => (
                control.clone(),
                self.batch_page
                    .as_ref()
                    .map(|p| p.summary.progress.workspaces_total)
                    .unwrap_or(0),
            ),
            _ => unreachable!("only a batch action starts batch progress"),
        };
        self.batch_progress = BatchProgress {
            workspaces_total: total,
            ..Default::default()
        };
        self.batch_control = Some(control.clone());
        self.batch_report = None;
        self.batch_page = None;
        self.run(action, window, cx);
        self.batch_poll = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let keep = this.update_in(cx, |this, _, cx| {
                    let Some(control) = &this.batch_control else {
                        return false;
                    };
                    this.batch_progress = control.progress();
                    cx.notify();
                    true
                });
                if !matches!(keep, Ok(true)) {
                    break;
                }
            }
        }));
    }
    fn run(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        self.notice = None;
        let directory = self.directory.clone();
        let endpoint = self.endpoint.clone();
        let task = cx.background_executor().spawn(async move {
            background(async move {
                let store = Store::open(directory).await?;
                match action {
                    Action::Scratch(remove) => {
                        Ok(Reply::Scratch(store.paseo_scratch(remove).await?))
                    }
                    Action::Continue(id) => Ok(Reply::Continuation(Continuation {
                        intent: store.prepare_paseo_continuation(id).await?,
                        store,
                    })),
                    Action::Preview(home) => {
                        Ok(Reply::Catalog(paseo_import::discover(home).await?))
                    }
                    Action::Import(catalog, id) => {
                        let imported = store
                            .import_paseo_workspace(&catalog, &id, endpoint)
                            .await?;
                        Ok(Reply::Imported(
                            Box::new(imported),
                            store.paseo_workspaces().await?,
                        ))
                    }
                    Action::Saved => Ok(Reply::Saved(
                        store.paseo_workspaces().await?,
                        store.paseo_batches(None).await?,
                    )),
                    Action::Batches(before) => {
                        Ok(Reply::Batches(store.paseo_batches(before).await?))
                    }
                    Action::BatchResults(id, after) => Ok(Reply::BatchResults(Box::new(
                        store.paseo_batch_results(id, after).await?,
                    ))),
                    Action::RetryBatch(id, control) => Ok(Reply::Batch(
                        Box::new(store.retry_paseo_batch(id, control).await?),
                        store.paseo_workspaces().await?,
                        store.paseo_batches(None).await?,
                    )),
                    Action::Sessions(parent, before) => Ok(Reply::Sessions(
                        parent,
                        store.paseo_sessions(parent, before).await?,
                    )),
                    Action::CopyWorkspace(parent, roots) => Ok(Reply::Report(
                        store.copy_paseo_workspace_history(parent, roots).await?,
                    )),
                    Action::CopySession(id, roots) => {
                        store.snapshot_paseo_history(id, roots).await?;
                        Ok(Reply::History(Box::new(store.paseo_history(id, 0).await?)))
                    }
                    Action::History(id, after) => Ok(Reply::History(Box::new(
                        store.paseo_history(id, after).await?,
                    ))),
                    Action::Batch(plan, control) => Ok(Reply::Batch(
                        Box::new(store.import_paseo_batch(plan, control).await?),
                        store.paseo_workspaces().await?,
                        store.paseo_batches(None).await?,
                    )),
                }
            })
        });
        cx.spawn_in(window,async move |this,cx| {
            let result:Result<Reply,Error>=task.await;
            let _closed=this.update_in(cx,|this,_,cx| {
                this.busy=false;
                this.batch_control=None;
                this.batch_poll=None;
                match result {
                    Ok(Reply::Continuation(event))=>cx.emit(event),
                    Ok(Reply::Scratch(report))=>this.scratch=Some(report),
                    Ok(Reply::Catalog(catalog))=>{this.catalog=Some(Arc::new(catalog));this.selection.clear();},
                    Ok(Reply::Saved(saved,batches))=>{this.saved=saved;this.batches=batches;},
                    Ok(Reply::Batches(batches))=>this.batches=batches,
                    Ok(Reply::BatchResults(page))=>this.batch_page=Some(*page),
                    Ok(Reply::Imported(imported,saved))=>{this.saved=saved;this.notice=Some(format!("Workspace saved with {} session identities. Copy workspace history next.",imported.session_count));},
                    Ok(Reply::Sessions(parent,sessions))=>{this.parent=Some(parent);this.sessions=sessions;this.selected=None;this.page=None;this.cards.clear();},
                    Ok(Reply::Report(report))=>{
                        this.notice=Some(format!("{} history snapshots available; {} failed. Existing snapshots kept.",report.copied,report.failures.len()));
                        if !report.failures.is_empty(){this.error=Some(report.failures.iter().map(|(id,e)|format!("{id}: {e}")).collect::<Vec<_>>().join("\n"));}
                    },
                    Ok(Reply::History(page))=>{
                        let cwd=this.sessions.iter().find(|s|s.id==page.session_id).map(|s|s.agent.cwd.to_string_lossy().into_owned()).unwrap_or_default();
                        let thread=page.session_id.to_string();
                        this.cards=page.items.iter().filter_map(|item|Row::item(&thread,"import",item)).map(|row|cx.new(|cx|ItemView::new(row,cwd.clone(),cx))).collect();
                        this.selected=Some(page.session_id);this.page=Some(*page);
                    },
                    Ok(Reply::Batch(report,saved,batches))=>{this.saved=saved;this.batch_report=Some(*report);this.batches=batches;},
                    Err(error)=>this.error=Some(error.to_string()),
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
}
impl Render for PaseoImportView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.filter.read(cx).value().to_lowercase();
        self.visible = self
            .catalog
            .as_ref()
            .map(|catalog| {
                catalog
                    .candidates
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| {
                        query.is_empty()
                            || c.source.name().to_lowercase().contains(&query)
                            || c.source
                                .cwd
                                .to_string_lossy()
                                .to_lowercase()
                                .contains(&query)
                    })
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default();
        div().id("paseo-import").size_full().overflow_y_scroll().track_scroll(&self.scroll).flex().flex_col().gap_3().p_4()
            .child(div().text_lg().child("Import from Paseo"))
            .child(format!("Target Host: {}. Imports reference existing directories; provider histories are copied into private Fleetpane snapshots.",self.endpoint.name))
            .child("Preview is read-only. Import saves workspace/session identities. Copy workspace history copies every supported log; failures remain visible. Running Paseo sessions keep their current owner.")
            .child("Codex: prepare an independent fork from a complete saved legacy history, then explicitly fork on its target Host. DeepSeek ACP has no independent fork; imported history remains available for browsing.")
            .child(div().flex().gap_2().child(div().flex_1().min_w_0().child(Input::new(&self.home).disabled(self.busy)))
                .child(Button::new("preview-paseo").small().label("Preview Paseo").disabled(self.busy).on_click(cx.listener(|this,_,window,cx|this.run(Action::Preview(PathBuf::from(this.home.read(cx).value().as_str())),window,cx)))))
            .child(div().flex().gap_2().child(div().flex_1().min_w_0().child(Input::new(&self.codex).disabled(self.busy))).child(div().flex_1().min_w_0().child(Input::new(&self.dsh).disabled(self.busy))))
            .when(self.busy && self.batch_control.is_none(),|pane|pane.child("Working…"))
            .when_some(self.batch_control.as_ref(),|pane,control|pane
                .child(format!("{} / {} workspaces checked · {} histories available · {} history failures · {} · {}",
                    self.batch_progress.workspaces_done,self.batch_progress.workspaces_total,self.batch_progress.histories_available,self.batch_progress.histories_failed,
                    self.batch_progress.current_workspace.as_deref().unwrap_or("Reading source…"),self.batch_progress.current_session.as_deref().unwrap_or("")))
                .child(Button::new("stop-paseo-batch").small().label(if control.is_cancelled(){"Stopping after current copy…"}else{"Stop after current copy"}).disabled(control.is_cancelled()).on_click(cx.listener(|this,_,_,cx|{if let Some(control)=&this.batch_control{control.cancel();cx.notify();}}))))
            .when_some(self.error.clone(),|pane,e|pane.child(div().text_color(cx.theme().danger).child(e)))
            .when_some(self.notice.clone(),|pane,n|pane.child(n))
            .when_some(self.catalog.as_ref(),|pane,catalog| {
                pane.child(format!("{} workspaces · {} unassociated sessions excluded · preview {}",catalog.candidates.len(),catalog.unassociated_sessions,&catalog.revision[..12]))
                    .child(Input::new(&self.filter).disabled(self.busy))
                    .child(div().flex().gap_2().items_center()
                        .child(format!("{} matching · {} selected",self.visible.len(),self.selection.len()))
                        .child(Button::new("select-matching-paseo").small().label("Select matching").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{if let Some(catalog)=&this.catalog{for &i in &this.visible{this.selection.insert(catalog.candidates[i].source.workspace_id.clone());}}cx.notify();})))
                        .child(Button::new("clear-paseo-selection").small().ghost().label("Clear").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.selection.clear();cx.notify();})))
                        .child(Button::new("migrate-paseo-selection").small().primary().label("Migrate selected + history").disabled(self.busy||self.selection.is_empty()).on_click(cx.listener(|this,_,window,cx|this.start_batch(window,cx)))))
                    .child(uniform_list("paseo-candidates",self.visible.len(),cx.processor(|this,range:std::ops::Range<usize>,_,cx| {
                        let Some(catalog)=&this.catalog else{return Vec::new();};
                        range.map(|position| {let i=this.visible[position];let candidate=&catalog.candidates[i];let id=candidate.source.workspace_id.clone();let preview=catalog.clone();let selection_id=id.clone();
                            div().h(px(62.)).flex().items_center().gap_2()
                                .child(Button::new(("select-paseo-workspace",i)).small().label(if this.selection.contains(&id){"Selected"}else{"Select"}).disabled(this.busy).on_click(cx.listener(move|this,_,_,cx|{if !this.selection.remove(&selection_id){this.selection.insert(selection_id.clone());}cx.notify();})))
                                .child(div().flex_1().min_w_0().flex().flex_col().child(div().truncate().child(format!("{} · {} sessions",candidate.source.name(),candidate.sessions.len()))).child(div().text_xs().truncate().child(candidate.source.cwd.to_string_lossy().into_owned())))
                                .child(Button::new(("import-paseo-workspace",i)).small().label("Import workspace").disabled(this.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Import(preview.clone(),id.clone()),window,cx))))
                        }).collect()
                    })).h(px(124.)).flex_shrink_0())
            })
            .when_some(self.batch_report.as_ref(),|pane,report|pane
                .child(format!("{} · {} / {} workspaces checked · {} histories available · {} history failures · {} workspace failures",
                    if report.cancelled{"Migration stopped"}else{"Migration finished"},report.progress.workspaces_done,report.progress.workspaces_total,report.progress.histories_available,report.progress.histories_failed,
                    report.results.iter().filter(|r|r.error.is_some()).count()))
                .child("Saved imports and snapshots are kept. Repeat the same selection to continue. Counts include existing snapshots.")
                .child(uniform_list("paseo-batch-results",report.results.len(),cx.processor(|this,range:std::ops::Range<usize>,_,cx|{
                    let Some(report)=&this.batch_report else{return Vec::new();};
                    range.map(|i|{let r=&report.results[i];let details=batch_details(r);
                        div().h(px(62.)).flex().items_center().gap_2()
                            .child(div().flex_1().min_w_0().flex().flex_col().child(div().truncate().child(format!("{} · {} · {} histories · {} failed · {} warnings",r.name,if r.error.is_some(){"Failed"}else if r.reused{"Already imported"}else{"Imported"},r.histories_available,r.history_failures.len(),r.history_warnings.len())))
                                .child(div().text_xs().truncate().child(r.error.clone().or_else(||r.registration_error.clone()).unwrap_or_else(||if r.histories_finished{"History copy checked".into()}else{"History copy incomplete".into()}))))
                            .child(Button::new(("paseo-batch-result",i)).small().label("Details").disabled(this.busy).on_click(cx.listener(move|this,_,_,cx|{this.notice=Some(details.clone());cx.notify();})))
                    }).collect()
                })).h(px(124.)).flex_shrink_0()))
            .child(self.saved_batches(cx))
            .child(self.scratch_controls(cx))
            .child(div().flex().gap_2().child(div().text_lg().child("Imported workspaces"))
                .child(Button::new("reload-paseo-imports").small().ghost().label("Reload imports").disabled(self.busy).on_click(cx.listener(|this,_,window,cx|this.run(Action::Saved,window,cx)))))
            .child(uniform_list("paseo-saved",self.saved.len(),cx.processor(|this,range:std::ops::Range<usize>,_,cx|range.map(|i| {
                let saved=&this.saved[i];let id=saved.id;
                div().h(px(68.)).flex().items_center().gap_2()
                    .child(div().flex_1().min_w_0().flex().flex_col().child(div().truncate().child(format!("{} · {} · {} sessions",saved.source.name(),saved.endpoint.name,saved.session_count))).child(div().text_xs().truncate().child(saved.registration_error.clone().unwrap_or_else(||saved.source.cwd.to_string_lossy().into_owned()))))
                    .child(Button::new(("paseo-workspace-sessions",i)).small().label("Sessions").disabled(this.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Sessions(id,None),window,cx))))
                    .child(Button::new(("copy-paseo-workspace",i)).small().label("Copy workspace history").disabled(this.busy).on_click(cx.listener(move|this,_,window,cx|{let roots=this.roots(cx);this.run(Action::CopyWorkspace(id,roots),window,cx)})))
            }).collect())).h(px(136.)).flex_shrink_0())
            .when_some(self.parent,|pane,parent|pane
                .child(div().flex().gap_2().child("Historical sessions")
                    .child(Button::new("paseo-sessions-first").small().ghost().label("First sessions").disabled(self.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Sessions(parent,None),window,cx))))
                    .child(Button::new("paseo-sessions-next").small().ghost().label("Older sessions").disabled(self.busy||self.sessions.len()<32).on_click(cx.listener(move|this,_,window,cx|{let before=this.sessions.last().map(|s|s.position);this.run(Action::Sessions(parent,before),window,cx)}))))
                .child(uniform_list("paseo-imported-sessions",self.sessions.len(),cx.processor(|this,range:std::ops::Range<usize>,_,cx|range.map(|i| {
                    let session=&this.sessions[i];let id=session.id;
                    div().h(px(62.)).flex().items_center().gap_2()
                        .child(div().flex_1().min_w_0().flex().flex_col().child(div().truncate().child(format!("{} · {}",session.agent.title.as_deref().unwrap_or(&session.agent.id),session.agent.provider))).child(div().text_xs().truncate().child(session.agent.id.clone())))
                        .child(Button::new(("view-paseo-history",i)).small().label("View history").disabled(this.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::History(id,0),window,cx))))
                        .when(session.agent.provider=="codex",|row|row.child(Button::new(("prepare-paseo-fork",i)).small().label("Continue in Codex…").disabled(this.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Continue(id),window,cx)))))
                        .child(Button::new(("copy-paseo-history",i)).small().label("Copy & view history").disabled(this.busy).on_click(cx.listener(move|this,_,window,cx|{let roots=this.roots(cx);this.run(Action::CopySession(id,roots),window,cx)})))
                }).collect())).h(px(186.)).flex_shrink_0()))
            .when_some(self.page.as_ref(),|pane,page| {
                let id=page.session_id;let next=page.next;
                pane.child(div().flex().gap_2().child(format!("Saved history · {} items · {} source records",page.status.rows,page.status.source_records))
                    .child(Button::new("paseo-history-first").small().label("First page").disabled(self.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::History(id,0),window,cx))))
                    .child(Button::new("paseo-history-next").small().label(if next.is_some(){"Next page"}else{"End of history"}).disabled(self.busy||next.is_none()).on_click(cx.listener(move|this,_,window,cx|{if let Some(after)=next{this.run(Action::History(id,after),window,cx)}}))))
                    .when_some(page.status.warning.clone(),|pane,w|pane.child(div().text_color(cx.theme().danger).child(w)))
                    .children(self.cards.iter().cloned())
            })
    }
}

fn batch_details(r: &paseo_import::WorkspaceResult) -> String {
    format!(
        "{} ({})\n{}\n{}\n{} histories available; {} failures; {} warnings\n{}\n{}\n{}",
        r.name,
        r.source_id,
        r.error.as_deref().unwrap_or(if r.imported_id.is_some() {
            "Workspace identity saved"
        } else {
            "Workspace import not confirmed"
        }),
        if r.imported_id.is_none() {
            "Directory not checked"
        } else {
            r.registration_error
                .as_deref()
                .unwrap_or("Directory registered")
        },
        r.histories_available,
        r.history_failures.len(),
        r.history_warnings.len(),
        if r.histories_finished {
            "History traversal finished"
        } else {
            "History traversal incomplete"
        },
        r.history_failures
            .iter()
            .map(|(id, e)| format!("{id}: {e}"))
            .collect::<Vec<_>>()
            .join("\n"),
        r.history_warnings
            .iter()
            .map(|(id, e)| format!("{id}: {e}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}
