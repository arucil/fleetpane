//! Explicit history navigation retains its tree source and revalidates metadata.
use super::*;
use crate::session_tree_view::{OpenSession, SessionTreeView};
use fleetpane_session::session_tree::Target;

impl HostView {
    pub(super) fn show_session_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(state), Some(thread)) = (&self.snapshot, &self.thread) else {
            return;
        };
        let socket = self.socket.clone();
        let host = state.host_id;
        let root = thread.id.clone();
        let view = cx.new(|cx| SessionTreeView::new(socket, host, root, window, cx));
        self.session_tree_subscription =
            Some(
                cx.subscribe_in(&view, window, |this, _, event: &OpenSession, window, cx| {
                    this.open_tree_target(
                        event.target.clone(),
                        Some(event.return_to.clone()),
                        window,
                        cx,
                    );
                }),
            );
        self.session_tree = Some(view);
        cx.notify();
    }

    fn open_tree_target(
        &mut self,
        target: Target,
        return_to: Option<Target>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tree_opening.is_some() || self.busy || self.pending.is_some() {
            return;
        }
        if !self.can_act() || !self.owns() {
            self.error =
                Some("Take control of this Host explicitly before opening session history.".into());
            cx.notify();
            return;
        }
        if !self.draft_can_navigate() {
            self.error = Some(
                "Finish saving or resolving the current draft before opening another session."
                    .into(),
            );
            cx.notify();
            return;
        }
        let Some(thread) = &self.thread else {
            return;
        };
        let original = thread.id.clone();
        let revision = self.revision;
        let token = Uuid::new_v4();
        self.tree_opening = Some(token);
        self.error = None;
        let socket = self.socket.clone();
        let task = cx.background_executor().spawn(async move {
            let result = background(target.confirm(&socket, return_to.as_ref()));
            (result, return_to)
        });
        cx.spawn_in(window,async move|this,cx| {
            let (result,return_to)=task.await;
            let _closed=this.update_in(cx,|this,window,cx| {
                if this.tree_opening!=Some(token) {return;}
                this.tree_opening=None;
                if this.revision!=revision || !this.thread.as_ref().is_some_and(|thread|thread.id==original) {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(source) if this.snapshot.as_ref().is_some_and(|state|state.host_id==source.host_id)
                        && this.can_act() && this.owns() && !this.busy && this.pending.is_none()
                        && this.draft_can_navigate()=> {
                        let return_to=return_to.filter(|target|target.thread_id!=source.thread_id);
                        this.prepare_directory_session(source,window,cx);
                        this.tree_return=return_to;
                    }
                    Ok(_)=>this.error=Some("Host, control, operation or draft state changed during tree inspection. Open the session again when ready.".into()),
                    Err(error)=>this.error=Some(error.to_string()),
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    pub(super) fn tree_navigation_banner(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(target) = &self.tree_return else {
            return div()
                .when(self.tree_opening.is_some(), |pane| {
                    pane.px_4().py_1().text_xs().child("Checking session tree…")
                })
                .into_any_element();
        };
        let target = target.clone();
        div()
            .px_4()
            .pb_2()
            .flex()
            .flex_wrap()
            .gap_2()
            .items_center()
            .text_xs()
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(format!("Tree source · {}", target.thread_id)),
            )
            .child(
                Button::new("return-tree-source")
                    .small()
                    .ghost()
                    .label(if self.tree_opening.is_some() {
                        "Checking tree…"
                    } else {
                        "Back to source session"
                    })
                    .disabled(
                        self.tree_opening.is_some()
                            || self.busy
                            || self.pending.is_some()
                            || !self.can_act()
                            || !self.draft_can_navigate(),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_tree_target(target.clone(), None, window, cx)
                    })),
            )
            .child(
                Button::new("dismiss-tree-source")
                    .small()
                    .ghost()
                    .label("Dismiss source")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.tree_return = None;
                        this.tree_opening = None;
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}
