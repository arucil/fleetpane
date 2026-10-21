//! Selection persistence is independent of Host control, session reads and drafts.
use super::*;
use fleetpane_session::host_selection::Saved;
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct State {
    saved: Option<Saved>,
    pub busy: bool,
    selected_during_load: bool,
    desired: Option<PathBuf>,
    pub error: Option<String>,
    notice: Option<String>,
}

impl Workbench {
    pub(super) fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.host_choice.selected_during_load = true;
        self.show_host(index, window, cx);
        self.remember_host(window, cx);
    }

    pub(super) fn load_host_choice(
        &mut self,
        startup: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.host_choice.busy {
            return;
        }
        let Some(directory) = self.creation_directory.clone() else {
            return;
        };
        self.host_choice.busy = true;
        self.host_choice.error = None;
        self.close_error = None;
        if !startup {
            self.host_choice.desired = None;
        }
        let task = cx.background_executor().spawn(async move {
            background(async move { Store::open(directory).await?.host_selection().await })
        });
        cx.spawn_in(window,async move |this,cx| {
            let result=task.await;
            let _closed=this.update_in(cx,|this,window,cx| {
                this.host_choice.busy=false;
                match result {
                    Ok(saved)=>{
                        if startup && !this.host_choice.selected_during_load && !this.sessions.active && !this.attention.active && !this.browsing_workspaces && !this.choosing {
                            let index=saved.configured_index(&this.configuration);
                            if saved.socket.is_some() && index.is_none() {
                                this.host_choice.notice=Some("Saved Host is absent from this configuration. The first Host is shown; the saved choice is retained.".into());
                            }
                            this.show_host(index.unwrap_or(0),window,cx);
                        }
                        this.host_choice.saved=Some(saved);
                        this.save_host_choice(window,cx);
                    }
                    Err(error)=>{
                        this.host_choice.error=Some(format!("Host choice could not be read: {error}"));
                        if startup && !this.host_choice.selected_during_load {this.show_host(0,window,cx);}
                    }
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    fn remember_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.host_choice.desired = Some(self.slots[self.selected].endpoint.socket.clone());
        self.host_choice.notice = None;
        self.save_host_choice(window, cx);
    }

    fn save_host_choice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.host_choice.busy || self.host_choice.error.is_some() {
            return;
        }
        let (Some(saved), Some(socket), Some(directory)) = (
            self.host_choice.saved.as_ref(),
            self.host_choice.desired.clone(),
            self.creation_directory.clone(),
        ) else {
            return;
        };
        if saved.socket.as_ref() == Some(&socket) {
            self.host_choice.desired = None;
            return;
        }
        let revision = saved.revision;
        self.host_choice.busy = true;
        let task = cx.background_executor().spawn(async move {
            background(async move {
                Store::open(directory)
                    .await?
                    .save_host_selection(revision, socket)
                    .await
            })
        });
        cx.spawn_in(window,async move |this,cx| {
            let result=task.await;
            let _closed=this.update_in(cx,|this,window,cx| {
                this.host_choice.busy=false;
                match result {
                    Ok(saved)=>{this.host_choice.saved=Some(saved);this.save_host_choice(window,cx);}
                    Err(error)=>{
                        this.host_choice.saved=None;
                        this.host_choice.error=Some(format!("Host choice was not saved: {error}. Reload, then choose Remember current Host explicitly."));
                    }
                }
                cx.notify();
            });
        }).detach();
    }

    pub(super) fn host_choice_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_4()
            .py_1()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .text_color(if self.host_choice.error.is_some() {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(
                        self.host_choice
                            .error
                            .clone()
                            .or_else(|| self.host_choice.notice.clone())
                            .unwrap_or_else(|| {
                                if self.host_choice.busy {
                                    "Saving / loading Host choice…".into()
                                } else if self.host_choice.saved.is_none() {
                                    "Host choice is not saved; reload before remembering this Host.".into()
                                } else {
                                    "Host choice stays local. Switching keeps sessions running."
                                        .into()
                                }
                            }),
                    ),
            )
            .when(self.host_choice.error.is_some(), |bar| bar.child(Button::new("host-choice-keep").small().ghost().label("Keep previous choice").disabled(self.host_choice.busy).on_click(cx.listener(|this,_,_,cx|{
                this.host_choice.error=None;
                this.host_choice.desired=None;
                this.host_choice.saved=None;
                this.host_choice.notice=Some("Host choice was not saved. Previous record retained; reload before remembering another choice.".into());
                this.close_error=None;
                cx.notify();
            }))))
            .child(
                Button::new("host-choice-reload")
                    .small()
                    .ghost()
                    .label("Reload choice")
                    .disabled(self.host_choice.busy)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.load_host_choice(false, window, cx)),
                    ),
            )
            .child(
                Button::new("host-choice-remember")
                    .small()
                    .ghost()
                    .label("Remember current Host")
                    .disabled(
                        self.host_choice.busy
                            || self.host_choice.saved.is_none()
                            || self.host_choice.error.is_some(),
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.remember_host(window, cx))),
            )
    }
}
