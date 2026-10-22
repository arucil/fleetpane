use super::*;
use fleetpane_session::{attention::review::Item, notifications::delivery::Batch};
use gpui_kit::component::{
    WindowExt,
    notification::{Notification, NotificationDelivery},
};

struct CompletionReminder;

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
            }
        }
    }

    pub(in crate::workbench) fn reconcile_completion_toast(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A policy change retracts an existing aggregate. Claimed results remain
        // consumed; no toast is resurrected when a narrower mute is removed.
        let allowed = self.notifications.error.is_none()
            && self.notifications.saved.as_ref().is_some_and(|saved| {
                self.notifications
                    .reminder_items
                    .iter()
                    .all(|item| saved.value.result_allowed(item))
            });
        if !allowed {
            window.remove_notification::<CompletionReminder>(cx);
            self.notifications.reminder_items.clear();
        }
    }
}
