mod attention;
mod completion;
mod continuation;
mod directory_source;
mod draft;
mod images;
mod observe;
mod progress;
mod recent;
mod recovery;
mod row_changes;
mod session_settings;
mod stopping;
mod tasks;
mod tree_navigation;
use crate::conversation_item::{ItemView, OpenFile, Row};
use crate::question_view::{AnswerEvent, QuestionView};
use fleetpane_runtime::Snapshot;
use fleetpane_session::questions::{self, Request as QuestionRequest};
use fleetpane_session::{
    Thread, Turn,
    client::{self, Command, Completed, Connection, Error},
    directory::{Directory, Page, PageRequest, Query},
    history::{History, Request as HistoryRequest},
    models::{Catalog, Page as ModelPage, TurnOptions},
};
use gpui_kit::component::{
    ActiveTheme, Disableable, Selectable, Sizable,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::{prelude::*, *};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
use uuid::Uuid;

pub(crate) fn background<T>(
    future: impl std::future::Future<Output = Result<T, Error>>,
) -> Result<T, Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| Error::Transport(error.to_string()))?
        .block_on(future)
}

struct QuestionEntry {
    request: QuestionRequest,
    view: Entity<QuestionView>,
    _subscription: Subscription,
}

fn composer_max_rows(window: &Window) -> usize {
    // Leave room for pending requests and Stop in the minimum-size window.
    if window.viewport_size().height < px(680.) {
        3
    } else {
        6
    }
}

pub struct HostView {
    socket: PathBuf,
    connection: Option<Connection>,
    snapshot: Option<Snapshot>,
    attention_target: Option<fleetpane_session::attention::Item>,
    completion_target: Option<fleetpane_session::attention::review::Item>,
    completion_reveal: bool,
    directory_target: Option<fleetpane_session::fleet_directory::Source>,
    directory: Directory,
    directory_changed: bool,
    models: Catalog,
    models_requested: bool,
    turn_options: BTreeMap<String, TurnOptions>,
    search: Entity<InputState>,
    filter_workspace: Entity<InputState>,
    rename: Entity<InputState>,
    rename_target: Option<String>,
    thread: Option<Thread>,
    // Membership observed from provider directory; separate from loaded status.
    selected_archived: bool,
    session_tree: Option<Entity<crate::session_tree_view::SessionTreeView>>,
    session_tree_subscription: Option<Subscription>,
    tree_opening: Option<Uuid>,
    tree_return: Option<fleetpane_session::session_tree::Target>,
    history: Option<History>,
    rows: Vec<Row>,
    row_index: BTreeMap<String, usize>,
    questions: Vec<QuestionEntry>,
    permission_preview: Option<(Row, Entity<ItemView>)>,
    item_views: BTreeMap<String, Entity<ItemView>>,
    file_subscriptions: BTreeMap<String, Subscription>,
    permission_file_subscription: Option<Subscription>,
    timeline: ListState,
    interactions_scroll: ScrollHandle,
    directory_scroll: UniformListScrollHandle,
    workspace: Entity<InputState>,
    composer: Entity<TextareaState>,
    draft: draft::State,
    recent: recent::State,
    tasks: tasks::State,
    busy: bool,
    poll_paused: bool,
    polling: bool,
    observer_polled_at: Option<std::time::Instant>,
    observer_error: Option<String>,
    recovery: recovery::Recovery,
    revision: u64,
    cursor: u64,
    error: Option<String>,
    pending: Option<(Uuid, Command)>,
    pending_uncertain: bool,
    creation_target: Option<fleetpane_session::routing::Target>,
    workspace_target: Option<fleetpane_session::workspaces::Workspace>,
    creation_intent: Option<(
        fleetpane_session::creations::Store,
        fleetpane_session::creations::Intent,
    )>,
    continuation: Option<continuation::State>,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone)]
pub(crate) struct FileRequest {
    pub thread_id: String,
    pub cwd: String,
    pub target: fleetpane_session::files::Target,
}
impl EventEmitter<FileRequest> for HostView {}

impl HostView {
    pub(crate) fn matches_file_request(&self, request: &FileRequest) -> bool {
        self.thread
            .as_ref()
            .is_some_and(|thread| thread.id == request.thread_id && thread.cwd == request.cwd)
    }
    pub fn prepare_creation(
        &mut self,
        store: fleetpane_session::creations::Store,
        intent: fleetpane_session::creations::Intent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace.update(cx, |input, cx| {
            input.set_value(intent.cwd.clone(), window, cx)
        });
        self.creation_target = Some(intent.target.clone());
        self.workspace_target = None;
        self.creation_intent = Some((store, intent));
        cx.notify();
    }

    pub fn prepare_workspace(
        &mut self,
        workspace: fleetpane_session::workspaces::Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cwd = workspace
            .registration
            .directory
            .path
            .to_str()
            .expect("inspected UTF-8 directory")
            .to_owned();
        self.workspace
            .update(cx, |input, cx| input.set_value(cwd, window, cx));
        self.creation_target = None;
        self.creation_intent = None;
        self.workspace_target = Some(workspace);
        cx.notify();
    }

    fn draining(&self) -> bool {
        self.snapshot.as_ref().is_some_and(|state| {
            state.admission.mode == fleetpane_runtime::lifecycle::Mode::Draining
        })
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
    }

    pub fn badge(&self) -> String {
        if self.recovery.blocked() || self.observer_error.is_some() {
            return "Updates unavailable".into();
        }
        if let Some(state) = &self.snapshot {
            if state.fault.is_some() {
                return "Unavailable".into();
            }
            if self.error.is_some() {
                return "Attention".into();
            }
            if !state.pending_requests.is_empty() {
                return format!("{} pending", state.pending_requests.len());
            }
            if self.draining() {
                return format!("Draining · {} active", state.activity.turns.len());
            }
            return if self.owns() {
                "Controlling"
            } else {
                "Read only"
            }
            .into();
        }
        if self.error.is_some() {
            "Unavailable"
        } else {
            "Connecting"
        }
        .into()
    }

    pub fn new(
        socket: PathBuf,
        draft_directory: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let workspace =
            cx.new(|cx| InputState::new(window, cx).placeholder("Absolute workspace directory"));
        let mut max_rows = composer_max_rows(window);
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, max_rows)
                .placeholder("Message · Enter adds a line")
        });
        let subscription = cx.subscribe_in(&composer, window, |this, input, event, window, cx| {
            if matches!(event, InputEvent::Change) {
                let text = input.read(cx).value().to_string();
                this.edit_draft(text, window, cx);
                cx.notify();
            }
        });
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Title contains…"));
        let filter_workspace =
            cx.new(|cx| InputState::new(window, cx).placeholder("Exact workspace directory"));
        let rename = cx.new(|cx| InputState::new(window, cx).placeholder("Session name"));
        let mut subscriptions = vec![subscription];
        subscriptions.push(cx.observe_window_bounds(window, move |this, window, cx| {
            let rows = composer_max_rows(window);
            if rows != max_rows {
                max_rows = rows;
                this.composer
                    .update(cx, |input, cx| input.set_auto_grow(1, rows, cx));
            }
        }));
        subscriptions.push(cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.recovery.wake();
                this.observer_polled_at = None;
                this.poll(window, cx);
            }
        }));
        for input in [&search, &filter_workspace, &rename] {
            subscriptions.push(cx.subscribe(input, |_, _, event, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }));
        }
        for input in [&search, &filter_workspace] {
            subscriptions.push(
                cx.subscribe_in(input, window, |this, _, event, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.apply_filters(window, cx);
                    }
                }),
            );
        }
        subscriptions.push(
            cx.subscribe_in(&rename, window, |this, _, event, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.save_name(window, cx);
                }
            }),
        );
        let timeline = ListState::new(0, ListAlignment::Bottom, window.viewport_size().height);
        // Geometric end detection is unknown while virtual rows are unmeasured.
        // Tail mode tracks user intent without treating that unknown as "follow".
        timeline.set_follow_mode(FollowMode::Tail);
        let mut view = Self {
            socket,
            connection: None,
            snapshot: None,
            attention_target: None,
            completion_target: None,
            completion_reveal: false,
            directory_target: None,
            directory: Directory::default(),
            directory_changed: false,
            models: Catalog::default(),
            models_requested: false,
            turn_options: BTreeMap::new(),
            search,
            filter_workspace,
            rename,
            rename_target: None,
            thread: None,
            selected_archived: false,
            session_tree: None,
            session_tree_subscription: None,
            tree_opening: None,
            tree_return: None,
            tasks: tasks::State::default(),
            history: None,
            rows: Vec::new(),
            row_index: BTreeMap::new(),
            questions: Vec::new(),
            permission_preview: None,
            item_views: BTreeMap::new(),
            file_subscriptions: BTreeMap::new(),
            permission_file_subscription: None,
            // List scrolling uses measured heights. With zero leading overdraw,
            // older rows remain zero-height and scrolling stops at the first
            // measured row. Measure one viewport on either side as we scroll.
            timeline,
            directory_scroll: UniformListScrollHandle::new(),
            interactions_scroll: ScrollHandle::new(),
            workspace,
            composer,
            draft: draft::State::new(draft_directory),
            recent: recent::State::default(),
            busy: false,
            poll_paused: false,
            polling: false,
            observer_polled_at: None,
            observer_error: None,
            recovery: recovery::Recovery::default(),
            revision: 0,
            cursor: 0,
            error: None,
            pending: None,
            pending_uncertain: false,
            creation_target: None,
            workspace_target: None,
            creation_intent: None,
            continuation: None,
            _subscriptions: subscriptions,
        };
        view.connect(false, window, cx);
        view.update_recent(recent::Action::Load, window, cx);
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                if this
                    .update_in(cx, |this, window, cx| this.poll(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        view
    }

    fn adopt_snapshot(&mut self, state: Snapshot) {
        if self.connection.as_ref().is_some_and(|connection| {
            connection.lease.host_id != state.host_id
                || state.owner.as_ref() != Some(&connection.lease)
        }) {
            // Once a newer observation revokes our lease, never restore it implicitly.
            self.connection = None;
            self.poll_paused = self.thread.is_some();
            self.recovery.clear();
            self.error = Some("Host identity or controller changed. Take control explicitly and reload this session; retained content is a previous observation.".into());
        }
        if self
            .snapshot
            .as_ref()
            .is_some_and(|old| old.host_id != state.host_id)
        {
            self.session_tree = None;
            self.session_tree_subscription = None;
            self.tree_opening = None;
            self.tree_return = None;
        }
        self.snapshot = Some(state);
    }

    fn owns(&self) -> bool {
        self.connection
            .as_ref()
            .zip(self.snapshot.as_ref())
            .is_some_and(|(connection, state)| {
                state.owner.as_ref() == Some(&connection.lease) && state.fault.is_none()
            })
    }

    fn can_act(&self) -> bool {
        self.owns() && !self.recovery.blocked() && self.observer_error.is_none()
    }

    fn connect(&mut self, take_control: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        self.revision += 1;
        let socket = self.socket.clone();
        let existing = self.connection.clone();
        let task = cx.background_executor().spawn(async move {
            background(async move {
                let state = client::snapshot(&socket).await?;
                let connection = if take_control {
                    Some(Connection::claim(socket, &state).await?)
                } else {
                    existing.filter(|connection| state.owner.as_ref() == Some(&connection.lease))
                };
                let state = if let Some(connection) = &connection {
                    client::snapshot(&connection.socket).await?
                } else {
                    state
                };
                Ok((state, connection))
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok((state, connection)) => {
                        this.observer_error = None;
                        this.recovery.clear();
                        if this.thread.is_none() {
                            this.cursor = state.sequence;
                        }
                        if this
                            .snapshot
                            .as_ref()
                            .is_some_and(|old| old.host_id != state.host_id)
                        {
                            this.questions.clear();
                            this.permission_preview = None;
                        }
                        this.connection = connection;
                        this.adopt_snapshot(state);
                        this.sync_questions(window, cx);
                        this.sync_permission_preview(cx);
                        for entry in &this.questions {
                            entry.view.update(cx, |view, cx| view.allow_submit(cx));
                        }
                        this.models_requested = false;
                        // Keep the old cursor until the selected history is read.
                        // A failed reload must not silently skip disconnected output.
                        this.poll_paused = this.thread.is_some();
                        if this.owns() {
                            let command = this.thread.as_ref().map_or_else(
                                || Command::List(this.directory.refresh()),
                                |thread| Command::Inspect {
                                    thread_id: thread.id.clone(),
                                    resume: thread.status.kind == "notLoaded",
                                },
                            );
                            this.command(command, window, cx);
                        }
                    }
                    Err(error) if matches!(error, Error::Transport(_)) => {
                        this.observer_error = Some(error.to_string());
                    }
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() || !self.can_act() {
            return;
        }
        if matches!(
            command,
            Command::Send { .. }
                | Command::Steer { .. }
                | Command::Open { .. }
                | Command::Inspect { .. }
                | Command::Fork { .. }
        ) && !draft::observes_selected(&command, self.thread.as_ref())
            && !self.draft_can_navigate()
        {
            self.error=Some("Finish saving or resolving the current draft before opening another session or sending.".into());
            cx.notify();
            return;
        }
        if self.selected_archived
            && match &command {
                Command::Send { thread_id, .. }
                | Command::Steer { thread_id, .. }
                | Command::Inspect {
                    thread_id,
                    resume: true,
                }
                | Command::Open {
                    thread_id,
                    resume: true,
                } => self
                    .thread
                    .as_ref()
                    .is_some_and(|selected| &selected.id == thread_id),
                _ => false,
            }
        {
            self.error = Some("Restore this archived session before continuing its context".into());
            cx.notify();
            return;
        }
        if command.is_composer_input() && !self.draft_sendable() {
            return;
        }
        if command.is_composer_input()
            && let Some(stop) = self.snapshot.as_ref().and_then(|state| {
                state.activity.stopping.as_ref()?.iter().find(|stop| {
                    self.thread
                        .as_ref()
                        .is_some_and(|thread| thread.id == stop.thread_id)
                })
            })
        {
            self.error = Some(stop.description().into());
            cx.notify();
            return;
        }
        if let Some(state) = &self.snapshot {
            if matches!(command, Command::Create { .. })
                && let Some(target) = &self.creation_target
                && let Err(error) = target.check(state)
            {
                self.error = Some(error.to_string());
                cx.notify();
                return;
            }
            match command.permitted_by(&state.admission) {
                Ok(true) => {}
                Ok(false) => {
                    self.error = Some("Host is draining. Select a ready Host for new work; reads, answers and Stop remain available.".into());
                    cx.notify();
                    return;
                }
                Err(error) => {
                    self.error = Some(error.to_string());
                    cx.notify();
                    return;
                }
            }
        }
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if matches!(command, Command::Stop { .. }) {
            self.request_stop(connection, command, window, cx);
            return;
        }
        self.busy = true;
        if matches!(command, Command::Models { .. }) {
            self.models_requested = true;
        }
        self.revision += 1;
        self.error = None;
        let creation = if let Command::Create { cwd } = &command {
            if let Some((store, intent)) = &self.creation_intent {
                if cwd != &intent.cwd {
                    self.busy = false;
                    self.error = Some("Workspace differs from the saved creation. Return to New via group to prepare another creation.".into());
                    cx.notify();
                    return;
                }
                Some((store.clone(), intent.id))
            } else {
                None
            }
        } else {
            None
        };
        let workspace = if let Command::Create { cwd } = &command {
            if let Some(workspace) = &self.workspace_target {
                if std::path::Path::new(cwd) != workspace.registration.directory.path {
                    self.busy = false;
                    self.error = Some("Directory differs from the selected saved workspace. Choose the workspace again to prepare it.".into());
                    cx.notify();
                    return;
                }
                Some(workspace.clone())
            } else {
                None
            }
        } else {
            None
        };
        let continuation = if matches!(command, Command::Fork { .. }) {
            let Some(state) = &mut self.continuation else {
                self.busy = false;
                self.error = Some("Prepare an imported history first".into());
                cx.notify();
                return;
            };
            if state.intent.attempted
                || !matches!(&command,Command::Fork {thread_id,path,cwd} if *thread_id==state.intent.thread_id && *path==state.intent.path && *cwd==state.intent.cwd)
            {
                self.busy = false;
                self.error=Some("Fork already attempted or differs from its saved binding. Check saved fork status.".into());
                cx.notify();
                return;
            }
            state.intent.attempted = true;
            Some((state.store.clone(), state.intent.clone()))
        } else {
            None
        };
        let operation = continuation
            .as_ref()
            .map(|(_, intent)| intent.id)
            .or_else(|| creation.as_ref().map(|(_, id)| *id))
            .unwrap_or_else(Uuid::new_v4);
        let operation_command = command.clone();
        let saved_draft = if command.is_composer_input() {
            self.draft.record.clone()
        } else {
            None
        };
        let draft_directory = self.draft.directory.clone();
        let task = cx.background_executor().spawn(async move {
            background(async move {
                if let Some(draft) = saved_draft {
                    fleetpane_session::creations::Store::open(draft_directory)
                        .await?
                        .send_draft(&draft, &connection, operation, &operation_command)
                        .await
                } else if let Some((store, intent)) = continuation {
                    store
                        .dispatch_paseo_continuation(intent.session_id, intent.id, &connection)
                        .await
                } else if let Some((store, id)) = creation {
                    store.dispatch(id, &connection).await
                } else if let Some(workspace) = workspace {
                    workspace.create(&connection, operation).await
                } else {
                    connection.execute(&operation_command, operation).await
                }
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.finish(command, result, window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn finish(
        &mut self,
        command: Command,
        result: Result<Completed, Error>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.busy = false;
        if command.is_composer_input() {
            self.draft.record = None;
        }
        let sent_command = command.is_composer_input();
        match result {
            Ok(completed) => {
                if let Err(error) = self.apply_reply(&command, completed, window, cx) {
                    self.error = Some(error.to_string());
                    // Directory decode failures do not invalidate selected history replay.
                    if !matches!(
                        command,
                        Command::List(_) | Command::Models { .. } | Command::History(_)
                    ) {
                        self.poll_paused = true;
                    }
                } else if let Some(next) = match &command {
                    Command::Inspect { thread_id, .. } => {
                        if let Some(history) = &self.history {
                            history.next().map(Command::History)
                        } else {
                            Some(Command::Open {
                                thread_id: thread_id.clone(),
                                resume: false,
                            })
                        }
                    }
                    Command::Fork { .. } => self.thread.as_ref().and_then(|thread| {
                        if let Some(history) = &self.history {
                            history.next().map(Command::History)
                        } else {
                            Some(Command::Open {
                                thread_id: thread.id.clone(),
                                resume: false,
                            })
                        }
                    }),
                    Command::History(HistoryRequest::Turns { .. }) => self
                        .history
                        .as_ref()
                        .and_then(History::next)
                        .map(Command::History),
                    _ => None,
                } {
                    self.command(next, window, cx);
                } else if matches!(
                    command,
                    Command::Rename { .. }
                        | Command::Create { .. }
                        | Command::Restore { .. }
                        | Command::Fork { .. }
                ) || self.take_continuation_refresh(&command)
                {
                    self.command(Command::List(self.directory.refresh()), window, cx);
                } else if matches!(command, Command::Models { .. }) {
                    if let Some(cursor) = self.models.next_cursor.clone() {
                        self.command(
                            Command::Models {
                                cursor: Some(cursor),
                            },
                            window,
                            cx,
                        );
                    }
                } else if !self.models_requested && !self.is_acp() {
                    self.command(Command::Models { cursor: None }, window, cx);
                }
            }
            Err(Error::Pending(id)) => {
                self.error = Some(Error::Pending(id).to_string());
                self.pending = Some((id, command));
                self.pending_uncertain = false;
            }
            Err(Error::Uncertain { id, reason }) => {
                self.error = Some(Error::Uncertain { id, reason }.to_string());
                self.pending = Some((id, command));
                self.pending_uncertain = true;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        if sent_command {
            self.load_draft(false, window, cx);
        }
        cx.notify();
    }

    fn apply_reply(
        &mut self,
        command: &Command,
        completed: Completed,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        command.validate_completed(&completed)?;
        match command {
            Command::Mode { thread_id, .. } => {
                let modes: fleetpane_runtime::acp::modes::State = serde_json::from_value(
                    completed
                        .result
                        .get("modes")
                        .cloned()
                        .ok_or("ACP mode result lacks state")?,
                )?;
                modes.validate()?;
                if let Some(thread) = self
                    .thread
                    .as_mut()
                    .filter(|thread| &thread.id == thread_id)
                {
                    thread.acp_modes = Some(modes);
                }
            }
            Command::Config { thread_id, .. } => {
                if let Some(thread) = self.thread.as_mut().filter(|t| &t.id == thread_id) {
                    thread.config_options = serde_json::from_value(
                        completed
                            .result
                            .get("configOptions")
                            .cloned()
                            .ok_or("ACP config result lacks options")?,
                    )?;
                }
            }
            Command::Models { cursor } => {
                self.models.apply(
                    cursor,
                    serde_json::from_value::<ModelPage>(completed.result)?,
                )?;
            }
            Command::List(request) => {
                let page: Page = serde_json::from_value(completed.result)?;
                self.directory.apply(request, page)?;
                if self.thread.as_ref().is_some_and(|selected| {
                    self.directory
                        .threads
                        .iter()
                        .any(|row| row.id == selected.id)
                }) {
                    self.selected_archived = request.query.archived;
                }
                if request.cursor.is_none() {
                    self.directory_changed = false;
                    self.directory_scroll.scroll_to_item(0, ScrollStrategy::Top);
                }
            }
            Command::Restore { thread_id } => {
                if self
                    .thread
                    .as_ref()
                    .is_some_and(|thread| &thread.id == thread_id)
                {
                    self.selected_archived = false;
                }
                if self.directory.query.archived {
                    self.directory
                        .threads
                        .retain(|thread| &thread.id != thread_id);
                }
            }
            Command::Rename { thread_id, name } => {
                #[derive(Deserialize)]
                struct Named {}
                let _: Named = serde_json::from_value(completed.result)?;
                let name = name.trim().to_owned();
                if let Some(thread) = self
                    .thread
                    .as_mut()
                    .filter(|thread| &thread.id == thread_id)
                {
                    thread.name = Some(name.clone());
                }
                for thread in self
                    .directory
                    .threads
                    .iter_mut()
                    .filter(|thread| &thread.id == thread_id)
                {
                    thread.name = Some(name.clone());
                }
                self.rename_target = None;
            }
            Command::History(request) => {
                let history = self
                    .history
                    .as_mut()
                    .ok_or("history reply has no traversal")?;
                let thread = self
                    .thread
                    .as_mut()
                    .ok_or("history reply has no selected thread")?;
                history.apply(thread, request, completed.result, completed.sequence)?;
                if matches!(request, HistoryRequest::Items { .. }) || history.next().is_none() {
                    self.poll_paused = false;
                }
                self.rebuild_rows(cx);
            }
            Command::Create { .. }
            | Command::Open { .. }
            | Command::Inspect { .. }
            | Command::Fork { .. } => {
                #[derive(Deserialize)]
                struct Opened {
                    thread: Thread,
                }
                let thread: Thread = serde_json::from_value::<Opened>(completed.result)?.thread;
                thread.validate()?;
                if draft::observes_selected(command, self.thread.as_ref())
                    && self
                        .thread
                        .as_ref()
                        .is_some_and(|current| current.cwd != thread.cwd)
                {
                    return Err("Session directory changed during history hydration; existing draft retained".into());
                }
                if matches!(command, Command::Create { .. }) {
                    self.update_recent(
                        recent::Action::Remember(PathBuf::from(&thread.cwd)),
                        window,
                        cx,
                    );
                    self.creation_target = None;
                    self.creation_intent = None;
                    self.workspace_target = None;
                }
                if matches!(command, Command::Fork { .. })
                    && let Some(state) = &mut self.continuation
                {
                    state.resolution = Some(fleetpane_session::creations::Resolution::Created {
                        thread_id: thread.id.clone(),
                    });
                    state.refresh_directory = true;
                }
                self.history = if matches!(command, Command::Inspect { .. } | Command::Fork { .. })
                    && thread.supports_history_pages()
                {
                    if !thread.turns.is_empty() {
                        return Err("metadata-only read returned hydrated turns".into());
                    }
                    Some(History::new(thread.id.clone()))
                } else {
                    None
                };

                let mut metadata = thread.clone();
                metadata.turns.clear();
                if let Some(old) = self
                    .directory
                    .threads
                    .iter_mut()
                    .find(|old| old.id == thread.id)
                {
                    *old = metadata;
                }
                let directory_open = matches!(command, Command::Inspect { resume: false, .. })
                    && self.directory_target.as_ref().is_some_and(|source| {
                        source.thread_id == thread.id
                            && self
                                .snapshot
                                .as_ref()
                                .is_some_and(|state| state.host_id == source.host_id)
                    });
                self.selected_archived = if directory_open {
                    self.directory_target
                        .as_ref()
                        .expect("matching directory source")
                        .archived
                } else if matches!(command, Command::Fork { .. } | Command::Create { .. }) {
                    false
                } else if self.directory.threads.iter().any(|row| row.id == thread.id) {
                    self.directory.query.archived
                } else {
                    self.thread.as_ref().is_some_and(|old| old.id == thread.id)
                        && self.selected_archived
                };
                if directory_open {
                    self.directory_target = None;
                }
                self.rename_target = None;
                if self.thread.as_ref().is_none_or(|old| old.id != thread.id) {
                    self.interactions_scroll.set_offset(point(px(0.), px(0.)));
                    self.session_tree = None;
                    self.session_tree_subscription = None;
                    self.tree_opening = None;
                    if !directory_open {
                        self.tree_return = None;
                    }
                }
                self.thread = Some(thread);
                self.cursor = completed.sequence;
                self.poll_paused = self.history.is_some();
                self.timeline.reset(0);
                self.timeline.set_follow_mode(FollowMode::Tail);
                self.rows.clear();
                self.rebuild_rows(cx);
                self.load_draft(false, window, cx);
                self.load_tasks(window, cx);
            }
            Command::Tasks { thread_id } => self.apply_tasks(thread_id, completed)?,
            Command::Send { thread_id, .. } => {
                #[derive(Deserialize)]
                struct Started {
                    turn: Turn,
                }
                let turn = serde_json::from_value::<Started>(completed.result)?.turn;
                if let Some(thread) = self
                    .thread
                    .as_mut()
                    .filter(|thread| &thread.id == thread_id)
                {
                    if !thread.turns.iter().any(|existing| existing.id == turn.id) {
                        thread.turns.push(turn);
                    }

                    self.composer
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    self.rebuild_rows(cx);
                }
                // Do not skip notifications emitted before turn/start replied.
            }
            Command::Steer { .. } => {
                self.composer
                    .update(cx, |input, cx| input.set_value("", window, cx));
                // Acceptance is not delivery: only provider events add history.
            }
            Command::Stop { .. } => { /* Completion remains provider-authoritative. */ }
        }
        Ok(())
    }

    fn poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.polling {
            return;
        }
        if !self.owns() {
            self.observe(window, cx);
            return;
        }
        if self.poll_paused {
            self.observe(window, cx);
            return;
        }
        if !self.recovery.ready(std::time::Instant::now()) {
            return;
        }
        let Some(connection) = self.connection.clone() else {
            return;
        };
        // One poll task at a time; independent user commands invalidate old results.
        self.polling = true;
        let after = self.cursor;
        let revision = self.revision;
        let task = cx
            .background_executor()
            .spawn(async move { background(connection.poll(after)) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.polling = false;
                if this.revision != revision {
                    return;
                }
                match result {
                    Ok((state, events)) => {
                        let mut invalidation = row_changes::Invalidation::default();
                        let recovered =
                            this.recovery.clear() | this.observer_error.take().is_some();
                        let mut changed = recovered
                            || this.snapshot.as_ref().is_none_or(|old| {
                                old.owner != state.owner
                                    || old.pending_requests != state.pending_requests
                                    || old.activity.stopping != state.activity.stopping
                                    || old.activity.turns != state.activity.turns
                                    || old.fault != state.fault
                            });
                        this.adopt_snapshot(state);
                        this.sync_questions(window, cx);
                        for event in events {
                            if event.sequence <= this.cursor {
                                continue;
                            }
                            if matches!(
                                event.message.get("method").and_then(Value::as_str),
                                Some(
                                    "thread/name/updated"
                                        | "thread/status/changed"
                                        | "thread/acpSessionInfo/updated"
                                )
                            ) {
                                let id = event
                                    .message
                                    .pointer("/params/threadId")
                                    .and_then(Value::as_str);
                                if let Some(thread) = this
                                    .directory
                                    .threads
                                    .iter_mut()
                                    .find(|thread| Some(thread.id.as_str()) == id)
                                {
                                    let old_name = thread.name.clone();
                                    match thread.apply(&event) {
                                        Ok(applied) => {
                                            changed |= applied;
                                            this.directory_changed |= old_name != thread.name;
                                        }
                                        Err(error) => {
                                            this.error = Some(error.to_string());
                                            this.poll_paused = true;
                                            changed = true;
                                        }
                                    }
                                }
                            }
                            if let Some(thread) = &mut this.thread {
                                match if let Some(history) = &this.history {
                                    history.apply_event(thread, &event)
                                } else {
                                    thread.apply(&event)
                                } {
                                    Ok(applied) => {
                                        changed |= applied;
                                        invalidation.record(&event, applied);
                                    }
                                    Err(error) => {
                                        this.error = Some(error.to_string());
                                        this.poll_paused = true;
                                        changed = true;
                                        invalidation = row_changes::Invalidation::All;
                                        break;
                                    }
                                }
                            }
                            this.cursor = event.sequence;
                        }
                        if changed {
                            this.refresh_rows(&invalidation, cx);
                            cx.notify();
                        }
                    }
                    Err(error) => {
                        this.poll_paused = !matches!(error, Error::Transport(_));
                        this.recovery.failed(&error, std::time::Instant::now());
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn sync_questions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let requests: Vec<_> = self
            .snapshot
            .iter()
            .flat_map(|state| &state.pending_requests)
            .filter_map(|pending| questions::Request::decode(pending).ok().flatten())
            .collect();
        // Decode errors remain visible in the unsupported request card below.
        self.questions
            .retain(|entry| requests.contains(&entry.request));
        for request in requests {
            if self.questions.iter().any(|entry| entry.request == request) {
                continue;
            }
            let view = cx.new(|cx| QuestionView::new(request.clone(), window, cx));
            let subscription =
                cx.subscribe_in(&view, window, |this, _, event: &AnswerEvent, window, cx| {
                    this.submit_questions(
                        event.request.clone(),
                        event.response.clone(),
                        window,
                        cx,
                    );
                });
            self.questions.push(QuestionEntry {
                request,
                view,
                _subscription: subscription,
            });
        }
    }

    fn submit_questions(
        &mut self,
        request: QuestionRequest,
        response: questions::Response,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || !self.can_act() || self.poll_paused || self.pending.is_some() {
            return;
        }
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if self
            .thread
            .as_ref()
            .is_none_or(|thread| thread.id != request.params.thread_id)
        {
            return;
        }
        self.busy = true;
        self.error = None;
        self.revision += 1;
        let sent = request.clone();
        let task = cx
            .background_executor()
            .spawn(async move { background(connection.answer_questions(&sent, &response)) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(()) => {
                        if let Some(state) = &mut this.snapshot {
                            state.pending_requests.retain(|pending| pending.get("id") != Some(&request.id));
                        }
                        // Drop the form, including sensitive values, after Host acknowledgment.
                        this.sync_questions(window, cx);
                    }
                    Err(error) => {
                        let uncertain = matches!(error, Error::Transport(_)) || matches!(&error, Error::Rejected { code, .. } if code == "outcome_unknown");
                        if !uncertain && let Some(entry) = this.questions.iter().find(|entry| entry.request == request) {
                            entry.view.update(cx, |view, cx| view.allow_submit(cx));
                        }
                        this.error = Some(if uncertain { format!("Answer delivery is uncertain. Reconnect to reconcile before submitting again: {error}") } else { error.to_string() });
                    }
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    fn refresh_rows(&mut self, invalidation: &row_changes::Invalidation, cx: &mut Context<Self>) {
        let Some(thread) = &self.thread else {
            self.rebuild_rows(cx);
            return;
        };
        let Some(updates) = invalidation.project(thread, &self.rows, &self.row_index) else {
            self.rebuild_rows(cx);
            return;
        };
        self.sync_permission_preview(cx);
        for (index, row) in updates {
            self.item_views[&row.id].update(cx, |view, cx| view.set_row(row.clone(), cx));
            self.rows[index] = row;
            self.timeline.splice(index..index + 1, 1);
        }
    }

    fn rebuild_rows(&mut self, cx: &mut Context<Self>) {
        self.sync_permission_preview(cx);
        let mut rows = Vec::new();
        if let Some(thread) = &self.thread {
            rows = Row::conversation(thread);
            let changes = row_changes::plan(&self.rows, &rows);
            self.item_views
                .retain(|id, _| changes.retained.contains(id.as_str()));
            self.file_subscriptions
                .retain(|id, _| changes.retained.contains(id.as_str()));
            for index in changes.updated {
                let row = &rows[index];
                if let Some(view) = self.item_views.get(&row.id) {
                    view.update(cx, |view, cx| view.set_row(row.clone(), cx));
                } else {
                    let view = cx.new(|cx| ItemView::new(row.clone(), thread.cwd.clone(), cx));
                    let session = thread.id.clone();
                    let subscription =
                        cx.subscribe(&view, move |this, _, event: &OpenFile, cx| {
                            if this.thread.as_ref().is_some_and(|thread| {
                                thread.id == session && thread.cwd == event.cwd
                            }) {
                                cx.emit(FileRequest {
                                    thread_id: session.clone(),
                                    cwd: event.cwd.clone(),
                                    target: event.target.clone(),
                                });
                            }
                        });
                    self.file_subscriptions.insert(row.id.clone(), subscription);
                    self.item_views.insert(row.id.clone(), view);
                }
            }
        } else {
            self.item_views.clear();
            self.file_subscriptions.clear();
        }
        let first = self
            .rows
            .iter()
            .zip(&rows)
            .take_while(|(old, new)| old == new)
            .count();
        let suffix = self.rows[first..]
            .iter()
            .rev()
            .zip(rows[first..].iter().rev())
            .take_while(|(old, new)| old == new)
            .count();
        self.timeline
            .splice(first..self.rows.len() - suffix, rows.len() - first - suffix);
        self.row_index = rows
            .iter()
            .enumerate()
            .map(|(index, row)| (row.id.clone(), index))
            .collect();
        self.rows = rows;
        self.reveal_completion();
    }

    fn sync_permission_preview(&mut self, cx: &mut Context<Self>) {
        let pending = self
            .snapshot
            .as_ref()
            .and_then(|state| {
                state
                    .pending_requests
                    .iter()
                    .find(|pending| {
                        let target = pending.pointer("/params/threadId").and_then(Value::as_str);
                        target.is_none()
                            || self
                                .thread
                                .as_ref()
                                .is_some_and(|thread| Some(thread.id.as_str()) == target)
                    })
                    .map(|pending| (state.host_id, pending))
            })
            .filter(|(_, pending)| pending["method"] == "session/request_permission");
        let Some((host, pending)) = pending else {
            self.permission_preview = None;
            self.permission_file_subscription = None;
            return;
        };
        let row = Row::permission(
            format!("permission:{host}:{}", pending["id"]),
            pending,
            self.thread.as_ref(),
        );
        if let Some((previous, view)) = &mut self.permission_preview
            && previous.id == row.id
        {
            if previous != &row {
                view.update(cx, |view, cx| view.set_row(row.clone(), cx));
                *previous = row;
            }
        } else {
            let cwd = self
                .thread
                .as_ref()
                .map_or_else(String::new, |thread| thread.cwd.clone());
            let view = cx.new(|cx| ItemView::permission(row.clone(), cwd, cx));
            let session = self.thread.as_ref().map(|thread| thread.id.clone());
            self.permission_file_subscription =
                Some(cx.subscribe(&view, move |this, _, event: &OpenFile, cx| {
                    if this.thread.as_ref().is_some_and(|thread| {
                        Some(&thread.id) == session.as_ref() && thread.cwd == event.cwd
                    }) {
                        cx.emit(FileRequest {
                            thread_id: session.clone().expect("selected session checked"),
                            cwd: event.cwd.clone(),
                            target: event.target.clone(),
                        });
                    }
                }));
            self.permission_preview = Some((row, view));
        }
    }

    fn save_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.rename_target.clone() {
            self.command(
                Command::Rename {
                    thread_id: id,
                    name: self.rename.read(cx).value().to_string(),
                },
                window,
                cx,
            );
        }
    }

    fn apply_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = |input: &Entity<InputState>| {
            let value = input.read(cx).value().trim().to_owned();
            (!value.is_empty()).then_some(value)
        };
        let page = PageRequest {
            query: Query {
                search: text(&self.search),
                cwd: text(&self.filter_workspace),
                archived: self.directory.query.archived,
            },
            cursor: None,
        };
        self.command(Command::List(page), window, cx);
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(thread) = &self.thread
            && thread.can_send()
            && !self.poll_paused
        {
            self.command(
                Command::Send {
                    thread_id: thread.id.clone(),
                    text: self.composer.read(cx).value().to_string(),
                    images: self.draft_images().to_vec(),
                    options: {
                        let options = self
                            .turn_options
                            .get(&thread.id)
                            .cloned()
                            .unwrap_or_default();
                        if let Err(error) = options.validate(&self.models) {
                            self.error = Some(error.to_string());
                            cx.notify();
                            return;
                        }
                        options
                    },
                },
                window,
                cx,
            );
        }
    }

    fn steer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_acp()
            && self.draft_images().is_empty()
            && !self.poll_paused
            && let Some(thread) = &self.thread
            && let Some(turn) = thread.active_turn()
        {
            self.command(
                Command::Steer {
                    thread_id: thread.id.clone(),
                    turn_id: turn.id.clone(),
                    text: self.composer.read(cx).value().to_string(),
                },
                window,
                cx,
            );
        }
    }

    fn choose_options(&mut self, id: &str, options: TurnOptions, cx: &mut Context<Self>) {
        if self.busy
            || !self.owns()
            || self.pending.is_some()
            || self.thread.as_ref().is_none_or(|thread| thread.id != id)
        {
            return;
        }
        match options.validate(&self.models) {
            Ok(()) => {
                self.turn_options.insert(id.to_owned(), options);
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        cx.notify();
    }

    fn is_acp(&self) -> bool {
        self.snapshot
            .as_ref()
            .is_some_and(|s| s.provider_info["fleetpaneProvider"] == "deepseek-acp")
    }

    fn acp_options(&self, cx: &mut Context<Self>) -> AnyElement {
        let unavailable = self.busy || !self.can_act() || self.pending.is_some() || self.draining();
        let blocked = unavailable || self.thread.as_ref().is_none_or(|t| !t.can_send());
        let mut row = div()
            .flex()
            .flex_wrap()
            .gap_2()
            .child("Session configuration");
        if let Some(thread) = &self.thread {
            if let Some(modes) = &thread.acp_modes {
                let owner = cx.entity().downgrade();
                let thread_id = thread.id.clone();
                let choices = modes.available_modes.clone();
                let current = modes.current_mode_id.clone();
                row = row.child(
                    Button::new("acp-mode")
                        .small()
                        .label(format!("Mode: {}", modes.current_name()))
                        .disabled(
                            unavailable
                                || !matches!(thread.status.kind.as_str(), "idle" | "active")
                                || choices.is_empty(),
                        )
                        .dropdown_menu_with_anchor(Anchor::BottomLeft, move |mut menu, _, _| {
                            menu = menu.scrollable(true).max_h(px(320.));
                            for choice in &choices {
                                let owner = owner.clone();
                                let id = thread_id.clone();
                                let mode_id = choice.id.clone();
                                let label = choice.description.as_ref().map_or_else(
                                    || choice.name.clone(),
                                    |description| format!("{} — {description}", choice.name),
                                );
                                menu = menu.item(
                                    PopupMenuItem::new(label)
                                        .checked(current == choice.id)
                                        .on_click(move |_, window, cx| {
                                            if let Some(entity) = owner.upgrade() {
                                                entity.update(cx, |this, cx| {
                                                    this.command(
                                                        Command::Mode {
                                                            thread_id: id.clone(),
                                                            mode_id: mode_id.clone(),
                                                        },
                                                        window,
                                                        cx,
                                                    )
                                                });
                                            }
                                        }),
                                );
                            }
                            menu
                        }),
                );
            }
            for option in &thread.config_options {
                let owner = cx.entity().downgrade();
                let thread_id = thread.id.clone();
                let config_id = option.id.clone();
                let choices: Vec<(String, String)> = option
                    .choices()
                    .iter()
                    .map(|(v, n)| (v.to_string(), n.to_string()))
                    .collect();
                let selected = option.current_value.clone();
                row = row.child(
                    Button::new(SharedString::from(format!("acp-option:{}", option.id)))
                        .small()
                        .label(format!("{}: {}", option.name, option.selected_name()))
                        .disabled(blocked || option.kind != "select")
                        .dropdown_menu_with_anchor(Anchor::BottomLeft, move |mut menu, _, _| {
                            for (value, name) in &choices {
                                let owner = owner.clone();
                                let thread_id = thread_id.clone();
                                let config_id = config_id.clone();
                                let value = value.clone();
                                menu = menu.item(
                                    PopupMenuItem::new(name.clone())
                                        .checked(selected == value)
                                        .on_click(move |_, window, cx| {
                                            if let Some(entity) = owner.upgrade() {
                                                entity.update(cx, |this, cx| {
                                                    this.command(
                                                        Command::Config {
                                                            thread_id: thread_id.clone(),
                                                            config_id: config_id.clone(),
                                                            value: value.clone(),
                                                        },
                                                        window,
                                                        cx,
                                                    )
                                                });
                                            }
                                        }),
                                );
                            }
                            menu
                        }),
                );
            }
        }
        row.into_any_element()
    }

    fn render_history_navigation(&self, blocked: bool, cx: &mut Context<Self>) -> AnyElement {
        let show_latest = !self.rows.is_empty() && !self.timeline.is_following_tail();
        div()
            .when(self.history.is_some() || show_latest, |view| {
                view.px_4()
                    .pb_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .children(self.history.as_ref().map(|history| {
                        let next = history.next();
                        let count = self.thread.as_ref().map_or(0, |thread| {
                            thread
                                .turns
                                .iter()
                                .map(|turn| turn.items.len())
                                .sum::<usize>()
                        });
                        let label = if history.at_limit {
                            "Loaded history limit reached · Reload for recent messages".to_owned()
                        } else if next.is_none() {
                            format!("Beginning of conversation · {count} loaded")
                        } else {
                            format!("Load earlier messages · {count} loaded")
                        };
                        Button::new("history-earlier")
                            .small()
                            .ghost()
                            .label(label)
                            .disabled(blocked || next.is_none())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(request) = &next {
                                    this.command(Command::History(request.clone()), window, cx);
                                }
                            }))
                    }))
                    .when(show_latest, |view| {
                        view.child(
                            Button::new("history-latest")
                                .small()
                                .label("Jump to latest")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    // Kit's tail mode anchors at the end, including
                                    // a growing last row, and resumes stream follow.
                                    this.timeline.set_follow_mode(FollowMode::Tail);
                                    cx.notify();
                                })),
                        )
                    })
            })
            .into_any_element()
    }

    fn render_options(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.is_acp() {
            return self.acp_options(cx);
        }
        let blocked = self.busy || !self.can_act() || self.pending.is_some();
        let id = self.thread.as_ref().map(|thread| thread.id.clone());
        let options = id
            .as_ref()
            .and_then(|id| self.turn_options.get(id))
            .cloned()
            .unwrap_or_default();
        let model = options.model().and_then(|name| self.models.find(name));
        let label = model.map_or_else(
            || options.model().unwrap_or("Session settings").to_owned(),
            |model| model.display_name.clone(),
        );
        let owner = cx.entity().downgrade();
        let model_owner = owner.clone();
        let model_id = id.clone();
        let effort_id = id.clone();
        let effort = if model.is_some_and(|model| model.supported_reasoning_efforts.is_empty()) {
            "No reasoning control".to_owned()
        } else {
            options.effort().unwrap_or("Session effort").to_owned()
        };
        let effort_disabled = model
            .is_none_or(|model| model.supported_reasoning_efforts.is_empty())
            || !self.models.complete;
        div().flex().flex_col().gap_1()
            .child(div().flex().items_center().flex_wrap().gap_2()
                .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Next message"))
                .child(Button::new("turn-model").small().label(label).disabled(blocked || id.is_none())
                    .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, cx| {
                        let Some(entity) = model_owner.upgrade() else { return menu; };
                        let view = entity.read(cx);
                        let selected = model_id.as_ref().and_then(|id| view.turn_options.get(id));
                        let reset_owner = model_owner.clone();
                        let reset_id = model_id.clone();
                        let mut menu = menu.scrollable(true).max_h(px(320.)).item(
                            PopupMenuItem::new("Use session settings")
                                .checked(selected.is_none_or(|options| options.model().is_none()))
                                .on_click(move |_, _, cx| {
                                    if let (Some(entity), Some(id)) = (reset_owner.upgrade(), &reset_id) {
                                        entity.update(cx, |view, cx| view.choose_options(id, TurnOptions::default(), cx));
                                    }
                                }),
                        );
                        for model in view.models.models.iter().filter(|model| !model.hidden) {
                            let name = model.model.clone();
                            let recommended = (!model.supported_reasoning_efforts.is_empty()).then(|| model.default_reasoning_effort.clone());
                            let target = model_id.clone();
                            let owner = model_owner.clone();
                            menu = menu.item(PopupMenuItem::new(model.display_name.clone())
                                .checked(selected.and_then(TurnOptions::model) == Some(name.as_str()))
                                .disabled(!view.models.complete)
                                .on_click(move |_, _, cx| {
                                    if let (Some(entity), Some(id)) = (owner.upgrade(), &target) {
                                        entity.update(cx, |view, cx| {
                                            match TurnOptions::new(&view.models, Some(name.clone()), recommended.clone()) {
                                                Ok(options) => view.choose_options(id, options, cx),
                                                Err(error) => { view.error = Some(error.to_string()); cx.notify(); }
                                            }
                                        });
                                    }
                                }));
                        }
                        menu
                    }))
                .child(Button::new("turn-effort").small().label(effort)
                    .disabled(blocked || id.is_none() || effort_disabled)
                    .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, cx| {
                        let Some(entity) = owner.upgrade() else { return menu; };
                        let view = entity.read(cx);
                        let selected = effort_id.as_ref().and_then(|id| view.turn_options.get(id));
                        let Some(model) = selected.and_then(TurnOptions::model).and_then(|name| view.models.find(name)) else { return menu; };
                        let mut menu = menu.scrollable(true).max_h(px(320.));
                        for effort in &model.supported_reasoning_efforts {
                            let value = effort.reasoning_effort.clone();
                            let name = model.model.clone();
                            let owner = owner.clone();
                            let target = effort_id.clone();
                            menu = menu.item(PopupMenuItem::new(value.clone())
                                .checked(selected.and_then(TurnOptions::effort) == Some(value.as_str()))
                                .on_click(move |_, _, cx| {
                                    if let (Some(entity), Some(id)) = (owner.upgrade(), &target) {
                                        entity.update(cx, |view, cx| {
                                            match TurnOptions::new(&view.models, Some(name.clone()), Some(value.clone())) {
                                                Ok(options) => view.choose_options(id, options, cx),
                                                Err(error) => { view.error = Some(error.to_string()); cx.notify(); }
                                            }
                                        });
                                    }
                                }));
                        }
                        menu
                    }))
                .child(Button::new("reload-models").small().ghost().label("Reload models").disabled(blocked)
                    .on_click(cx.listener(|view, _, window, cx| view.command(Command::Models { cursor: None }, window, cx)))))
            .when(!self.models.complete, |row| row.child(div().text_xs().text_color(cx.theme().muted_foreground).child("Model catalog unavailable or incomplete. Session settings remain available.")))
            .when(self.models.complete && self.models.models.iter().all(|model| model.hidden), |row| row.child(div().text_xs().text_color(cx.theme().muted_foreground).child("No selectable models. Using session settings.")))
            .when_some(options.validate(&self.models).err(), |row, error| row.child(div().text_xs().text_color(cx.theme().danger).child(error.to_string())))
            .into_any_element()
    }

    fn check_operation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some((id, command)) = self.pending.clone() else {
            return;
        };
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.busy = true;
        self.revision += 1;
        let task = cx.background_executor().spawn(async move {
            background(async move { client::completed(connection.operation(id).await?) })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| match result {
                Err(
                    error @ (Error::Transport(_) | Error::Protocol(_) | Error::Rejected { .. }),
                ) => {
                    this.busy = false;
                    this.error = Some(format!("Could not reconcile operation {id}: {error}"));
                    cx.notify();
                }
                result => {
                    this.pending = None;
                    this.pending_uncertain = false;
                    this.error = None;
                    this.finish(command, result, window, cx);
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn set_admission(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.can_act() {
            return;
        }
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(state) = &self.snapshot else {
            return;
        };
        let expected = state.admission.generation;
        let mode = if self.draining() {
            fleetpane_runtime::lifecycle::Mode::Ready
        } else {
            fleetpane_runtime::lifecycle::Mode::Draining
        };
        self.busy = true;
        self.revision += 1;
        let task = cx.background_executor().spawn(async move {
            background(async move {
                connection.set_admission(expected, mode).await?;
                client::snapshot(&connection.socket).await
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                match result {
                    Ok(state) => { this.adopt_snapshot(state); this.error = None; },
                    Err(error) => { this.error = Some(format!("Could not confirm admission state: {error}. Reconnect to read the saved state before retrying.")); },
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    fn answer(
        &mut self,
        pending: Value,
        accept: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || !self.can_act() {
            return;
        }
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.busy = true;
        self.revision += 1;
        let task = cx
            .background_executor()
            .spawn(async move { background(connection.answer(&pending, accept)) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                if let Err(error) = result {
                    this.error = Some(error.to_string());
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn answer_acp(
        &mut self,
        pending: Value,
        choice: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || !self.can_act() || self.pending.is_some() {
            return;
        }
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.busy = true;
        self.revision += 1;
        let task = cx
            .background_executor()
            .spawn(async move { background(connection.answer_permission(&pending, &choice)) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                if let Err(error) = result {
                    this.error = Some(error.to_string());
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for HostView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owns = self.owns();
        let blocked = self.busy || !self.can_act() || self.pending.is_some();
        let stopping = self
            .snapshot
            .as_ref()
            .and_then(|state| {
                state.activity.stopping.as_ref()?.iter().find(|stop| {
                    self.thread
                        .as_ref()
                        .is_some_and(|thread| thread.id == stop.thread_id)
                })
            })
            .cloned();
        let stop_supported = self
            .snapshot
            .as_ref()
            .is_some_and(|state| state.activity.stopping.is_some());
        let title = self
            .thread
            .as_ref()
            .map_or("Select or create a session".into(), |thread| {
                thread.title().to_owned()
            });
        let send_disabled = blocked
            || stopping.is_some()
            || !self.draft_sendable()
            || self.selected_archived
            || self.draining()
            || self.poll_paused
            || self.thread.as_ref().is_none_or(|thread| !thread.can_send())
            || (self.composer.read(cx).value().trim().is_empty() && self.draft_images().is_empty())
            || (!self.draft_images().is_empty() && !self.supports_images());
        let send_disabled = send_disabled
            || self
                .thread
                .as_ref()
                .and_then(|thread| self.turn_options.get(&thread.id))
                .is_some_and(|options| options.validate(&self.models).is_err());
        let active_turn = self.thread.as_ref().and_then(Thread::active_turn).is_some();
        let cancel_requested = stopping.as_ref().is_some_and(|stop| !stop.can_retry())
            || self
                .thread
                .as_ref()
                .is_some_and(|thread| thread.status.cancel_requested);
        let steer_disabled = blocked
            || stopping.is_some()
            || cancel_requested
            || !self.draft_images().is_empty()
            || !self.draft_sendable()
            || self.draining()
            || self.poll_paused
            || !active_turn
            || self.is_acp()
            || self.composer.read(cx).value().trim().is_empty();
        let approvals = self
            .snapshot
            .as_ref()
            .map_or_else(Vec::new, |state| state.pending_requests.clone());
        let selected = self.thread.as_ref().map(|thread| thread.id.as_str());
        let other_requests: Vec<_> = approvals
            .iter()
            .filter(|pending| {
                pending
                    .pointer("/params/threadId")
                    .and_then(Value::as_str)
                    .is_some_and(|id| Some(id) != selected)
            })
            .cloned()
            .collect();
        let approvals: Vec<_> = approvals
            .into_iter()
            .filter(|pending| {
                let thread_id = pending.pointer("/params/threadId").and_then(Value::as_str);
                thread_id.is_none() || (thread_id == selected && selected.is_some())
            })
            .collect();
        let approval_count = approvals.len();
        for entry in &self.questions {
            entry.view.update(cx, |view, cx| {
                view.set_enabled(!blocked && !self.poll_paused, cx)
            });
        }
        let question_view = approvals
            .first()
            .and_then(|pending| {
                self.questions
                    .iter()
                    .find(|entry| pending.get("id") == Some(&entry.request.id))
            })
            .map(|entry| entry.view.clone());
        let has_question = question_view.is_some();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .h_16()
                    .px_6()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child("Fleetpane")
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .items_center()
                            .child(if self.recovery.blocked() || self.observer_error.is_some() {
                                "Host updates unavailable"
                            } else if owns {
                                "Connected · controlling"
                            } else {
                                "Read only · take control to work"
                            })
                            .when(self.busy, |row| row.child("Applying action…"))
                            .when(owns, |row| {
                                row.child(Button::new("host-admission")
                                    .small().ghost()
                                    .label(if self.draining() { "Accept new work" } else { "Drain Host" })
                                    .disabled(self.busy || !self.can_act())
                                    .on_click(cx.listener(|this, _, window, cx| this.set_admission(window, cx))))
                            })
                            .child(
                                Button::new("reconnect")
                                    .small()
                                    .ghost()
                                    .label("Reconnect")
                                    .disabled(self.busy)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.connect(false, window, cx)
                                    })),
                            )
                            .when(!owns, |row| {
                                row.child(
                                    Button::new("take-control")
                                        .small()
                                        .label("Take control")
                                        .disabled(self.busy)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.connect(true, window, cx)
                                        })),
                                )
                            }),
                    ),
            )
            .child(self.attention_banner(cx))
            .child(self.completion_banner(cx))
            .child(self.directory_source_banner(cx))
            .when(self.draining(), |view| {
                let state = self.snapshot.as_ref().expect("draining Host snapshot");
                view.child(div().px_6().py_2().text_sm().border_b_1().border_color(cx.theme().border)
                    .child(format!("Draining · {} active turns · {} live terminals · {} pending answers · {} in-flight operations · {} uncertain receipts. New work is paused; reads, terminal input, answers and Stop remain available. Host stays running.", state.activity.turns.len(), state.activity.terminal_ids.len(), state.pending_requests.len(), state.activity.in_flight_operations, state.activity.unknown_operations))
                    .children(state.activity.turns.iter().take(3).map(|turn| div().text_xs().child(format!("Session {} · turn {}", turn.thread_id, turn.turn_id.as_deref().unwrap_or("awaiting identity")))))
                    .when(state.activity.turns.len() > 3, |row| row.child("First 3 shown. The Host snapshot lists all observed active turns.")))
            })
            .children(self.error.iter().chain(self.observer_error.iter()).cloned().chain(self.recovery.message()).map(|error| {
                div()
                    .px_6()
                    .py_2()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone())
            }))
            .children(
                self.snapshot
                    .as_ref()
                    .and_then(|state| state.fault.clone())
                    .map(|fault| {
                        div()
                            .px_6()
                            .py_2()
                            .text_color(cx.theme().danger)
                            .child(format!("Provider unavailable: {fault}"))
                    }),
            )
            .when(self.pending.is_some(), |view| {
                view.child(
                    Button::new("check-operation")
                        .label("Check operation status")
                        .disabled(self.busy)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.check_operation(window, cx)),
                        ),
                ).when(self.pending_uncertain, |view| {
                    view.child(Button::new("acknowledge-uncertain")
                        .label("Continue without resending")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            // Explicit local acknowledgement. The UUID and its
                            // unknown receipt remain in the Host journal.
                            let (id, _) = this.pending.take().expect("pending uncertain operation");
                            this.pending_uncertain = false;
                            this.poll_paused = true;
                            this.error = Some(format!("Operation {id} remains uncertain. Inspect its receipt and provider state before any manual retry. Reconnect or select another session to continue."));
                            cx.notify();
                        })))
                })
            })
            .child(
                div()
                    .flex()
                    .items_stretch()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .w_80()
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .bg(cx.theme().sidebar)
                            .border_r_1()
                            .border_color(cx.theme().border)
                            .child(div().id("host-session-controls").max_h((window.viewport_size().height-px(340.)).max(px(180.))).overflow_y_scroll().flex_shrink_0()
                            .child(
                                div()
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child("Workspace directory")
                                    .child(Input::new(&self.workspace))
                                    .child(self.render_recent(cx))
                                    .child(self.render_continuation(cx))
                                    .child(
                                        Button::new("new-session")
                                            .label("New session")
                                            .disabled(blocked || self.draining() || self.recent.busy)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.command(
                                                    Command::Create {
                                                        cwd: this
                                                            .workspace
                                                            .read(cx)
                                                            .value()
                                                            .to_string(),
                                                    },
                                                    window,
                                                    cx,
                                                );
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .px_4()
                                    .pb_3()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child("Filter sessions")
                                    .child(div().flex().gap_2()
                                        .children([false,true].map(|archived| {
                                            Button::new(if archived {"archived-sessions"}else{"active-sessions"}).small()
                                                .label(if archived {"Archived"}else{"Sessions"})
                                                .selected(self.directory.query.archived == archived)
                                                .disabled(blocked || self.is_acp())
                                                .on_click(cx.listener(move |this,_,window,cx| {
                                                    let mut request = this.directory.refresh();
                                                    request.query.archived = archived;
                                                    this.command(Command::List(request),window,cx);
                                                }))
                                        })))
                                    .child(
                                        Input::new(&self.search)
                                            .aria_label("Title filter")
                                            .disabled(blocked || self.is_acp()),
                                    )
                                    .child(
                                        Input::new(&self.filter_workspace)
                                            .aria_label("Workspace filter")
                                            .disabled(blocked),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .gap_2()
                                            .child(
                                                Button::new("apply-filters")
                                                    .small()
                                                    .label("Apply")
                                                    .disabled(blocked)
                                                    .on_click(cx.listener(
                                                        |this, _, window, cx| {
                                                            this.apply_filters(window, cx)
                                                        },
                                                    )),
                                            )
                                            .child(
                                                Button::new("clear-filters")
                                                    .small()
                                                    .ghost()
                                                    .label("Clear")
                                                    .disabled(blocked)
                                                    .on_click(cx.listener(
                                                        |this, _, window, cx| {
                                                            this.search.update(cx, |input, cx| {
                                                                input.set_value("", window, cx)
                                                            });
                                                            this.filter_workspace.update(
                                                                cx,
                                                                |input, cx| {
                                                                    input.set_value("", window, cx)
                                                                },
                                                            );
                                                            this.command(
                                                                Command::List(
                                                                    PageRequest::default(),
                                                                ),
                                                                window,
                                                                cx,
                                                            );
                                                        },
                                                    )),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(format!(
                                                "Showing: {} · {}",
                                                self.directory
                                                    .query
                                                    .search
                                                    .as_deref()
                                                    .unwrap_or("all titles"),
                                                self.directory
                                                    .query
                                                    .cwd
                                                    .as_deref()
                                                    .unwrap_or("all workspaces")
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .px_4()
                                    .py_2()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(format!("Sessions · {}", self.directory.threads.len()))
                                    .child(
                                        Button::new("refresh-host")
                                            .small()
                                            .ghost()
                                            .label("Refresh")
                                            .disabled(blocked)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.command(
                                                    Command::List(this.directory.refresh()),
                                                    window,
                                                    cx,
                                                )
                                            })),
                                    ),
                            )
                            )
                            .when(
                                self.directory.threads.is_empty() && !self.busy && owns,
                                |pane| {
                                    pane.child(
                                        div()
                                            .p_4()
                                            .text_sm()
                                            .child("No sessions match the current filters."),
                                    )
                                },
                            )
                            .child(
                                uniform_list(
                                    "host-threads",
                                    self.directory.threads.len(),
                                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                                        range
                                            .map(|ix| {
                                                let thread = &this.directory.threads[ix];
                                                let id = thread.id.clone();
                                                let resume = thread.status.kind == "notLoaded" && !this.directory.query.archived;
                                                let label: String =
                                                    thread.title().chars().take(50).collect();
                                                div()
                                                    .h_16()
                                                    .px_4()
                                                    .py_2()
                                                    .child(
                                                        Button::new(SharedString::from(id.clone()))
                                                            .ghost()
                                                            .w_full()
                                                            .small()
                                                            .label(label)
                                                            .selected(
                                                                this.thread.as_ref().is_some_and(
                                                                    |selected| selected.id == id,
                                                                ),
                                                            )
                                                            .disabled(
                                                                this.busy
                                                                    || this.pending.is_some()
                                                                    || !this.owns(),
                                                            )
                                                            .on_click(cx.listener(
                                                                move |this, _, window, cx| {
                                                                    this.command(
                                                                        Command::Inspect {
                                                                            thread_id: id.clone(),
                                                                            resume,
                                                                        },
                                                                        window,
                                                                        cx,
                                                                    )
                                                                },
                                                            )),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .truncate()
                                                            .text_color(cx.theme().muted_foreground)
                                                            .child(format!(
                                                                "{} · {}",
                                                                thread.status.kind, thread.cwd
                                                            )),
                                                    )
                                            })
                                            .collect()
                                    }),
                                )
                                .track_scroll(&self.directory_scroll)
                                .flex_1(),
                            )
                            .when(self.directory_changed, |pane| {
                                pane.child(
                                    div()
                                        .px_4()
                                        .text_xs()
                                        .child("Session names changed. Refresh results."),
                                )
                            })
                            .when(self.directory.at_limit, |pane| {
                                pane.child(
                                    div()
                                        .p_4()
                                        .text_sm()
                                        .child("5,000-session limit reached. Narrow the filters."),
                                )
                            })
                            .when(self.directory.next().is_some(), |pane| {
                                pane.child(
                                    Button::new("load-more-sessions")
                                        .small()
                                        .ghost()
                                        .label("Load more sessions")
                                        .disabled(blocked)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            if let Some(page) = this.directory.next() {
                                                this.command(Command::List(page), window, cx);
                                            }
                                        })),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .p_4()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_3()
                                    .child(
                                        div().flex_1().min_w_0().text_lg().truncate().child(title),
                                    )
                                    .when_some(
                                        self.thread.as_ref().map(|thread| thread.id.clone()),
                                        |row, id| {
                                            row.child(
                                                Button::new("inspect-session-tree")
                                                    .small().ghost().label("Session tree")
                                                    .disabled(self.is_acp() || self.snapshot.is_none())
                                                    .on_click(cx.listener(move |this, _, window, cx| {
                                                        this.show_session_tree(window,cx);
                                                    })),
                                            ).child(
                                                Button::new("reload-thread")
                                                    .small()
                                                    .label("Reload conversation")
                                                    .disabled(blocked)
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            this.command(
                                                                Command::Inspect {
                                                                    thread_id: id.clone(),
                                                                    resume: false,
                                                                },
                                                                window,
                                                                cx,
                                                            )
                                                        },
                                                    )),
                                            )
                                        },
                                    ),
                            )
                            .child(self.reported_session_settings(cx))
                            .child(self.tree_navigation_banner(cx))
                            .when(self.thread.is_some(), |pane| {
                                pane.child(
                                    div()
                                        .px_4()
                                        .pb_2()
                                        .flex()
                                        .gap_2()
                                        .items_center()
                                        .when(self.rename_target.is_none(), |row| {
                                            row.child(
                                                Button::new("rename-session")
                                                    .small()
                                                    .ghost()
                                                    .label("Rename session")
                                                    .disabled(blocked || self.is_acp())
                                                    .on_click(cx.listener(
                                                        |this, _, window, cx| {
                                                            if let Some(thread) = &this.thread {
                                                                this.rename_target =
                                                                    Some(thread.id.clone());
                                                                let title =
                                                                    thread.title().to_owned();
                                                                this.rename.update(
                                                                    cx,
                                                                    |input, cx| {
                                                                        input.set_value(
                                                                            title, window, cx,
                                                                        );
                                                                        input.focus(window, cx);
                                                                    },
                                                                );
                                                                cx.notify();
                                                            }
                                                        },
                                                    )),
                                            )
                                        })
                                        .when(self.rename_target.is_some(), |row| {
                                            row.child(
                                                Input::new(&self.rename)
                                                    .aria_label("Session name")
                                                    .disabled(blocked)
                                                    .flex_1(),
                                            )
                                            .child(
                                                Button::new("save-session-name")
                                                    .small()
                                                    .label("Save name")
                                                    .disabled(
                                                        blocked || self.draining()
                                                            || self
                                                                .rename
                                                                .read(cx)
                                                                .value()
                                                                .trim()
                                                                .is_empty(),
                                                    )
                                                    .on_click(cx.listener(
                                                        |this, _, window, cx| {
                                                            this.save_name(window, cx);
                                                        },
                                                    )),
                                            )
                                            .child(
                                                Button::new("cancel-session-name")
                                                    .small()
                                                    .ghost()
                                                    .label("Cancel")
                                                    .disabled(self.busy)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.rename_target = None;
                                                        cx.notify();
                                                    })),
                                            )
                                        }),
                                )
                            })
                            .child(self.render_history_navigation(blocked, cx))
                            .child(
                                // Split the space left AFTER the header/editor.
                                // Details may use half; the virtual list keeps
                                // the rest even when task/approval content grows.
                                div().id("conversation-body").flex().flex_col().flex_1().min_h_0()
                            .child(
                                list(
                                    self.timeline.clone(),
                                    cx.processor(|this, ix: usize, _, _| {
                                        let row = &this.rows[ix];
                                        this.item_views[&row.id].clone().into_any_element()
                                    }),
                                )
                                .flex_1()
                                .min_h_0(),
                            )
                            .child(div().id("session-interactions")
                            .min_h_0()
                            .flex_shrink_0()
                            .max_h(relative(0.5))
                            .overflow_y_scroll()
                            .track_scroll(&self.interactions_scroll)
                            .child(div().flex().flex_col().flex_shrink_0()
                            .when(self.session_tree.is_some(), |pane| pane.child(
                                Button::new("close-session-tree").small().ghost().label("Close session tree")
                                    .on_click(cx.listener(|this, _, _, cx| {this.session_tree = None;this.session_tree_subscription=None;this.tree_opening=None;cx.notify();}))
                            ))
                            .children(self.session_tree.clone())
                            .when(self.thread.is_some(), |view| view.child(self.render_tasks(cx)))
                            .children(self.thread.as_ref().filter(|thread| {
                                thread.history_mode.as_deref() == Some("legacy")
                            }).map(|_| {
                                div()
                                    .px_4()
                                    .py_2()
                                    .text_sm()
                                    .bg(cx.theme().muted)
                                    .child("Older session: some command history may be unavailable. New sessions preserve tool history.")
                            }))
                            .children(self.thread.as_ref().filter(|thread|thread.history_mode.as_deref()==Some("acp-observed")).map(|thread| {
                                let id=thread.id.clone();
                                div().px_4().py_2().text_sm().bg(cx.theme().muted)
                                    .child("DeepSeek ACP: showing activity observed by Fleetpane. Earlier provider messages are not imported. Unknown turns have no confirmed outcome.")
                                    .when(thread.status.kind=="archived",|view| view.child(
                                        Button::new("resume-acp-context").small().label("Resume context")
                                            .disabled(self.busy || self.pending.is_some() || !self.owns())
                                            .on_click(cx.listener(move |this,_,window,cx| this.command(Command::Inspect{thread_id:id.clone(),resume:true},window,cx)))
                                    ))
                            }))
                            .children(self.thread.as_ref().filter(|_| self.selected_archived).map(|thread| {
                                let id = thread.id.clone();
                                div().px_4().py_2().text_sm().bg(cx.theme().muted)
                                    .child("Listed as archived · history is read-only until restored. Restoring adds this session to Sessions; it does not start a turn or restore descendants.")
                                    .child(Button::new("restore-session").small().label("Restore to sessions")
                                        .disabled(blocked || self.draining() || self.is_acp())
                                        .on_click(cx.listener(move |this,_,window,cx|this.command(Command::Restore {thread_id:id.clone()},window,cx))))
                            }))
                            .children(self.thread.as_ref().filter(|thread| !self.selected_archived && !self.is_acp() && thread.status.kind == "notLoaded").map(|thread| {
                                let id = thread.id.clone();
                                div().px_4().py_2().text_sm()
                                    .child("Context is not loaded. Resume explicitly to continue; this does not send a message.")
                                    .child(Button::new("resume-codex-context").small().label("Resume context")
                                        .disabled(blocked || self.draining())
                                        .on_click(cx.listener(move |this,_,window,cx|this.command(Command::Inspect {thread_id:id.clone(),resume:true},window,cx))))
                            }))
                            .children(other_requests.first().map(|pending| {
                                let thread_id = pending
                                    .pointer("/params/threadId")
                                    .and_then(Value::as_str)
                                    .map(str::to_owned);
                                div()
                                    .px_4()
                                    .py_2()
                                    .text_sm()
                                    .child(format!(
                                        "{} pending request(s) in another session",
                                        other_requests.len()
                                    ))
                                    .when_some(thread_id, |row, thread_id| {
                                        row.child(
                                            Button::new("open-request-session")
                                                .small()
                                                .ghost()
                                                .label("Open session")
                                                .disabled(blocked)
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.command(
                                                            Command::Inspect {
                                                                thread_id: thread_id.clone(),
                                                                resume: false,
                                                            },
                                                            window,
                                                            cx,
                                                        )
                                                    },
                                                )),
                                        )
                                    })
                            }))
                            .children(question_view)
                            .children(approvals.into_iter().take(1).filter(|_| !has_question).map(
                                |pending| {
                                    let supported = pending
                                        .pointer("/params/threadId")
                                        .and_then(Value::as_str)
                                        .is_some()
                                        && matches!(
                                            pending.get("method").and_then(Value::as_str),
                                            Some(
                                                "item/commandExecution/requestApproval"
                                                    | "item/fileChange/requestApproval"
                                            )
                                        );
                                    let accept = pending.clone();
                                    let decline = pending.clone();
                                    let key = pending
                                        .get("id")
                                        .map_or_else(|| "missing".into(), Value::to_string);
                                    let is_acp_permission = pending["method"] == "session/request_permission";
                                    let review_valid = self.permission_preview.as_ref().is_some_and(|(row, _)| row.error.is_none());
                                    let mut permission_buttons = div().flex().flex_wrap().gap_2();
                                    let mut permission_error = None;
                                    match fleetpane_session::acp::Permission::decode(&pending) {
                                        Ok(Some(permission)) => {
                                            for option in permission.options {
                                                let choice = option.option_id;
                                                let pending = pending.clone();
                                                permission_buttons = permission_buttons.child(
                                                    Button::new(SharedString::from(format!("acp-permission:{key}:{choice}")))
                                                        .label(option.name)
                                                        .disabled(blocked || !review_valid)
                                                        .on_click(cx.listener(move |this, _, window, cx| {
                                                            this.answer_acp(pending.clone(), choice.clone(), window, cx)
                                                        })),
                                                );
                                            }
                                        }
                                        Ok(None) => {}
                                        Err(error) => permission_error = Some(error),
                                    }
                                    div()
                                        .p_4()
                                        .border_t_1()
                                        .border_color(cx.theme().border)
                                        .child(format!(
                                            "Provider request · 1 of {approval_count} pending"
                                        ))
                                        .children(questions::Request::decode(&pending).err().map(
                                            |error| {
                                                div()
                                                    .text_color(cx.theme().danger)
                                                    .child(error.to_string())
                                            },
                                        ))
                                        .child(
                                            div()
                                                .id("approval-details")
                                                .max_h_24()
                                                .overflow_y_scroll()
                                                .text_sm()
                                            .when(!is_acp_permission, |details| details.child(pending.to_string())),
                                        )
                                        .children(is_acp_permission.then(|| self.permission_preview.as_ref().map(|(_, view)| view.clone())).flatten())
                                        .children(supported.then(||
                                            div()
                                                .flex()
                                                .gap_2()
                                                .child(
                                                    Button::new(SharedString::from(format!(
                                                        "approve:{key}"
                                                    )))
                                                    .label("Allow once")
                                                    .disabled(blocked || !supported)
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            this.answer(
                                                                accept.clone(),
                                                                true,
                                                                window,
                                                                cx,
                                                            )
                                                        },
                                                    )),
                                                )
                                                .child(
                                                    Button::new(SharedString::from(format!(
                                                        "decline:{key}"
                                                    )))
                                                    .label("Decline")
                                                    .disabled(blocked || !supported)
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            this.answer(
                                                                decline.clone(),
                                                                false,
                                                                window,
                                                                cx,
                                                            )
                                                        },
                                                    )),
                                                ),
                                        ))
                                        .children(permission_error.map(|error| div().text_color(cx.theme().danger).child(error)))
                                        .child(permission_buttons)
                                },
                            ))
                            .when(!has_question, |region| region.child(
                                div().p_4().flex().flex_col().gap_2()
                                    .child(self.render_draft(cx))
                                    .child(self.render_options(cx))
                                    .child(self.render_images(blocked, cx))
                                    .when(active_turn && self.is_acp() && !cancel_requested, |pane| pane.child(
                                        div().text_xs().text_color(cx.theme().muted_foreground)
                                            .child("DeepSeek ACP does not support live follow-ups. Stop the turn before sending a new message.")
                                    ))
                            ))
                            )))
                            .child(
                                div()
                                    .p_4()
                                    .border_t_1()
                                    .border_color(cx.theme().border)
                                    .flex()
                                    .flex_col()
                                    .flex_shrink_0()
                                    .gap_2()
                                    .child(self.render_progress(cx))
                                    .when(cancel_requested || stopping.is_some(), |pane| pane.child(
                                        div().text_xs().text_color(cx.theme().muted_foreground)
                                            .child(stopping.as_ref().map_or("Stop requested. Waiting for the agent to finish cancellation.", |stop| stop.description()))
                                            .children(stopping.as_ref().and_then(|stop| stop.error.clone()).map(|error| div().text_color(cx.theme().danger).child(error)))
                                    ))
                                    .when(active_turn && !stop_supported, |pane| pane.child(
                                        div().text_xs().text_color(cx.theme().muted_foreground)
                                            .child("This Host does not report Stop progress. Select an updated Host before stopping a turn.")
                                    ))
                                    .when(!has_question, |pane| {
                                        pane.child(
                                            Textarea::new(&self.composer)
                                                .aria_label("Message")
                                                .on_paste({
                                                    let view = cx.entity();
                                                    move |clipboard, window, cx| view.update(cx, |this, cx| {
                                                        this.paste_images(clipboard, window, cx)
                                                    })
                                                })
                                                .readonly(self.busy || !owns || self.pending.is_some() || self.thread.is_none() || !self.draft_editable()),
                                        )
                                    })
                                    .child(
                                        div()
                                            .flex()
                                            .gap_2()
                                            .justify_end()
                                            .when(active_turn && !self.is_acp(), |row| row.child(
                                                Button::new("steer-turn")
                                                    .label("Send to active turn")
                                                    .disabled(steer_disabled || has_question)
                                                    .on_click(cx.listener(|this, _, window, cx| this.steer(window, cx)))
                                            ))
                                            .child(
                                                Button::new("send-message")
                                                    .label("Send")
                                                    .disabled(send_disabled || has_question)
                                                    .on_click(cx.listener(
                                                        |this, _, window, cx| this.send(window, cx),
                                                    )),
                                            )
                                            .when_some(
                                                self.thread.as_ref().and_then(|thread| {
                                                    thread.active_turn().map(|turn| {
                                                        (thread.id.clone(), turn.id.clone())
                                                    })
                                                }),
                                                |row, (thread_id, turn_id)| {
                                                    row.child(
                                                        Button::new("stop-turn")
                                                            .label(if cancel_requested { "Stopping…" } else if stopping.as_ref().is_some_and(|stop| stop.can_retry()) { "Retry Stop" } else { "Stop turn" })
                                                            .disabled(blocked || cancel_requested || !stop_supported)
                                                            .on_click(cx.listener(
                                                                move |this, _, window, cx| {
                                                                    this.command(
                                                                        Command::Stop {
                                                                            thread_id: thread_id
                                                                                .clone(),
                                                                            turn_id: turn_id
                                                                                .clone(),
                                                                        },
                                                                        window,
                                                                        cx,
                                                                    )
                                                                },
                                                            )),
                                                    )
                                                },
                                            ),
                                    ),
                            ),
                    ),
            )
    }
}
