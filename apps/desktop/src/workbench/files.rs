//! File navigation does not claim, open, resume, or send to a provider.
use super::*;
use crate::{files_view::FilesView, host_view::FileRequest};
use fleetpane_session::workspaces::Kind;

#[derive(Default)]
pub(super) struct State {
    pub pending: Option<uuid::Uuid>,
    pub error: Option<String>,
    pub view: Option<Entity<FilesView>>,
    subscription: Option<Subscription>,
}

impl Workbench {
    pub(super) fn open_file(
        &mut self,
        index: usize,
        request: &FileRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected != index || self.file_navigation.pending.is_some() {
            return;
        }
        let Some(view) = self.slots[index].view.clone() else {
            return;
        };
        if !view.read(cx).matches_file_request(request) {
            return;
        }
        let request = request.clone();
        let endpoint = self.slots[index].endpoint.clone();
        let directory = self
            .creation_directory
            .clone()
            .expect("configured local controller directory");
        let token = uuid::Uuid::new_v4();
        self.file_navigation.pending = Some(token);
        self.file_navigation.error = None;
        let cwd = std::path::PathBuf::from(&request.cwd);
        let task = cx.background_executor().spawn(async move {
            background(async move {
                let store = Store::open(directory).await?;
                match store.workspace_at(&endpoint.socket, &cwd).await? {
                    Some(workspace) => Ok(workspace),
                    None => store.register_workspace(endpoint, cwd, Kind::Folder).await,
                }
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                if this.file_navigation.pending != Some(token) {
                    return;
                }
                this.file_navigation.pending = None;
                if this.selected != index || !view.read(cx).matches_file_request(&request) {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(workspace) => {
                        let files =
                            cx.new(|cx| FilesView::target(workspace, request.target, window, cx));
                        this.file_navigation.subscription =
                            Some(cx.subscribe_in(&files, window, |this, _, _, window, cx| {
                                this.file_navigation = State::default();
                                this.slots[this.selected]
                                    .view
                                    .as_ref()
                                    .expect("selected Host initialized")
                                    .update(cx, |view, cx| view.focus(window, cx));
                                cx.notify();
                            }));
                        this.file_navigation.view = Some(files);
                    }
                    Err(error) => this.file_navigation.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
