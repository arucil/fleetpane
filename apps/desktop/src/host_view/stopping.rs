use super::*;

impl HostView {
    pub(super) fn request_stop(
        &mut self,
        connection: Connection,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = Uuid::new_v4();
        self.busy = true;
        self.revision += 1;
        self.error = None;
        let sent = command.clone();
        let task = cx.background_executor().spawn(async move {
            background(async move {
                let operation = connection.request_stop(&sent, id).await?;
                // Do not promote admission to provider acknowledgement. In particular,
                // a missing reply must leave other sessions and requests usable.
                let snapshot = client::snapshot(&connection.socket).await;
                Ok((operation, snapshot))
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok((operation, snapshot)) => {
                        match snapshot {
                            Ok(snapshot) => this.adopt_snapshot(snapshot),
                            Err(error) => {
                                this.error = Some(format!("Stop was admitted as {id}, but progress could not be read: {error}. Reconnect before continuing."));
                                this.poll_paused = true;
                            }
                        }
                        if !matches!(operation.outcome, fleetpane_runtime::Outcome::Accepted)
                            && let Err(error) = client::completed(operation)
                        {
                            this.error = Some(error.to_string());
                        }
                        this.sync_questions(window, cx);
                    }
                    Err(Error::Pending(_)) => {
                        // Only admission transport loss requires global receipt recovery.
                        // Never submit a replacement cancellation automatically.
                        this.pending = Some((id, command));
                        this.pending_uncertain = false;
                        this.error = Some(Error::Pending(id).to_string());
                    }
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
}
