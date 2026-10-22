use super::*;
use fleetpane_runtime::usage::View;

#[derive(Default)]
pub(super) struct State {
    loading: bool,
    error: Option<String>,
    generation: u64,
}

impl HostView {
    pub(super) fn apply_context_usage(
        &mut self,
        id: &str,
        completed: Completed,
    ) -> Result<(), Error> {
        let view: View =
            serde_json::from_value(completed.result).map_err(|e| Error::Protocol(e.to_string()))?;
        view.validate(id, completed.sequence)
            .map_err(Error::Protocol)?;
        if let Some(thread) = self.thread.as_mut().filter(|t| t.id == id) {
            thread
                .apply_context_usage(view, completed.sequence)
                .map_err(|e| Error::Protocol(e.to_string()))?;
        }
        Ok(())
    }

    pub(super) fn load_context_usage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(thread), Some(connection)) = (&self.thread, self.connection.clone()) else {
            return;
        };
        let id = thread.id.clone();
        let host = connection.lease.host_id;
        self.usage.generation += 1;
        let generation = self.usage.generation;
        self.usage.loading = true;
        self.usage.error = None;
        let command = Command::ContextUsage {
            thread_id: id.clone(),
        };
        let task = cx
            .background_executor()
            .spawn(async move { background(connection.execute(&command, Uuid::new_v4())) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update(cx, |this, cx| {
                if this.usage.generation != generation
                    || this.snapshot.as_ref().is_none_or(|s| s.host_id != host)
                    || this.thread.as_ref().is_none_or(|t| t.id != id)
                {
                    return;
                }
                this.usage.loading = false;
                this.usage.error = result
                    .and_then(|result| this.apply_context_usage(&id, result))
                    .err()
                    .map(|e| e.to_string());
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn render_context_usage(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let observation = self
            .thread
            .as_ref()
            .and_then(|t| t.context_usage.as_ref())
            .and_then(|v| v.observation.as_ref());
        div().px_4().py_2().border_t_1().border_color(cx.theme().border)
            .child(div().flex().flex_wrap().items_center().gap_2()
                .child(div().flex_1().min_w_0().text_xs().text_color(cx.theme().muted_foreground)
                    .child(fleetpane_session::usage::summary(observation)))
                .when_some(observation, |row, observation| {
                    let copy = observation.copy_json();
                    row.child(Button::new("copy-context-usage").small().ghost().label("Copy usage JSON")
                        .on_click(move |_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))))
                })
                .child(Button::new("refresh-context-usage").small().ghost()
                    .label(if self.usage.loading {"Loading…"} else {"Refresh context"})
                    .disabled(self.usage.loading || self.thread.is_none())
                    .on_click(cx.listener(|this,_,window,cx|this.load_context_usage(window,cx)))))
            .when_some(observation, |row, observation| {
                let observed = format!("last observed at {} Unix ms (Host receipt)", observation.observed_at_unix_ms);
                row.child(div().text_xs().text_color(cx.theme().muted_foreground)
                    .child(format!("{} · {} · last reported snapshot", fleetpane_session::usage::source(observation), observed)))
            })
            .when(observation.is_none() && !self.usage.loading && self.usage.error.is_none(), |row|row.child(
                div().text_xs().text_color(cx.theme().muted_foreground)
                    .child("No provider context snapshot retained. Capacity and usage are unavailable.")))
            .when_some(self.usage.error.as_ref(), |row,error|row.child(
                div().text_xs().text_color(cx.theme().danger).child(format!("Cannot read context usage: {error}"))))
    }
}
