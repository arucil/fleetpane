//! The queue observes all configured Hosts without initializing their conversation views.
mod next;
use super::*;
use fleetpane_session::attention::navigation::{Cursor, Filter, Movement};
use fleetpane_session::attention::{self, Item, Kind, Report};

pub(super) struct State {
    pub active: bool,
    pub(super) busy: bool,
    pub(super) report: Option<Report>,
    pub(super) items: Vec<Item>,
    pub(super) completions: Vec<attention::review::Item>,
    pub(super) show_completions: bool,
    pub(super) review_busy: bool,
    pub(super) error: Option<String>,
    pub(super) navigation_error: Option<String>,
    pub(super) opening: Option<uuid::Uuid>,
    pub(super) filter: Filter,
    cursor: Cursor,
    next_cursor: Cursor,
    pub(super) next_pending: Option<uuid::Uuid>,
    selection_notice: Option<String>,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
}

impl State {
    pub fn new(cx: &mut Context<Workbench>) -> Self {
        Self {
            active: false,
            busy: false,
            report: None,
            items: vec![],
            completions: vec![],
            show_completions: false,
            review_busy: false,
            error: None,
            navigation_error: None,
            opening: None,
            filter: Filter::default(),
            cursor: Cursor::default(),
            next_cursor: Cursor::default(),
            next_pending: None,
            selection_notice: None,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
        }
    }
}

impl Workbench {
    pub(super) fn leave_attention(&mut self) {
        self.attention.active = false;
        self.attention.opening = None;
        self.attention.next_pending = None;
    }

    pub(super) fn open_attention(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.export_pending(cx) {
            return;
        }
        self.diagnostics.active = false;
        self.sessions.active = false;
        self.file_navigation = files::State::default();
        self.choosing = false;
        self.browsing_workspaces = false;
        self.route_notice = None;
        self.attention.active = true;
        self.attention.navigation_error = None;
        self.attention.focus.focus(window, cx);
        self.refresh_attention(window, cx);
        cx.notify();
    }

    pub(super) fn refresh_attention(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.attention.busy || self.attention.review_busy {
            return;
        }
        self.attention.busy = true;
        let next_token = self.attention.next_pending;
        let configuration = self.configuration.clone();
        let directory = self.creation_directory.clone();
        let preferences = self.notifications.directory.clone();
        let task = cx.background_executor().spawn(async move {
            background(async move {
                let report = if let Some(directory) = directory {
                    let store = Store::open(directory).await?;
                    attention::collect_reviewed(configuration, &store).await?
                } else {
                    attention::collect(configuration).await?
                };
                let observations =
                    fleetpane_session::notifications::delivery::Observation::from_report(&report);
                let pending =
                    fleetpane_session::notifications::pending::Observation::from_report(&report);
                let (batch, pending_batch) = match preferences {
                    Ok(directory) => match Store::open(directory).await {
                        Ok(store) => {
                            let batch = store.claim_completion_alerts(observations).await;
                            let pending = store.claim_pending_alerts(pending).await;
                            (batch, pending)
                        }
                        Err(error) => {
                            let message = error.to_string();
                            (Err(error), Err(Error::Input(message)))
                        }
                    },
                    Err(error) => (Err(Error::Input(error.clone())), Err(Error::Input(error))),
                };
                Ok((report, batch, pending_batch))
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.attention.busy = false;
                match result {
                    Ok((mut report, batch, pending_batch)) => {
                        this.attention.items.clear();
                        this.attention.completions.clear();
                        for host in &mut report.hosts {
                            this.attention.items.append(&mut host.items);
                            this.attention.completions.append(&mut host.completions);
                        }
                        this.reconcile_attention();
                        this.attention.report = Some(report);
                        this.attention.error = None;
                        this.finish_completion_reminders(batch, window, cx);
                        this.finish_pending_reminders(pending_batch, window, cx);
                    }
                    Err(error) => {
                        this.attention.error = Some(error.to_string());
                        this.attention.cursor = Cursor::default();
                        this.attention.selection_notice =
                            Some("Selection cleared: queue refresh failed.".into());
                    }
                }
                if this.attention.active && this.attention.next_pending.is_some() {
                    if this.attention.next_pending != next_token {
                        // The gesture arrived during an older read. Observe again
                        // once, so it cannot navigate from a pre-gesture snapshot.
                        this.refresh_attention(window, cx);
                    } else {
                        this.finish_next_request(window, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn open_attention_item(&mut self, item: Item, window: &mut Window, cx: &mut Context<Self>) {
        if self.attention.opening.is_some() || self.attention.error.is_some() {
            return;
        }
        self.attention.next_pending = None;
        self.attention.cursor.select(&item);
        let token = uuid::Uuid::new_v4();
        self.attention.opening = Some(token);
        self.attention.navigation_error = None;
        let checked = item.clone();
        let task = cx
            .background_executor()
            .spawn(async move { background(checked.confirm()) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                if this.attention.opening != Some(token) || !this.attention.active {
                    return;
                }
                this.attention.opening = None;
                if this.next_request_blocked(window, cx) || this.export_pending(cx) {
                    this.attention.navigation_error = Some("Navigation cancelled while another operation is active. Choose the request again.".into());
                    cx.notify();
                    return;
                }
                match result {
                    Ok(()) => {
                        let index = this
                            .slots
                            .iter()
                            .position(|slot| slot.endpoint.socket == item.endpoint.socket)
                            .expect("configured attention Host");
                        this.select(index, window, cx);
                        this.slots[index]
                            .view
                            .as_ref()
                            .expect("selected Host initialized")
                            .update(cx, |view, cx| view.prepare_attention(item, window, cx));
                    }
                    Err(error) => this.attention.navigation_error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn reconcile_attention(&mut self) {
        if self
            .attention
            .cursor
            .reconcile(&self.attention.items, &self.attention.filter)
        {
            self.attention.selection_notice = Some("Selection cleared: request changed, became unavailable or was filtered out. Choose a pending request.".into());
            self.attention.scroll.scroll_to_item(0, ScrollStrategy::Top);
        }
    }

    fn attention_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // This handle belongs only to the queue; text inputs and terminal keys are separate.
        if !self.attention.focus.is_focused(window) {
            return;
        }
        if self.attention.show_completions {
            return;
        }
        let key = &event.keystroke;
        let mods = key.modifiers;
        if mods.alt || mods.platform || mods.shift {
            return;
        }
        if (!mods.control && matches!(key.key.as_str(), "up" | "down" | "home" | "end" | "enter"))
            || (mods.control && key.key == "r")
        {
            self.attention.next_pending = None;
            self.attention.opening = None;
        }
        if mods.control {
            match key.key.as_str() {
                "r" => {
                    cx.stop_propagation();
                    self.refresh_attention(window, cx);
                }
                "c" => {
                    cx.stop_propagation();
                    if let Some(index) = self
                        .attention
                        .cursor
                        .index(&self.attention.items, &self.attention.filter)
                    {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            serde_json::to_string_pretty(&self.attention.items[index].request)
                                .expect("JSON request serializes"),
                        ));
                    }
                }
                _ => {}
            }
            return;
        }
        let movement = match key.key.as_str() {
            "down" => Some(Movement::Next),
            "up" => Some(Movement::Previous),
            "home" => Some(Movement::First),
            "end" => Some(Movement::Last),
            "enter" => {
                cx.stop_propagation();
                if let Some(index) = self
                    .attention
                    .cursor
                    .index(&self.attention.items, &self.attention.filter)
                {
                    self.open_attention_item(self.attention.items[index].clone(), window, cx);
                }
                return;
            }
            _ => None,
        };
        if let Some(movement) = movement {
            cx.stop_propagation();
            if self.attention.error.is_some() {
                return;
            }
            self.attention
                .cursor
                .navigate(&self.attention.items, &self.attention.filter, movement);
            self.attention.selection_notice = None;
            if let Some(index) = self
                .attention
                .cursor
                .index(&self.attention.items, &self.attention.filter)
            {
                let row = self.attention.items[..index]
                    .iter()
                    .filter(|item| self.attention.filter.matches(item))
                    .count();
                self.attention
                    .scroll
                    .scroll_to_item(row, ScrollStrategy::Center);
            }
            cx.notify();
        }
    }

    pub(super) fn attention_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.attention.show_completions {
            return self.completion_view(cx).into_any_element();
        }
        let indices: Vec<_> = self
            .attention
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| self.attention.filter.matches(item))
            .map(|(i, _)| i)
            .collect();
        let selected = self
            .attention
            .cursor
            .index(&self.attention.items, &self.attention.filter);
        div().flex_1().min_h_0().flex().flex_col()
            .track_focus(&self.attention.focus)
            .on_key_down(cx.listener(Self::attention_key))
            .on_mouse_down(MouseButton::Left,cx.listener(|this,_,window,cx|{this.attention.next_pending=None;this.attention.opening=None;this.attention.focus.focus(window,cx);cx.notify();}))
            .child(div().p_4().flex().flex_wrap().gap_3().items_center()
                .child(Button::new("attention-back").small().label("Back to Host").on_click(cx.listener(|this,_,window,cx|{this.select(this.selected,window,cx);})))
                .child(format!("Observed pending requests · {}",self.attention.items.len()))
                .child(Button::new("attention-completions").small().label(format!("Turn results · {} alerts",self.result_alert_count())).on_click(cx.listener(|this,_,_,cx|{this.attention.show_completions=true;cx.notify();})))
                .child(Button::new("attention-refresh").small().label(if self.attention.busy{"Checking…"}else{"Refresh"}).disabled(self.attention.busy).on_click(cx.listener(|this,_,window,cx|this.refresh_attention(window,cx)))))
            .child(div().px_4().pb_2().text_sm().child("Checks all configured Hosts every 2 seconds while this window is open. Review and answer in the original session."))
            .child(div().px_4().pb_2().text_sm().child(if cfg!(target_os="macos"){ "↑ / ↓ · Home / End select · Enter opens source · Ctrl+C copies · Ctrl+R refreshes · Cmd+Shift+J next across all Hosts" }else{ "↑ / ↓ · Home / End select · Enter opens source · Ctrl+C copies · Ctrl+R refreshes · Ctrl+Shift+J next across all Hosts" }))
            .child(div().px_4().pb_2().flex().flex_wrap().gap_2().children([(None,"All"),(Some(Kind::Approval),"Approvals"),(Some(Kind::Question),"Questions"),(Some(Kind::Other),"Other")].into_iter().enumerate().map(|(i,(kind,label))|Button::new(("attention-filter",i)).small().ghost().label(label).selected(self.attention.filter.kind==kind).on_click(cx.listener(move|this,_,window,cx|{this.attention.filter.kind=kind;this.reconcile_attention();this.attention.focus.focus(window,cx);cx.notify();})))))
            .child(div().px_4().pb_2().flex().flex_wrap().gap_2()
                .child(Button::new("attention-all-hosts").small().ghost().label("All Hosts").selected(self.attention.filter.socket.is_none()).on_click(cx.listener(|this,_,window,cx|{this.attention.filter.socket=None;this.reconcile_attention();this.attention.focus.focus(window,cx);cx.notify();})))
                .children(self.slots.iter().enumerate().map(|(index,slot)|{
                    let socket=slot.endpoint.socket.clone();
                    Button::new(("attention-host",index)).small().ghost().label(slot.endpoint.name.clone()).selected(self.attention.filter.socket.as_ref()==Some(&socket)).on_click(cx.listener(move|this,_,window,cx|{this.attention.filter.socket=Some(socket.clone());this.reconcile_attention();this.attention.focus.focus(window,cx);cx.notify();}))
                })))
            .when_some(self.attention.report.as_ref(),|view,report|view.child(div().id("attention-host-status").px_4().max_h_24().overflow_y_scroll().text_sm().children(report.hosts.iter().map(|host|div().text_color(if host.error.is_some(){cx.theme().danger}else{cx.theme().muted_foreground}).child(format!("{}: {}",host.endpoint.name,host.error.as_deref().unwrap_or("observed")))))))
            .children(self.attention.error.iter().chain(self.attention.navigation_error.iter()).map(|error|div().px_4().py_2().text_color(cx.theme().danger).child(error.clone())))
            .children(self.attention.selection_notice.iter().map(|notice|div().px_4().py_2().text_sm().child(notice.clone())))
            .when(self.attention.report.is_some()&&indices.is_empty(),|view|view.child(div().p_4().child("No matching pending requests in the available snapshots. Check unavailable Hosts above.")))
            .child(uniform_list("attention-items",indices.len(),cx.processor(move|this,range:std::ops::Range<usize>,_,cx|{
                range.map(|row|{
                    let item=&this.attention.items[indices[row]];
                    let open=item.clone();
                    let raw=item.request.clone();
                    let select=item.clone();
                    div().h(px(144.)).px_4().py_3().border_b_1().border_color(cx.theme().border)
                        .when(selected==Some(indices[row]),|view|view.bg(cx.theme().secondary).border_l_4().border_color(cx.theme().primary))
                        .on_mouse_down(MouseButton::Left,cx.listener(move|this,_,window,cx|{this.attention.cursor.select(&select);this.attention.selection_notice=None;this.attention.focus.focus(window,cx);cx.notify();}))
                        .child(div().truncate().child(format!("{}{} · {:?} · {}",if selected==Some(indices[row]){"Selected · "}else{""},item.endpoint.name,item.kind,item.title)))
                        .child(div().text_sm().truncate().child(format!("{} · Session: {} · Request: {} · {}",this.attention.report.as_ref().map_or_else(||"Observation time unavailable".into(),|r|item.observation_label(r.observed_unix_ms)),item.thread_id.as_deref().unwrap_or("no session target"),item.id,this.pending_alert_status(item))))
                        .child(div().text_xs().truncate().child((match item.kind {Kind::Approval=>"Waiting for your decision",Kind::Question=>"Waiting for your answer",Kind::Other=>item.method.as_str()}).to_owned()))
                        .child(div().flex().gap_2()
                            .child(Button::new(("attention-open",row)).small().label("Open source").disabled(item.thread_id.is_none()||this.attention.opening.is_some()||this.attention.error.is_some()).on_click(cx.listener(move|this,_,window,cx|this.open_attention_item(open.clone(),window,cx))))
                            .child(this.session_mute_button(format!("attention-mute-{row}").into(), item.thread_id.as_ref().map(|id| fleetpane_session::notifications::Session {socket:item.endpoint.socket.clone(),host_id:item.host_id,thread_id:id.clone()}),cx))
                            .child(Button::new(("attention-copy",row)).small().ghost().label("Copy original request").on_click(move|_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(serde_json::to_string_pretty(&raw).expect("JSON request serializes"))))))
                }).collect()
            })).track_scroll(&self.attention.scroll).flex_1().min_h_0()).into_any_element()
    }
}
