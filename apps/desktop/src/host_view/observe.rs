//! Read-only snapshots keep request/ownership state fresh without a controller lease.
use super::*;

impl HostView {
    pub(super) fn observe(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .observer_polled_at
            .is_some_and(|at| at.elapsed() < Duration::from_secs(2))
        {
            return;
        }
        self.observer_polled_at = Some(std::time::Instant::now());
        self.polling = true;
        let revision = self.revision;
        let socket = self.socket.clone();
        let task = cx
            .background_executor()
            .spawn(async move { background(client::snapshot(&socket)) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.polling = false;
                if this.revision != revision {
                    return;
                }
                match result {
                    Ok(state) => {
                        this.observer_error = None;
                        if this
                            .snapshot
                            .as_ref()
                            .is_some_and(|old| old.host_id != state.host_id)
                        {
                            this.questions.clear();
                            this.permission_preview = None;
                            this.permission_file_subscription = None;
                        }
                        this.adopt_snapshot(state);
                        this.sync_questions(window, cx);
                        this.sync_permission_preview(cx);
                    }
                    Err(error) => this.observer_error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
