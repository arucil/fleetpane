//! Cross-Host directory reads are independent of retained conversation views.
use super::*;
use fleetpane_session::{
    directory::{PageRequest, Query},
    fleet_directory::{
        self, Group,
        order::{self, Order, Row},
    },
};

pub(super) struct State {
    pub active: bool,
    groups: Vec<Group>,
    search: Entity<InputState>,
    workspace: Entity<InputState>,
    query: Query,
    busy: bool,
    paging: Option<usize>,
    filter: Option<usize>,
    error: Option<String>,
    order: Order,
    rows: std::sync::Arc<Vec<Row>>,
    _subscription: Subscription,
    _workspace_subscription: Subscription,
}

impl State {
    pub fn new(window: &mut Window, cx: &mut Context<Workbench>) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Search session titles across Hosts")
        });
        let subscription = cx.subscribe_in(&search, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.refresh_sessions(window, cx);
            }
        });
        let workspace = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Exact workspace directory (optional)")
        });
        let workspace_subscription =
            cx.subscribe_in(&workspace, window, |this, _, event, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.refresh_sessions(window, cx);
                }
            });
        Self {
            active: false,
            groups: Vec::new(),
            search,
            workspace,
            query: Query::default(),
            busy: false,
            paging: None,
            filter: None,
            error: None,
            order: Order::default(),
            rows: std::sync::Arc::new(Vec::new()),
            _subscription: subscription,
            _workspace_subscription: workspace_subscription,
        }
    }

    fn rebuild_rows(&mut self) {
        self.rows = std::sync::Arc::new(order::rows(&self.groups, self.filter, self.order));
    }
}

impl Workbench {
    pub(super) fn open_sessions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.export_pending(cx) {
            return;
        }
        self.diagnostics.active = false;
        self.leave_attention();
        self.file_navigation = files::State::default();
        self.choosing = false;
        self.browsing_workspaces = false;
        self.route_notice = None;
        self.sessions.active = true;
        self.sessions
            .search
            .update(cx, |input, cx| input.focus(window, cx));
        self.refresh_sessions(window, cx);
        cx.notify();
    }

    fn refresh_sessions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.filter_sessions(self.sessions.query.archived, window, cx);
    }

    fn filter_sessions(&mut self, archived: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.sessions.busy || self.sessions.paging.is_some() {
            return;
        }
        let search = self.sessions.search.read(cx).value().trim().to_owned();
        let cwd = self.sessions.workspace.read(cx).value().trim().to_owned();
        let query = Query {
            search: (!search.is_empty()).then_some(search),
            cwd: (!cwd.is_empty()).then_some(cwd),
            archived,
        };
        if let Err(error) = query.validate() {
            self.sessions.error = Some(error.to_string());
            cx.notify();
            return;
        }
        self.sessions.busy = true;
        self.sessions.error = None;
        if self.sessions.groups.is_empty() {
            self.sessions.groups = self
                .configuration
                .hosts
                .iter()
                .cloned()
                .map(Group::new)
                .collect();
            self.sessions.rebuild_rows();
        }
        let configuration = self.configuration.clone();
        let request = PageRequest {
            query: query.clone(),
            cursor: None,
        };
        let task = cx
            .background_executor()
            .spawn(async move { background(fleet_directory::collect(configuration, query)) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, _, cx| {
                this.sessions.busy = false;
                match result {
                    Ok(reports) => {
                        for (group, report) in this.sessions.groups.iter_mut().zip(reports) {
                            let result = match report.observation {
                                Some(observation) => Ok(observation),
                                None => Err(Error::Input(
                                    report.error.expect("failed observation has an error"),
                                )),
                            };
                            group.apply(&request, result);
                        }
                        this.sessions.query = request.query;
                        this.sessions.rebuild_rows();
                    }
                    Err(error) => this.sessions.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn more_sessions(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.sessions.busy || self.sessions.paging.is_some() {
            return;
        }
        let group = &self.sessions.groups[index];
        let Some(request) = group.directory.next() else {
            return;
        };
        let endpoint = group.endpoint.clone();
        let host_id = group.host_id;
        self.sessions.paging = Some(index);
        let pending = request.clone();
        let task = cx.background_executor().spawn(async move {
            background(fleet_directory::read_page(&endpoint, host_id, &pending))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, _, cx| {
                this.sessions.paging = None;
                this.sessions.groups[index].apply(&request, result);
                this.sessions.rebuild_rows();
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn open_fleet_session(
        &mut self,
        index: usize,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sessions.busy || self.sessions.paging.is_some() {
            return;
        }
        let group = &self.sessions.groups[index];
        let source = match group.source(&id) {
            Ok(source) => source,
            Err(error) => {
                self.sessions.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        self.select(index, window, cx);
        self.slots[index]
            .view
            .as_ref()
            .expect("selected Host initialized")
            .update(cx, |view, cx| {
                view.prepare_directory_session(source, window, cx)
            });
    }

    pub(super) fn sessions_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let blocked = self.sessions.busy || self.sessions.paging.is_some();
        // Cached projection; painting does not sort, parse timestamps or perform I/O.
        let rows = self.sessions.rows.clone();
        div().flex_1().min_h_0().flex().flex_col()
            .child(div().p_4().flex().items_center().gap_2()
                .child(Button::new("sessions-back").small().label("Back to Host").on_click(cx.listener(|this,_,window,cx|this.select(this.selected,window,cx))))
                .child(div().flex_1().child(Input::new(&self.sessions.search)))
                .child(Button::new("sessions-refresh").small().label(if self.sessions.busy { "Checking…" } else { "Search / Refresh" }).disabled(blocked).on_click(cx.listener(|this,_,window,cx|this.refresh_sessions(window,cx)))))
            .child(div().px_4().pb_2().text_sm().child("All configured Hosts · metadata only · 100 per page · refresh explicitly. Opening never resumes or sends a message."))
            .child(div().px_4().pb_2().flex().items_center().gap_2()
                .children([(Order::Activity,"Activity"),(Order::Host,"By Host")].map(|(order,label)|Button::new(if order==Order::Activity{"session-order-activity"}else{"session-order-host"}).small().label(label).selected(self.sessions.order==order).on_click(cx.listener(move|this,_,_,cx|{this.sessions.order=order;this.sessions.rebuild_rows();cx.notify();}))))
                .child(div().text_xs().child(if self.sessions.busy { "Reading session metadata…" } else if self.sessions.order==Order::Activity {"Loaded rows only · active first · update time · Host summaries / paging below"}else{"Provider page order within each Host"})))
            .child(div().px_4().pb_2().flex().items_center().gap_2()
                .children([false,true].map(|archived| Button::new(if archived {"fleet-archived"}else{"fleet-unarchived"}).small().label(if archived {"Archived"}else{"Sessions"}).selected(self.sessions.query.archived==archived).disabled(blocked).on_click(cx.listener(move |this,_,window,cx|this.filter_sessions(archived,window,cx)))))
                .child(div().flex_1().min_w_0().child(Input::new(&self.sessions.workspace))))
            .child(div().px_4().pb_2().flex().flex_wrap().gap_2()
                .child(Button::new("sessions-all-hosts").small().ghost().label("All Hosts").selected(self.sessions.filter.is_none()).on_click(cx.listener(|this,_,_,cx|{this.sessions.filter=None;this.sessions.rebuild_rows();cx.notify();})))
                .children(self.sessions.groups.iter().enumerate().map(|(index,group)|Button::new(("sessions-host-filter",index)).small().ghost().label(format!("{} · {}{}",group.endpoint.name,group.directory.threads.len(),if group.error.is_some(){" !"}else{""})).selected(self.sessions.filter==Some(index)).on_click(cx.listener(move|this,_,_,cx|{this.sessions.filter=Some(index);this.sessions.rebuild_rows();cx.notify();})))))
            .child(div().px_4().pb_2().text_xs().truncate().child(format!("Applied: {} · title: {} · workspace: {}", if self.sessions.query.archived {"Archived"}else{"Sessions"}, self.sessions.query.search.as_deref().unwrap_or("all titles"), self.sessions.query.cwd.as_deref().unwrap_or("all directories"))))
            .children(self.sessions.error.iter().map(|error| div().px_4().text_color(cx.theme().danger).child(error.clone())))
            .child(uniform_list("fleet-sessions", rows.len(), cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range.map(|row| {
                    let entry = &rows[row];
                    let index = entry.group;
                    let group = &this.sessions.groups[index];
                    if let Some(thread_index) = entry.thread {
                        let thread = &group.directory.threads[thread_index];
                        let id = thread.id.clone();
                        div().h(px(104.)).px_4().py_2().border_b_1().border_color(cx.theme().border)
                            .child(div().truncate().child(format!("{}{} · {}", if group.can_open() {""} else {"Last observed · "}, group.endpoint.name, thread.title())))
                            .child(div().text_xs().truncate().child(format!("{} · {} · {}",thread.id,thread.status.kind,thread.cwd)))
                            .child(div().text_xs().truncate().child(entry.updated.clone()))
                            .child(Button::new(("fleet-session-open",row)).small().label("Open session").disabled(blocked || !group.can_open()).on_click(cx.listener(move|this,_,window,cx|this.open_fleet_session(index,id.clone(),window,cx))))
                    } else {
                        div().h(px(104.)).px_4().py_2().bg(cx.theme().secondary)
                            .child(div().truncate().child(format!("{} · {} loaded{}",group.endpoint.name,group.directory.threads.len(),if group.error.is_some(){" · stale / unavailable"}else{""})))
                            .child(div().text_xs().truncate().text_color(if group.error.is_some(){cx.theme().danger}else{cx.theme().muted_foreground}).child(group.error.clone().unwrap_or_else(||if this.sessions.busy {"Reading session metadata…".into()}else if group.directory.at_limit { "5,000-session limit; narrow the search".into() }else if group.directory.next_cursor.is_some(){"More sessions available".into()}else{"All matching sessions loaded".into()})))
                            .child(Button::new(("fleet-session-more",index)).small().label(if this.sessions.paging==Some(index){"Loading…"}else{"Load more"}).disabled(blocked || group.directory.next().is_none()).on_click(cx.listener(move|this,_,window,cx|this.more_sessions(index,window,cx))))
                    }
                }).collect()
            })).flex_1().min_h_0())
    }
}
