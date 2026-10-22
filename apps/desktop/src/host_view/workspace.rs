//! Workspace-local view metadata with one explicit controller shared per Host.
use super::*;
pub(crate) type Control = std::rc::Rc<std::cell::RefCell<Option<Connection>>>;

impl HostView {
    pub(crate) fn scope_workspace(
        &mut self,
        workspace: fleetpane_session::workspaces::Workspace,
        saved: &fleetpane_session::workspace_layout::Tab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prepare_workspace(workspace.clone(), window, cx);
        let cwd = workspace
            .registration
            .directory
            .path
            .to_str()
            .expect("registered UTF-8 directory")
            .to_owned();
        self.filter_workspace
            .update(cx, |input, cx| input.set_value(cwd.clone(), window, cx));
        self.directory.query.cwd = Some(cwd);
        self.workspace_scope = Some(workspace);
        self.initial_sidebar_width = saved.sidebar_width;
        self.directory_visible = saved.sidebar_visible;
        self.last_session.saved = Some(fleetpane_session::session_selection::Saved {
            revision: 0,
            target: saved.session.clone(),
        });
        self.restore_workspace_session = saved.session.is_some();
        self.workspace_directory_pending = true;
        cx.notify();
    }

    pub(crate) fn set_view_visible(&mut self, visible: bool) {
        self.view_visible = visible;
    }

    pub(super) fn sync_workspace_control(&mut self) {
        self.connection = self.shared_control.borrow().clone();
    }

    pub(super) fn execution_connection(
        &self,
        command: &Command,
    ) -> Result<Option<Connection>, Error> {
        if self.can_act() {
            return Ok(self.connection.clone());
        }
        if self.workspace_scope.is_none()
            || !command.is_query()?
            || self.recovery.blocked()
            || self.observer_error.is_some()
        {
            return Ok(None);
        }
        Ok(self
            .snapshot
            .as_ref()
            .filter(|state| state.fault.is_none())
            .and_then(|state| state.owner.clone())
            .map(|lease| Connection {
                socket: self.socket.clone(),
                lease,
            }))
    }

    pub(super) fn restore_workspace_history(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self
            .last_session
            .saved
            .as_ref()
            .and_then(|saved| saved.target.clone())
        else {
            self.restore_workspace_session = false;
            return;
        };
        let Some(state) = &self.snapshot else {
            return;
        };
        if state.owner.is_none() {
            return;
        }
        self.restore_workspace_session = false;
        if state.host_id != target.host_id {
            self.error = Some(
                "Saved session belongs to a replaced Host. Choose a current session explicitly."
                    .into(),
            );
            cx.notify();
            return;
        }
        let checking = target.clone();
        let socket = self.socket.clone();
        let revision = self.revision;
        self.busy = true;
        let task = cx
            .background_executor()
            .spawn(async move { background(checking.observe(socket)) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                if this.revision != revision
                    || this
                        .last_session
                        .saved
                        .as_ref()
                        .and_then(|saved| saved.target.as_ref())
                        != Some(&target)
                {
                    return;
                }
                match result {
                    Ok(()) => {
                        this.last_session.restoring = Some(target.clone());
                        this.command(
                            Command::Inspect {
                                thread_id: target.thread_id,
                                resume: false,
                            },
                            window,
                            cx,
                        );
                        if !this.busy {
                            this.last_session.restoring = None;
                        }
                    }
                    Err(error) => {
                        this.error =
                            Some(format!("Saved workspace session was not opened: {error}"))
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn presentation(
        &self,
        cx: &App,
    ) -> (
        Option<fleetpane_session::session_selection::Target>,
        u16,
        bool,
    ) {
        // The saved target was bound when a thread read succeeded. A newer
        // Host snapshot must never relabel retained old content as its session.
        let session = self
            .last_session
            .saved
            .as_ref()
            .and_then(|saved| saved.target.clone());
        let width = self
            .directory_layout
            .read(cx)
            .sizes()
            .first()
            .map_or(self.initial_sidebar_width, |width| {
                f32::from(*width).round().clamp(240., 520.) as u16
            });
        (session, width, self.directory_visible)
    }
}
