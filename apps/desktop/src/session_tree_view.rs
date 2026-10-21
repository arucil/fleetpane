//! On-demand observation, independent of conversation mutations and Stop.
use crate::host_view::background;
use fleetpane_session::session_tree::{self, View};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::{prelude::*, *};
use std::path::PathBuf;
use uuid::Uuid;

pub struct SessionTreeView {
    socket: PathBuf,
    host_id: Uuid,
    root: String,
    view: Option<View>,
    error: Option<String>,
    busy: bool,
    scroll: UniformListScrollHandle,
}
impl SessionTreeView {
    pub fn new(
        socket: PathBuf,
        host_id: Uuid,
        root: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            socket,
            host_id,
            root,
            view: None,
            error: None,
            busy: false,
            scroll: UniformListScrollHandle::new(),
        };
        this.refresh(window, cx);
        this
    }
    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        let socket = self.socket.clone();
        let host = self.host_id;
        let root = self.root.clone();
        let task = cx
            .background_executor()
            .spawn(async move { background(session_tree::inspect(&socket, host, &root)) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                match result {
                    Ok(view) => {
                        this.view = Some(view);
                        this.scroll.scroll_to_item(0, ScrollStrategy::Top);
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
impl Render for SessionTreeView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().flex().flex_col().min_w_0().px_4().py_2().gap_2().text_xs()
            .child(div().flex().gap_2().items_center()
                .child(div().flex_1().min_w_0().truncate().child(format!("Session tree · {}",self.root)))
                .child(Button::new("refresh-tree").small().ghost().label(if self.busy {"Inspecting…"}else{"Refresh tree"}).disabled(self.busy).on_click(cx.listener(|this,_,window,cx|this.refresh(window,cx))))
                .child(Button::new("copy-tree").small().ghost().label("Copy tree JSON").disabled(self.view.is_none()).on_click(cx.listener(|this,_,_,cx| {
                    if let Some(view)=&this.view {
                        match serde_json::to_string_pretty(view) {Ok(text)=>cx.write_to_clipboard(ClipboardItem::new_string(text)),Err(error)=>{this.error=Some(error.to_string());cx.notify();}}
                    }
                }))))
            .child("Observation only. Background resources are not enumerated; idle or unloaded is not proof that a session can be safely archived.")
            .when_some(self.error.clone(),|pane,error|pane.child(div().text_color(cx.theme().danger).child(format!("Inspection failed: {error}. Previous results, if shown, are stale."))))
            .when(self.busy && self.view.is_some(),|pane|pane.child("Refreshing; previous results remain visible."))
            .when_some(self.view.as_ref(),|pane,view| {
                let count=view.rows.len();
                pane.child(format!("{} nodes · {} scanned · {} other loaded · {} unassigned requests · {} mutations in flight · {} unknown operations · sequence {}–{}",count,view.scanned_nodes,view.other_loaded_contexts,view.unassigned_requests,view.in_flight_mutations,view.unknown_operations,view.sequence_before,view.sequence_after))
                    .when(!view.unresolved_same_session.is_empty(),|pane|pane.child(div().text_color(cx.theme().danger).child(format!("Incomplete parent chains: {} related sessions; copy JSON for identities.",view.unresolved_same_session.len()))))
                    .child(uniform_list("session-tree-nodes",count,cx.processor(|this,range:std::ops::Range<usize>,_,_cx| {
                        let Some(view)=&this.view else{return Vec::new();};
                        range.map(|n| {
                            let row=&view.rows[n];let id=row.metadata.id.clone();
                            let membership=match row.archived {Some(true)=>"archived",Some(false)=>"history",None=>"outside history"};
                            div().h(px(64.)).w_full().min_w_0().flex().items_center().gap_2()
                                .child(div().w(px((row.depth.min(8)*12) as f32)).flex_shrink_0())
                                .child(div().flex_1().min_w_0().flex().flex_col()
                                    .child(div().truncate().child(format!("{} · {}",row.metadata.title(),row.metadata.id)))
                                    .child(div().truncate().child(format!("depth {} · {} · {} · {} · {} pending{}",row.depth,row.metadata.status.kind,membership,if row.loaded {"loaded"}else{"unloaded"},row.pending_requests,if row.active_turn.is_some(){" · observed turn active"}else{""}))))
                                .child(Button::new(SharedString::from(format!("copy-tree-{id}"))).small().ghost().label("Copy ID").on_click(move |_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(id.clone()))))
                        }).collect()
                    })).track_scroll(&self.scroll).w_full().flex_shrink_0().h(px((count.min(2)*64) as f32)))
            })
    }
}
