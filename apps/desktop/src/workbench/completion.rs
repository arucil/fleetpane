//! Completed turns are reviewed locally; pending approvals remain actionable.
use super::*;
use fleetpane_session::attention::review::Item;

impl Workbench {
    fn review_turn_results(
        &mut self,
        items: Vec<Item>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.attention.review_busy || self.attention.busy || items.is_empty() {
            return;
        }
        let Some(directory) = self.creation_directory.clone() else {
            self.attention.navigation_error = Some(
                "Local controller storage is unavailable; results have not been marked reviewed."
                    .into(),
            );
            cx.notify();
            return;
        };
        self.attention.review_busy = true;
        self.attention.navigation_error = None;
        let task = cx.background_executor().spawn(async move {
            background(async move {
                Store::open(directory)
                    .await?
                    .review_completions(items)
                    .await
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.attention.review_busy = false;
                match result {
                    Ok(()) => this.refresh_attention(window, cx),
                    Err(error) => this.attention.navigation_error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn open_turn_result(&mut self, item: Item, window: &mut Window, cx: &mut Context<Self>) {
        if self.attention.opening.is_some() || self.attention.error.is_some() {
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
                if !this.attention.active || this.attention.opening != Some(token) {
                    return;
                }
                this.attention.opening = None;
                match result {
                    Ok(()) => {
                        let index = this
                            .slots
                            .iter()
                            .position(|slot| slot.endpoint.socket == item.endpoint.socket)
                            .expect("configured completion Host");
                        this.select(index, window, cx);
                        this.slots[index]
                            .view
                            .as_ref()
                            .expect("selected Host initialized")
                            .update(cx, |view, cx| view.prepare_completion(item, window, cx));
                    }
                    Err(error) => this.attention.navigation_error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn completion_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let indices: Vec<_> = self
            .attention
            .completions
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                self.attention
                    .filter
                    .socket
                    .as_ref()
                    .is_none_or(|socket| socket == &item.endpoint.socket)
            })
            .map(|(i, _)| i)
            .collect();
        let unread: Vec<_> = indices
            .iter()
            .map(|i| &self.attention.completions[*i])
            .filter(|item| !item.reviewed)
            .cloned()
            .collect();
        let count = unread.len();
        div().flex_1().min_h_0().flex().flex_col()
            .child(div().p_4().flex().flex_wrap().gap_2().items_center()
                .child(Button::new("results-pending").small().label("Pending requests").on_click(cx.listener(|this,_,_,cx|{this.attention.show_completions=false;cx.notify();})))
                .child(format!("Turn results · {count} unreviewed · {} alerts",self.result_alert_count()))
                .child(Button::new("results-refresh").small().label("Refresh").disabled(self.attention.busy||self.attention.review_busy).on_click(cx.listener(|this,_,window,cx|this.refresh_attention(window,cx))))
                .child(Button::new("results-review-visible").small().label("Mark shown results reviewed").disabled(count==0||self.attention.busy||self.attention.review_busy||self.attention.error.is_some()).on_click(cx.listener(move|this,_,window,cx|this.review_turn_results(unread.clone(),window,cx)))))
            .child(div().px_4().pb_2().text_sm().child("Reviewing keeps the original outcome and never answers a pending request. Records cover this Host run; older results may have been evicted."))
            .child(div().px_4().pb_2().flex().flex_wrap().gap_2()
                .child(Button::new("results-all-hosts").small().ghost().label("All Hosts").selected(self.attention.filter.socket.is_none()).on_click(cx.listener(|this,_,_,cx|{this.attention.filter.socket=None;cx.notify();})))
                .children(self.slots.iter().enumerate().map(|(i,slot)|{
                    let socket=slot.endpoint.socket.clone();
                    Button::new(("results-host",i)).small().ghost().label(slot.endpoint.name.clone()).selected(self.attention.filter.socket.as_ref()==Some(&socket)).on_click(cx.listener(move|this,_,_,cx|{this.attention.filter.socket=Some(socket.clone());cx.notify();}))
                })))
            .when_some(self.attention.report.as_ref(),|view,report|view.child(div().id("results-host-status").px_4().max_h_24().overflow_y_scroll().text_xs().children(report.hosts.iter().map(|host|div().child(format!("{}: {}",host.endpoint.name,host.error.clone().unwrap_or_else(||match host.completion_evicted { Some(evicted)=>format!("latest 64 results at most · {evicted} evicted"),None=>"completion feed unavailable on this Host".into() })))))))
            .children(self.attention.error.iter().chain(self.attention.navigation_error.iter()).map(|error|div().px_4().py_2().text_color(cx.theme().danger).child(error.clone())))
            .when(indices.is_empty(),|view|view.child(div().p_4().child("No retained turn results for these Hosts. Check availability above.")))
            .child(uniform_list("completion-items",indices.len(),cx.processor(move|this,range:std::ops::Range<usize>,_,cx|{
                range.map(|row|{
                    let item=&this.attention.completions[indices[row]];
                    let open=item.clone();let review=item.clone();
                    let raw=serde_json::to_string_pretty(item).expect("completion serializes");
                    div().h(px(144.)).px_4().py_3().border_b_1().border_color(cx.theme().border)
                        .child(div().truncate().text_color(if item.record.kind==fleetpane_runtime::attention::Kind::Failed{cx.theme().danger}else{cx.theme().foreground}).child(format!("{} · {} · {}",item.endpoint.name,item.label(),this.result_alert_status(item))))
                        .child(div().text_sm().truncate().child(format!("Session {} · Turn {}",item.record.thread_id,item.record.turn_id)))
                        .child(div().text_xs().truncate().child(format!("Observed Unix ms {} · event {}",item.record.observed_unix_ms,item.record.sequence)))
                        .child(div().flex().gap_2()
                            .child(Button::new(("result-open",row)).small().label("Open source").disabled(this.attention.opening.is_some()||this.attention.error.is_some()).on_click(cx.listener(move|this,_,window,cx|this.open_turn_result(open.clone(),window,cx))))
                            .child(Button::new(("result-review",row)).small().label("Mark reviewed").disabled(item.reviewed||this.attention.busy||this.attention.review_busy||this.attention.error.is_some()).on_click(cx.listener(move|this,_,window,cx|this.review_turn_results(vec![review.clone()],window,cx))))
                            .child(this.session_mute_button(format!("result-mute-{row}").into(),Some(fleetpane_session::notifications::Session {socket:item.endpoint.socket.clone(),host_id:item.host_id,thread_id:item.record.thread_id.clone()}),cx))
                            .child(Button::new(("result-copy",row)).small().ghost().label("Copy record").on_click(move|_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(raw.clone())))))
                }).collect()
            })).flex_1().min_h_0())
    }
}
