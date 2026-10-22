use super::*;
use fleetpane_session::{attention::review::Item, notifications::delivery::Batch};
use gpui_kit::component::{
    WindowExt,
    notification::{Notification, NotificationDelivery},
};

struct CompletionReminder;
struct PendingReminder;

impl Workbench {
    pub(in crate::workbench) fn finish_completion_reminders(
        &mut self,
        batch: Result<Batch, Error>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match batch {
            Ok(batch) => {
                self.notifications.observe(batch.policy);
                self.notifications.delivery_error = None;
                self.reconcile_completion_toast(window, cx);
                if self.closing || self.notifications.busy || self.notifications.error.is_some() {
                    return;
                }
                let Some(saved) = &self.notifications.saved else {
                    return;
                };
                let items: Vec<Item> = batch
                    .items
                    .into_iter()
                    .filter(|item| saved.value.result_allowed(item))
                    .collect();
                if items.is_empty() {
                    return;
                }
                let failed = items
                    .iter()
                    .filter(|i| i.record.kind == fleetpane_runtime::attention::Kind::Failed)
                    .count();
                let interrupted = items
                    .iter()
                    .filter(|i| i.record.kind == fleetpane_runtime::attention::Kind::Interrupted)
                    .count();
                // Keep one bounded toast, with no prompt text, paths or provider payloads.
                let message = format!(
                    "{} new turn results · {failed} failed · {interrupted} interrupted. Open Attention to inspect.",
                    items.len()
                );
                self.notifications.reminder_items = items;
                if saved.value.system_popups {
                    self.notifications
                        .system
                        .show(system::Category::Results, message.clone());
                }
                let owner = cx.entity().downgrade();
                window.push_notification(
                    Notification::new()
                        .id::<CompletionReminder>()
                        .title("Fleetpane · Turn results")
                        .message(message)
                        .delivery(NotificationDelivery::InApp)
                        .on_click(move |_, window, cx| {
                            if let Some(owner) = owner.upgrade() {
                                owner.update(cx, |this, cx| {
                                    if this.closing || this.next_request_blocked(window, cx) {
                                        return;
                                    }
                                    this.open_attention(window, cx);
                                    this.attention.show_completions = true;
                                    this.attention.filter.socket = None;
                                    cx.notify();
                                });
                            }
                        }),
                    cx,
                );
            }
            Err(error) => {
                self.notifications.delivery_error = Some(format!(
                    "Completion reminders unavailable: {error}. No reminder shown; results remain in Attention."
                ));
                window.remove_notification::<CompletionReminder>(cx);
                self.notifications.reminder_items.clear();
                self.notifications.system.clear(system::Category::Results);
            }
        }
    }

    pub(in crate::workbench) fn reconcile_completion_toast(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reconcile_pending_toast(window, cx);
        // A policy change retracts an existing aggregate. Claimed results remain
        // consumed; no toast is resurrected when a narrower mute is removed.
        let allowed = !self.closing
            && self.attention.error.is_none()
            && self.notifications.error.is_none()
            && self.notifications.saved.as_ref().is_some_and(|saved| {
                self.notifications.reminder_items.iter().all(|item| {
                    saved.value.result_allowed(item)
                        && self.reminder_host_available(&item.endpoint.socket, item.host_id)
                        && self.attention.completions.iter().any(|current| {
                            current.endpoint.socket == item.endpoint.socket
                                && current.host_id == item.host_id
                                && current.record == item.record
                                && !current.reviewed
                        })
                })
            });
        if !allowed {
            window.remove_notification::<CompletionReminder>(cx);
            self.notifications.reminder_items.clear();
        }
        if !allowed
            || !self
                .notifications
                .saved
                .as_ref()
                .is_some_and(|s| s.value.system_popups)
        {
            self.notifications.system.clear(system::Category::Results);
        }
    }

    pub(in crate::workbench) fn finish_pending_reminders(
        &mut self,
        batch: Result<fleetpane_session::notifications::pending::Batch, Error>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let batch = match batch {
            Ok(batch) => batch,
            Err(error) => {
                self.notifications.pending_delivery_error = Some(format!(
                    "Pending reminders unavailable: {error}. Requests remain in Attention."
                ));
                window.remove_notification::<PendingReminder>(cx);
                self.notifications.pending_reminder_items.clear();
                self.notifications.system.clear(system::Category::Pending);
                return;
            }
        };
        self.notifications.pending_delivery_error = None;
        self.notifications.observe(batch.policy);
        self.reconcile_pending_toast(window, cx);
        if self.closing || self.notifications.busy || self.notifications.error.is_some() {
            return;
        }
        let Some(saved) = &self.notifications.saved else {
            return;
        };
        let items: Vec<_> = batch
            .items
            .into_iter()
            .filter(|i| saved.value.pending_allowed(i))
            .collect();
        if items.is_empty() {
            return;
        }
        let approvals = items
            .iter()
            .filter(|i| i.kind == fleetpane_session::attention::Kind::Approval)
            .count();
        let questions = items
            .iter()
            .filter(|i| i.kind == fleetpane_session::attention::Kind::Question)
            .count();
        let message = format!(
            "{} new requests · {approvals} approvals · {questions} questions. Open Attention to inspect.",
            items.len()
        );
        self.notifications.pending_reminder_items = items;
        if saved.value.system_popups {
            self.notifications
                .system
                .show(system::Category::Pending, message.clone());
        }
        let owner = cx.entity().downgrade();
        window.push_notification(
            Notification::new()
                .id::<PendingReminder>()
                .title("Fleetpane · Requests need attention")
                .message(message)
                .delivery(NotificationDelivery::InApp)
                .on_click(move |_, window, cx| {
                    if let Some(owner) = owner.upgrade() {
                        owner.update(cx, |this, cx| {
                            if this.closing || this.next_request_blocked(window, cx) {
                                return;
                            }
                            this.open_attention(window, cx);
                            this.attention.show_completions = false;
                            this.attention.filter.socket = None;
                            this.attention.filter.kind = None;
                            cx.notify();
                        });
                    }
                }),
            cx,
        );
    }

    fn reconcile_pending_toast(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let allowed = !self.closing
            && self.attention.error.is_none()
            && self.notifications.error.is_none()
            && self.notifications.saved.as_ref().is_some_and(|saved| {
                self.notifications
                    .pending_reminder_items
                    .iter()
                    .all(|item| {
                        saved.value.pending_allowed(item)
                            && self.reminder_host_available(&item.endpoint.socket, item.host_id)
                            && self.attention.items.iter().any(|current| {
                                current.endpoint.socket == item.endpoint.socket
                                    && current.host_id == item.host_id
                                    && current.request_sequence == item.request_sequence
                                    && current.request == item.request
                            })
                    })
            });
        if !allowed {
            window.remove_notification::<PendingReminder>(cx);
            self.notifications.pending_reminder_items.clear();
        }
        if !allowed
            || !self
                .notifications
                .saved
                .as_ref()
                .is_some_and(|s| s.value.system_popups)
        {
            self.notifications.system.clear(system::Category::Pending);
        }
    }

    fn reminder_host_available(&self, socket: &std::path::Path, host_id: uuid::Uuid) -> bool {
        self.attention.report.as_ref().is_some_and(|report| {
            report.hosts.iter().any(|host| {
                host.endpoint.socket == socket
                    && host.host_id == Some(host_id)
                    && host.error.is_none()
            })
        })
    }
}
