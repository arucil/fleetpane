use super::*;
use fleetpane_runtime::attachments::Image;
use fleetpane_session::{
    creations::Store,
    drafts::{Draft, Key, Status},
};

#[derive(Clone, PartialEq, Eq)]
struct Content {
    text: String,
    images: Vec<Image>,
}

pub(super) struct State {
    pub directory: PathBuf,
    pub record: Option<Draft>,
    loading: bool,
    saving: bool,
    saving_content: Option<Content>,
    dirty: Option<Content>,
    pub(super) importing: bool,
    pub(super) picking: bool,
    pub(super) images_error: Option<String>,
    error: Option<String>,
    notice: Option<String>,
}

impl State {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            record: None,
            loading: false,
            saving: false,
            saving_content: None,
            dirty: None,
            importing: false,
            picking: false,
            images_error: None,
            error: None,
            notice: None,
        }
    }
}

/// Hydrating the already-selected metadata is an observation, not navigation.
pub(super) fn observes_selected(command: &Command, thread: Option<&Thread>) -> bool {
    matches!(command, Command::Open {thread_id,resume:false}
        if thread.is_some_and(|thread| &thread.id == thread_id))
}

impl HostView {
    pub(crate) fn draft_close_status(&self) -> Result<bool, String> {
        if self.draft.dirty.is_some()
            && let Some(error) = &self.draft.error
        {
            return Err(error.clone());
        }
        Ok(!self.draft.importing
            && !self.draft.saving
            && self.draft.dirty.is_none()
            && !self.recent.busy)
    }

    pub(super) fn draft_can_navigate(&self) -> bool {
        !self.draft.loading
            && !self.draft.importing
            && !self.draft.picking
            && !self.draft.saving
            && self.draft.dirty.is_none()
            && self.draft.error.is_none()
    }

    pub(super) fn draft_editable(&self) -> bool {
        !self.draft.loading
            && self
                .draft
                .record
                .as_ref()
                .is_some_and(|record| record.attempt.is_none())
    }

    pub(super) fn draft_sendable(&self) -> bool {
        self.draft_can_navigate() && self.draft_editable()
    }

    pub(super) fn load_draft(&mut self, check: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(thread), Some(snapshot)) = (&self.thread, &self.snapshot) else {
            return;
        };
        let key = Key {
            socket: self.socket.clone(),
            host_id: snapshot.host_id,
            thread_id: thread.id.clone(),
            cwd: thread.cwd.clone(),
        };
        if !check
            && self
                .draft
                .record
                .as_ref()
                .is_some_and(|record| record.key == key)
        {
            return;
        }
        if self.draft.picking
            || self.draft.importing
            || self.draft.loading
            || self.draft.saving
            || self.draft.dirty.is_some()
        {
            return;
        }
        self.draft.loading = true;
        self.draft.error = None;
        self.draft.notice = None;
        self.draft.images_error = None;
        self.draft.record = None;
        let directory = self.draft.directory.clone();
        let task = cx.background_executor().spawn(async move {
            background(async move {
                let store = Store::open(directory).await?;
                let draft = store.draft(key).await?;
                if check {
                    store.check_draft(&draft).await
                } else {
                    Ok(fleetpane_session::drafts::Checked {
                        draft,
                        resolution: Status::Editing,
                    })
                }
            })
        });
        cx.spawn_in(window,async move|this,cx| {
            let result=task.await;
            let _closed=this.update_in(cx,|this,window,cx| {
                this.draft.loading=false;
                match result {
                    Ok(checked)=>{
                        this.draft.notice=Some(match checked.resolution {
                            Status::Editing if checked.draft.attempt.is_some()=>"Recovered send attempt. Check send status before editing or sending again.".into(),
                            Status::Editing=>"Draft saved locally".into(),
                            Status::Pending=>"Send is still pending. Check again later; it has not been resent.".into(),
                            Status::Sent{turn_id}=>format!("Send confirmed for turn {turn_id}. Reopen the session to refresh its history."),
                            Status::Rejected{error}=>format!("Send was rejected: {error}. Saved text is ready to edit."),
                            Status::Unknown{reason}=>format!("Send outcome is unknown: {reason}. Copy text for review; this attempt cannot be resent."),
                        });
                        let text=checked.draft.text.clone();
                        this.draft.record=Some(checked.draft);
                        this.composer.update(cx,|input,cx|input.set_value(text,window,cx));
                    }
                    Err(error)=>this.draft.error=Some(error.to_string()),
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    pub(super) fn edit_draft(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_content(
            Content {
                text,
                images: self.draft_images().to_vec(),
            },
            window,
            cx,
        );
    }

    pub(super) fn draft_images(&self) -> &[Image] {
        if let Some(content) = self
            .draft
            .dirty
            .as_ref()
            .or(self.draft.saving_content.as_ref())
        {
            &content.images
        } else {
            self.draft
                .record
                .as_ref()
                .map_or(&[], |draft| draft.images.as_slice())
        }
    }

    pub(super) fn edit_images(
        &mut self,
        images: Vec<Image>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_content(
            Content {
                text: self.composer.read(cx).value().to_string(),
                images,
            },
            window,
            cx,
        );
    }

    fn edit_content(&mut self, content: Content, window: &mut Window, cx: &mut Context<Self>) {
        let Some(record) = &self.draft.record else {
            return;
        };
        if self.draft.loading || record.attempt.is_some() {
            return;
        }
        let current = self
            .draft
            .dirty
            .as_ref()
            .or(self.draft.saving_content.as_ref());
        if current.is_some_and(|current| current == &content)
            || (current.is_none() && record.text == content.text && record.images == content.images)
        {
            return;
        }
        self.draft.dirty = Some(content);
        self.draft.notice = None;
        self.save_draft(window, cx);
    }

    fn save_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.draft.saving || self.draft.error.is_some() {
            return;
        }
        let (Some(record), Some(content)) = (self.draft.record.clone(), self.draft.dirty.take())
        else {
            return;
        };
        self.draft.saving = true;
        self.draft.saving_content = Some(content.clone());
        let directory = self.draft.directory.clone();
        let saving_content = content.clone();
        let task = cx.background_executor().spawn(async move {
            background(async move {
                Store::open(directory)
                    .await?
                    .edit_content(&record, content.text, content.images)
                    .await
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.draft.saving = false;
                this.draft.saving_content = None;
                match result {
                    Ok(record) => {
                        this.draft.record = Some(record);
                        if this.draft.dirty.is_some() {
                            this.save_draft(window, cx);
                        } else {
                            this.draft.notice = Some("Draft saved locally".into());
                        }
                    }
                    Err(error) => {
                        if this.draft.dirty.is_none() {
                            this.draft.dirty = Some(saving_content);
                        }
                        this.draft.error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn render_draft(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let copy = self.composer.read(cx).value().to_string();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .text_xs()
            .when(self.draft.loading, |view| {
                view.child("Loading saved draft…")
            })
            .when(self.draft.saving || self.draft.dirty.is_some(), |view| {
                view.child("Draft has unsaved changes…")
            })
            .children(
                self.draft
                    .notice
                    .iter()
                    .map(|notice| div().child(notice.clone())),
            )
            .when_some(self.draft.error.as_ref(), |view, error| {
                view.child(div().text_color(cx.theme().danger).child(error.clone()))
            })
            .child(
                div()
                    .flex()
                    .gap_2()
                    .when(
                        self.draft
                            .record
                            .as_ref()
                            .is_some_and(|record| record.attempt.is_some()),
                        |view| {
                            view.child(
                                Button::new("check-draft-send")
                                    .small()
                                    .label("Check send status")
                                    .disabled(self.draft.loading)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.load_draft(true, window, cx)
                                    })),
                            )
                        },
                    )
                    .when(self.draft.error.is_some(), |view| {
                        view.child(
                            Button::new("retry-draft-save")
                                .small()
                                .label("Retry draft save")
                                .disabled(self.draft.saving)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.draft.error = None;
                                    if this.draft.dirty.is_some() {
                                        this.save_draft(window, cx);
                                    } else {
                                        this.load_draft(false, window, cx);
                                    }
                                })),
                        )
                        .child(
                            Button::new("reload-saved-draft")
                                .small()
                                .label("Discard local edits and reload draft")
                                .disabled(self.draft.saving)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.draft.error = None;
                                    this.draft.dirty = None;
                                    this.draft.record = None;
                                    this.load_draft(false, window, cx);
                                })),
                        )
                    })
                    .when(
                        self.draft.error.is_some()
                            || self
                                .draft
                                .record
                                .as_ref()
                                .is_some_and(|record| record.attempt.is_some()),
                        |view| {
                            view.child(
                                Button::new("copy-recovered-draft")
                                    .small()
                                    .label("Copy text")
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            copy.clone(),
                                        ))
                                    }),
                            )
                        },
                    ),
            )
    }
}

#[cfg(test)]
mod hydration_tests {
    use super::{Command, Thread, observes_selected};
    #[test]
    fn same_thread_read_can_follow_metadata_while_draft_loads_without_relaxing_navigation() {
        let thread: Thread = serde_json::from_value(serde_json::json!({"id":"one","cwd":"/fixture","preview":"","status":{"type":"idle"},"turns":[]})).unwrap();
        let read = |id: &str, resume| Command::Open {
            thread_id: id.into(),
            resume,
        };
        assert!(observes_selected(&read("one", false), Some(&thread)));
        assert!(!observes_selected(&read("other", false), Some(&thread)));
        assert!(!observes_selected(&read("one", true), Some(&thread)));
        assert!(!observes_selected(&read("one", false), None));
        assert!(!observes_selected(
            &Command::Inspect {
                thread_id: "one".into(),
                resume: false
            },
            Some(&thread)
        ));
    }
}
