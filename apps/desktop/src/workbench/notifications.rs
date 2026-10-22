//! Local alert preferences never acquire a Host lease or filter the true queue.
use super::*;
use fleetpane_session::notifications::{Kind, Preferences, Saved, Session};
const CLOSE_ERROR: &str = "Alert preferences were not saved. Reload alerts before closing.";

pub(super) struct State {
    directory: Result<std::path::PathBuf, String>,
    saved: Option<Saved>,
    pub busy: bool,
    refreshing: bool,
    open: bool,
    error: Option<String>,
    save_failed: bool,
}

impl State {
    pub fn new(directory: Result<std::path::PathBuf, String>) -> Self {
        Self {
            directory,
            saved: None,
            busy: false,
            refreshing: false,
            open: false,
            error: None,
            save_failed: false,
        }
    }
    fn editable(&self) -> bool {
        self.saved.is_some() && !self.busy && self.error.is_none()
    }
    fn observe(&mut self, saved: Saved) {
        if self
            .saved
            .as_ref()
            .is_none_or(|current| current.revision <= saved.revision)
        {
            self.saved = Some(saved);
        }
    }
    fn finish_read(&mut self, explicit: bool, result: Result<Saved, Error>) {
        self.refreshing = false;
        if explicit {
            self.busy = false;
        }
        match result {
            Ok(saved) => {
                self.observe(saved);
                if explicit {
                    self.error = None;
                    self.save_failed = false;
                }
            }
            Err(error) => {
                self.error = Some(format!("Notification preferences unavailable: {error}"))
            }
        }
    }
    pub(super) fn close_status(&self) -> Result<bool, String> {
        if self.save_failed {
            return Err(CLOSE_ERROR.into());
        }
        Ok(!self.busy)
    }
}

impl Workbench {
    pub(super) fn pending_alert_status(
        &self,
        item: &fleetpane_session::attention::Item,
    ) -> &'static str {
        match self
            .notifications
            .saved
            .as_ref()
            .filter(|_| self.notifications.error.is_none())
        {
            None => "Alert policy unavailable",
            Some(saved) if saved.value.pending_allowed(item) => "Included in alert count",
            Some(_) => "Muted · retained in queue",
        }
    }

    pub(super) fn result_alert_status(
        &self,
        item: &fleetpane_session::attention::review::Item,
    ) -> &'static str {
        match self
            .notifications
            .saved
            .as_ref()
            .filter(|_| self.notifications.error.is_none())
        {
            None => "Alert policy unavailable",
            Some(_) if item.reviewed => "Reviewed",
            Some(saved) if saved.value.result_allowed(item) => "Included in alert count",
            Some(_) => "Muted · unreviewed",
        }
    }

    pub(super) fn result_alert_count(&self) -> String {
        self.notifications
            .saved
            .as_ref()
            .filter(|_| self.notifications.error.is_none())
            .map_or_else(
                || "unavailable".into(),
                |s| {
                    self.attention
                        .completions
                        .iter()
                        .filter(|i| s.value.result_allowed(i))
                        .count()
                        .to_string()
                },
            )
    }
    pub(super) fn load_notifications(
        &mut self,
        explicit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.notifications.busy
            || self.notifications.refreshing
            || self.closing
            || (!explicit && self.notifications.error.is_some())
        {
            return;
        }
        let directory = match &self.notifications.directory {
            Ok(directory) => directory.clone(),
            Err(error) => {
                self.notifications.error = Some(error.clone());
                cx.notify();
                return;
            }
        };
        self.notifications.busy = explicit;
        self.notifications.refreshing = true;
        let task = cx.background_executor().spawn(async move {
            background(async move { Store::open(directory).await?.notifications().await })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, _, cx| {
                if explicit
                    && result.is_ok()
                    && this.close_error.as_deref()
                        == Some(format!("Window remains open: {CLOSE_ERROR}").as_str())
                {
                    this.close_error = None;
                }
                this.notifications.finish_read(explicit, result);
                cx.notify();
            });
        })
        .detach();
    }

    fn save_notifications(
        &mut self,
        value: Preferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.notifications.editable() || self.closing {
            return;
        }
        let saved = self
            .notifications
            .saved
            .as_ref()
            .expect("editable preferences");
        let revision = saved.revision;
        let directory = self
            .notifications
            .directory
            .as_ref()
            .expect("loaded directory")
            .clone();
        self.notifications.busy = true;
        let task = cx.background_executor().spawn(async move {
            background(async move {
                Store::open(directory)
                    .await?
                    .save_notifications(revision, value)
                    .await
            })
        });
        cx.spawn_in(window,async move |this,cx| {
            let result=task.await;
            let _closed=this.update_in(cx,|this,_,cx| {
                this.notifications.busy=false;
                match result {
                    Ok(saved) => this.notifications.observe(saved),
                    Err(error) => {this.notifications.save_failed=true;this.notifications.error=Some(format!("Notification preferences not saved: {error}. Reload before trying again."));}
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    pub(super) fn attention_label(&self) -> String {
        let Some(saved) = self
            .notifications
            .saved
            .as_ref()
            .filter(|_| self.notifications.error.is_none())
        else {
            return "Attention · alerts unavailable".into();
        };
        if saved.value.muted {
            return "Attention · muted".into();
        }
        if self.attention.error.is_some()
            || self
                .attention
                .report
                .as_ref()
                .is_none_or(|r| r.hosts.iter().any(|h| h.error.is_some()))
        {
            return "Attention · checking availability".into();
        }
        let count = self
            .attention
            .items
            .iter()
            .filter(|i| saved.value.pending_allowed(i))
            .count()
            + self
                .attention
                .completions
                .iter()
                .filter(|i| saved.value.result_allowed(i))
                .count();
        format!("Attention · {count}")
    }

    pub(super) fn session_mute_button(
        &self,
        id: SharedString,
        session: Option<Session>,
        cx: &mut Context<Self>,
    ) -> Button {
        let muted = session.as_ref().is_some_and(|s| {
            self.notifications
                .saved
                .as_ref()
                .is_some_and(|p| p.value.muted_sessions.contains(s))
        });
        Button::new(id)
            .small()
            .ghost()
            .label(if muted {
                "Unmute session"
            } else {
                "Mute session"
            })
            .disabled(session.is_none() || !self.notifications.editable() || self.closing)
            .on_click(cx.listener(move |this, _, window, cx| {
                let Some(session) = session.clone() else {
                    return;
                };
                let Some(saved) = &this.notifications.saved else {
                    return;
                };
                let mut value = saved.value.clone();
                if value.muted_sessions.contains(&session) {
                    value.muted_sessions.retain(|s| s != &session);
                } else {
                    value.muted_sessions.push(session);
                }
                this.save_notifications(value, window, cx);
            }))
    }

    pub(super) fn notifications_button(&self, cx: &mut Context<Self>) -> Button {
        let label = if self.notifications.busy {
            "Alerts · saving / loading"
        } else if self.notifications.error.is_some() {
            "Alerts · unavailable"
        } else if self
            .notifications
            .saved
            .as_ref()
            .is_some_and(|s| s.value.muted)
        {
            "Alerts · muted"
        } else {
            "Alerts"
        };
        Button::new("notifications-open")
            .small()
            .ghost()
            .label(label)
            .selected(self.notifications.open)
            .on_click(cx.listener(|this, _, _, cx| {
                this.notifications.open = !this.notifications.open;
                cx.notify();
            }))
    }

    pub(super) fn notification_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.notifications.saved.as_ref().map(|s| s.value.clone());
        let editable = self.notifications.editable() && !self.closing;
        let muted = selected.as_ref().is_some_and(|p| p.muted);
        div()
            .when(self.notifications.open,|view|view.child(div().id("notification-settings").max_h_48().overflow_y_scroll().px_4().pb_2().text_sm()
                .child(div().flex().flex_wrap().gap_2().items_center()
                    .child(Button::new("notifications-global").small().label(if muted {"Unmute globally"} else {"Mute globally"}).disabled(!editable).on_click(cx.listener(|this,_,window,cx|{
                        let Some(saved)=&this.notifications.saved else{return;};let mut value=saved.value.clone();value.muted = !value.muted;this.save_notifications(value,window,cx);
                    })))
                    .child(Button::new("notifications-reload").small().label("Reload alerts").disabled(self.notifications.busy||self.notifications.refreshing||self.closing).on_click(cx.listener(|this,_,window,cx|this.load_notifications(true,window,cx))))
                    .child(Button::new("notifications-copy").small().ghost().label("Copy policy").disabled(selected.is_none()).on_click(cx.listener(|this,_,_,cx|{
                        if let Some(saved)=&this.notifications.saved {cx.write_to_clipboard(ClipboardItem::new_string(serde_json::to_string_pretty(saved).expect("policy serializes")));}
                    }))))
                .child("Muted items stay in Attention and Next request. System notifications are not available yet.")
                .children(self.notifications.error.iter().map(|error|div().text_color(cx.theme().danger).child(error.clone())))
                .child(div().flex().flex_wrap().gap_2().items_center().child("Mute event types:")
                    .children(Kind::ALL.into_iter().enumerate().map(|(i,kind)| {
                        let muted=selected.as_ref().is_some_and(|p|p.muted_kinds.contains(&kind));
                        Button::new(("notifications-kind",i)).small().ghost().label(kind.label()).selected(muted).disabled(!editable).on_click(cx.listener(move|this,_,window,cx|{
                            let Some(saved)=&this.notifications.saved else{return;};let mut value=saved.value.clone();
                            if value.muted_kinds.contains(&kind){value.muted_kinds.retain(|k|*k!=kind);}else{value.muted_kinds.push(kind);}
                            this.save_notifications(value,window,cx);
                        }))
                    })))
                .child(div().flex().flex_wrap().gap_2().items_center().child("Mute Hosts:")
                    .children(self.slots.iter().enumerate().map(|(i,slot)| {
                        let socket=slot.endpoint.socket.clone();let muted=selected.as_ref().is_some_and(|p|p.muted_hosts.contains(&socket));
                        Button::new(("notifications-host",i)).small().ghost().label(slot.endpoint.name.clone()).selected(muted).disabled(!editable).on_click(cx.listener(move|this,_,window,cx|{
                            let Some(saved)=&this.notifications.saved else{return;};let mut value=saved.value.clone();
                            if value.muted_hosts.contains(&socket){value.muted_hosts.retain(|s|s!=&socket);}else{value.muted_hosts.push(socket.clone());}
                            this.save_notifications(value,window,cx);
                        }))
                    })))
                .children(selected.iter().flat_map(|p|p.muted_sessions.iter()).enumerate().map(|(i,s)|div().flex().gap_2().items_center()
                    .child(self.session_mute_button(format!("notifications-saved-{i}").into(),Some(s.clone()),cx))
                    .child(div().truncate().child(format!("{} · {} · {}",s.socket.display(),s.host_id,s.thread_id)))))))
    }
}

#[cfg(test)]
mod tests {
    use super::{Preferences, Saved, State};
    #[test]
    fn background_read_cannot_release_a_save_or_revert_a_newer_policy() {
        let mut state = State::new(Ok("/tmp/synthetic".into()));
        state.saved = Some(Saved {
            revision: 3,
            value: Preferences {
                muted: true,
                ..Default::default()
            },
        });
        state.busy = true;
        state.refreshing = true;
        state.finish_read(
            false,
            Ok(Saved {
                revision: 2,
                value: Preferences::default(),
            }),
        );
        assert!(state.busy);
        assert!(!state.refreshing);
        assert!(!state.editable());
        assert_eq!(state.saved.as_ref().unwrap().revision, 3);
        assert!(state.saved.as_ref().unwrap().value.muted);
        state.error = Some("save conflict".into());
        state.save_failed = true;
        assert!(state.close_status().is_err());
        state.finish_read(
            false,
            Ok(Saved {
                revision: 4,
                value: Preferences::default(),
            }),
        );
        assert_eq!(state.error.as_deref(), Some("save conflict"));
        assert!(state.close_status().is_err());
        state.finish_read(
            true,
            Ok(Saved {
                revision: 4,
                value: Preferences::default(),
            }),
        );
        assert!(!state.busy);
        assert!(state.error.is_none());
        assert!(state.close_status().unwrap());
    }
}
