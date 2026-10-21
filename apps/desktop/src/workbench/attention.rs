//! The queue observes all configured Hosts without initializing their conversation views.
use super::*;
use fleetpane_session::attention::{self, Item, Kind, Report};

#[derive(Default)]
pub(super) struct State {
    pub active: bool,
    busy: bool,
    report: Option<Report>,
    items: Vec<Item>,
    error: Option<String>,
    navigation_error: Option<String>,
    opening: Option<uuid::Uuid>,
    filter: Option<Kind>,
}

impl Workbench {
    pub(super) fn leave_attention(&mut self) {
        self.attention.active = false;
        self.attention.opening = None;
    }

    pub(super) fn open_attention(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sessions.active = false;
        self.file_navigation = files::State::default();
        self.choosing = false;
        self.browsing_workspaces = false;
        self.route_notice = None;
        self.attention.active = true;
        self.attention.navigation_error = None;
        window.blur(cx);
        self.refresh_attention(window, cx);
        cx.notify();
    }

    pub(super) fn refresh_attention(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.attention.busy {
            return;
        }
        self.attention.busy = true;
        let configuration = self.configuration.clone();
        let task = cx
            .background_executor()
            .spawn(async move { background(attention::collect(configuration)) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, _, cx| {
                this.attention.busy = false;
                match result {
                    Ok(mut report) => {
                        this.attention.items.clear();
                        for host in &mut report.hosts {
                            this.attention.items.append(&mut host.items);
                        }
                        this.attention.report = Some(report);
                        this.attention.error = None;
                    }
                    Err(error) => this.attention.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn open_attention_item(&mut self, item: Item, window: &mut Window, cx: &mut Context<Self>) {
        if self.attention.opening.is_some() {
            return;
        }
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

    pub(super) fn attention_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let indices: Vec<_> = self
            .attention
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| self.attention.filter.is_none_or(|kind| kind == item.kind))
            .map(|(i, _)| i)
            .collect();
        div().flex_1().min_h_0().flex().flex_col()
            .child(div().p_4().flex().flex_wrap().gap_3().items_center()
                .child(Button::new("attention-back").small().label("Back to Host").on_click(cx.listener(|this,_,window,cx|{this.select(this.selected,window,cx);})))
                .child(format!("Observed pending requests · {}",self.attention.items.len()))
                .child(Button::new("attention-refresh").small().label(if self.attention.busy{"Checking…"}else{"Refresh"}).disabled(self.attention.busy).on_click(cx.listener(|this,_,window,cx|this.refresh_attention(window,cx)))))
            .child(div().px_4().pb_2().text_sm().child("Checks all configured Hosts every 2 seconds while open. Review and answer in the original session."))
            .child(div().px_4().pb_2().flex().gap_2().children([(None,"All"),(Some(Kind::Approval),"Approvals"),(Some(Kind::Question),"Questions"),(Some(Kind::Other),"Other")].into_iter().enumerate().map(|(i,(kind,label))|Button::new(("attention-filter",i)).small().ghost().label(label).selected(self.attention.filter==kind).on_click(cx.listener(move|this,_,_,cx|{this.attention.filter=kind;cx.notify();})))))
            .when_some(self.attention.report.as_ref(),|view,report|view.child(div().id("attention-host-status").px_4().max_h_24().overflow_y_scroll().text_sm().children(report.hosts.iter().map(|host|div().text_color(if host.error.is_some(){cx.theme().danger}else{cx.theme().muted_foreground}).child(format!("{}: {}",host.endpoint.name,host.error.as_deref().unwrap_or("observed")))))))
            .children(self.attention.error.iter().chain(self.attention.navigation_error.iter()).map(|error|div().px_4().py_2().text_color(cx.theme().danger).child(error.clone())))
            .when(self.attention.report.is_some()&&indices.is_empty(),|view|view.child(div().p_4().child("No matching pending requests in the available snapshots. Check unavailable Hosts above.")))
            .child(uniform_list("attention-items",indices.len(),cx.processor(move|this,range:std::ops::Range<usize>,_,cx|{
                range.map(|row|{
                    let item=&this.attention.items[indices[row]];
                    let open=item.clone();
                    let raw=item.request.clone();
                    div().h(px(144.)).px_4().py_3().border_b_1().border_color(cx.theme().border)
                        .child(div().truncate().child(format!("{} · {:?} · {}",item.endpoint.name,item.kind,item.title)))
                        .child(div().text_sm().truncate().child(format!("Session: {} · Request: {}",item.thread_id.as_deref().unwrap_or("no session target"),item.id)))
                        .child(div().text_xs().truncate().child((match item.kind {Kind::Approval=>"Waiting for your decision",Kind::Question=>"Waiting for your answer",Kind::Other=>item.method.as_str()}).to_owned()))
                        .child(div().flex().gap_2()
                            .child(Button::new(("attention-open",row)).small().label("Open source").disabled(item.thread_id.is_none()||this.attention.opening.is_some()).on_click(cx.listener(move|this,_,window,cx|this.open_attention_item(open.clone(),window,cx))))
                            .child(Button::new(("attention-copy",row)).small().ghost().label("Copy original request").on_click(move|_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(serde_json::to_string_pretty(&raw).expect("JSON request serializes"))))))
                }).collect()
            })).flex_1().min_h_0())
    }
}
