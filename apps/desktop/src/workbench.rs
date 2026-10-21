//! Retain separate Host views; switching never transfers a lease or resumes a session.
mod attention;
mod files;
mod host_choice;
mod sessions;
use crate::host_view::{HostView, background};
use crate::workspace_view::WorkspaceView;
use fleetpane_session::hosts::{Configuration, Endpoint};
use fleetpane_session::{
    client::Error,
    creations::{Intent, Resolution, Store},
    routing::{self, Selection},
};
use gpui_kit::component::{
    ActiveTheme, Disableable, Selectable, Sizable,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::{prelude::*, *};

struct Slot {
    endpoint: Endpoint,
    view: Option<Entity<HostView>>,
    observation: Option<Subscription>,
    file_subscription: Option<Subscription>,
}

pub struct Workbench {
    slots: Vec<Slot>,
    selected: usize,
    closing: bool,
    close_error: Option<String>,
    configuration: Configuration,
    route_index: usize,
    workspace: Entity<InputState>,
    selection: Option<Selection>,
    routing_busy: bool,
    route_error: Option<String>,
    route_notice: Option<String>,
    choosing: bool,
    browsing_workspaces: bool,
    workspace_view: Option<Entity<WorkspaceView>>,
    workspace_subscription: Option<Subscription>,
    continuation_subscription: Option<Subscription>,
    file_navigation: files::State,
    attention: attention::State,
    sessions: sessions::State,
    host_choice: host_choice::State,
    creation_directory: Option<std::path::PathBuf>,
    creations: Vec<Intent>,
    records_busy: bool,
    resolution: Option<(uuid::Uuid, String)>,
    _input_subscription: Subscription,
}

impl Workbench {
    fn open_chooser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.leave_attention();
        self.sessions.active = false;
        self.choosing = true;
        self.file_navigation = files::State::default();
        self.browsing_workspaces = false;
        self.route_notice = None;
        self.workspace
            .update(cx, |input, cx| input.focus(window, cx));
        self.load_creations(None, window, cx);
        cx.notify();
    }

    pub fn new(
        configuration: Configuration,
        creation_directory: Option<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let workspace =
            cx.new(|cx| InputState::new(window, cx).placeholder("Absolute workspace directory"));
        let subscription = cx.subscribe(&workspace, |this, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                this.selection = None;
                this.route_error = None;
                this.route_notice = None;
                cx.notify();
            }
        });
        let mut value = Self {
            slots: configuration
                .hosts
                .iter()
                .cloned()
                .map(|endpoint| Slot {
                    endpoint,
                    view: None,
                    observation: None,
                    file_subscription: None,
                })
                .collect(),
            selected: 0,
            closing: false,
            close_error: None,
            configuration,
            route_index: 0,
            workspace,
            selection: None,
            routing_busy: false,
            route_error: None,
            route_notice: None,
            choosing: false,
            browsing_workspaces: false,
            workspace_view: None,
            workspace_subscription: None,
            continuation_subscription: None,
            file_navigation: files::State::default(),
            attention: attention::State::default(),
            sessions: sessions::State::new(window, cx),
            host_choice: host_choice::State::default(),
            creation_directory,
            creations: Vec::new(),
            records_busy: false,
            resolution: None,
            _input_subscription: subscription,
        };
        value.load_host_choice(true, window, cx);
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(2))
                    .await;
                if this
                    .update_in(cx, |this, window, cx| {
                        if this.attention.active {
                            this.refresh_attention(window, cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let owner = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            owner
                .upgrade()
                .is_none_or(|view| view.update(cx, |this, cx| this.request_close(window, cx)))
        });
        value
    }

    fn drafts_saved(&self, cx: &App) -> Result<bool, String> {
        if let Some(error) = &self.host_choice.error {
            return Err(error.clone());
        }
        let mut ready = !self.host_choice.busy;
        for slot in &self.slots {
            if let Some(view) = &slot.view {
                ready &= view
                    .read(cx)
                    .draft_close_status()
                    .map_err(|error| format!("{}: {error}", slot.endpoint.name))?;
            }
        }
        if let Some(view) = &self.workspace_view {
            ready &= view.read(cx).close_status(cx)?;
        }
        Ok(ready)
    }

    pub(crate) fn is_closing(&self) -> bool {
        self.closing
    }

    pub(crate) fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match self.drafts_saved(cx) {
            Ok(true) => true,
            Err(error) => {
                self.close_error = Some(format!("Window remains open: {error}"));
                cx.notify();
                false
            }
            Ok(false) => {
                if self.closing {
                    return false;
                }
                self.closing = true;
                self.close_error =
                    Some("Waiting for local saves and terminal input before closing…".into());
                cx.spawn_in(window, async move |this, cx| {
                    loop {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(25))
                            .await;
                        let waiting =
                            this.update_in(cx, |this, window, cx| match this.drafts_saved(cx) {
                                Ok(true) => {
                                    window.remove_window();
                                    false
                                }
                                Ok(false) => true,
                                Err(error) => {
                                    this.closing = false;
                                    this.close_error =
                                        Some(format!("Window remains open: {error}"));
                                    cx.notify();
                                    false
                                }
                            });
                        if !matches!(waiting, Ok(true)) {
                            break;
                        }
                    }
                })
                .detach();
                cx.notify();
                false
            }
        }
    }

    fn show_host(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.sessions.active = false;
        self.leave_attention();
        self.file_navigation = files::State::default();
        let slot = &mut self.slots[index];
        if slot.view.is_none() {
            let socket = slot.endpoint.socket.clone();
            let directory = self
                .creation_directory
                .clone()
                .expect("local Host configuration has a controller directory");
            let view = cx.new(|cx| HostView::new(socket, directory, window, cx));
            slot.observation = Some(cx.observe(&view, |_, _, cx| cx.notify()));
            slot.file_subscription = Some(cx.subscribe_in(
                &view,
                window,
                move |this, _, request, window, cx| this.open_file(index, request, window, cx),
            ));
            slot.view = Some(view);
        }
        self.selected = index;
        self.choosing = false;
        self.browsing_workspaces = false;
        self.route_notice = None;
        // Remove focus from the previous Host's composer. Retained hidden input
        // entities must never receive keystrokes intended for the new Host.
        slot.view
            .as_ref()
            .expect("selected Host initialized")
            .update(cx, |view, cx| view.focus(window, cx));
        cx.notify();
    }

    fn open_workspaces(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sessions.active = false;
        self.leave_attention();
        self.file_navigation = files::State::default();
        let Some(directory) = self.creation_directory.clone() else {
            return;
        };
        self.choosing = false;
        self.browsing_workspaces = true;
        self.route_notice = None;
        if self.workspace_view.is_none() {
            let view = cx.new(|cx| {
                WorkspaceView::new(
                    directory,
                    self.configuration.hosts.clone(),
                    self.selected,
                    window,
                    cx,
                )
            });
            self.workspace_subscription=Some(cx.subscribe_in(&view,window,|this,_,workspace:&fleetpane_session::workspaces::Workspace,window,cx| {
                if !this.browsing_workspaces { return; }
                let index=this.slots.iter().position(|slot|slot.endpoint.socket==workspace.registration.endpoint.socket).expect("workspace target validated against configured Hosts");
                this.select(index,window,cx);
                this.slots[index].view.as_ref().expect("selected Host initialized").update(cx,|view,cx|view.prepare_workspace(workspace.clone(),window,cx));
                this.route_notice=Some("Saved workspace prepared. Choose New session; take control first if needed. Directory identity is rechecked before creation.".into());
                cx.notify();
            }));
            self.continuation_subscription=Some(cx.subscribe_in(&view,window,|this,_,event:&crate::paseo_import_view::Continuation,window,cx| {
                if !this.browsing_workspaces {return;}
                let Some(index)=this.slots.iter().position(|s|s.endpoint.socket==event.intent.target.endpoint.socket) else {
                    this.route_notice=Some("The saved fork targets a Host absent from this configuration. Restore its endpoint to check the original operation.".into());cx.notify();return;
                };
                this.select(index,window,cx);
                let result=this.slots[index].view.as_ref().expect("selected Host initialized").update(cx,|view,cx|view.prepare_continuation(event.clone(),cx));
                this.route_notice=Some(match result {Ok(())=>"Saved Codex history prepared. Take control if needed, then choose Fork saved context. This creates an independent session.".into(),Err(e)=>e});
                cx.notify();
            }));
            self.workspace_view = Some(view);
        }
        self.workspace_view
            .as_ref()
            .expect("workspace view initialized")
            .update(cx, |view, cx| view.open(window, cx));
        cx.notify();
    }

    fn find_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.routing_busy {
            return;
        }
        if !std::path::Path::new(self.workspace.read(cx).value().as_str()).is_absolute() {
            self.route_error = Some("Choose an absolute workspace directory.".into());
            cx.notify();
            return;
        }
        self.routing_busy = true;
        self.selection = None;
        self.route_error = None;
        self.route_notice = None;
        let configuration = self.configuration.clone();
        let group = configuration.routes[self.route_index].name.clone();
        let task = cx.background_executor().spawn(async move {
            background(async move {
                routing::select(&configuration, &group)
                    .await
                    .map_err(|error| Error::Input(error.to_string()))
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, _, cx| {
                this.routing_busy = false;
                if !this.choosing {
                    this.selection = None;
                    cx.notify();
                    return;
                }
                match result {
                    Ok(selection) => this.selection = Some(selection),
                    Err(error) => this.route_error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn use_target(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.routing_busy {
            return;
        }
        let Some(target) = self
            .selection
            .as_ref()
            .and_then(|selection| selection.target.clone())
        else {
            return;
        };
        let cwd = self.workspace.read(cx).value().to_string();
        let Some(directory) = self.creation_directory.clone() else {
            return;
        };
        let group = self.configuration.routes[self.route_index].name.clone();
        self.routing_busy = true;
        self.route_error = None;
        let checked = target.clone();
        let task = cx.background_executor().spawn(async move {
            background(async move {
                checked
                    .confirm()
                    .await
                    .map_err(|error| Error::Input(error.to_string()))?;
                let store = Store::open(directory).await?;
                let intent = store
                    .bind(uuid::Uuid::new_v4(), group, cwd, checked)
                    .await?;
                Ok((store, intent))
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.routing_busy = false;
                this.selection = None;
                // A user may have returned to an approval while the read was
                // pending. Do not switch them away on a late response.
                if !this.choosing {
                    cx.notify();
                    return;
                }
                match result {
                    Ok((store, intent)) => {
                        let index = this.slots.iter().position(|slot| slot.endpoint.socket == target.endpoint.socket).expect("configured recommendation target");
                        this.select(index, window, cx);
                        this.slots[index].view.as_ref().expect("selected Host initialized").update(cx, |view, cx| view.prepare_creation(store, intent, window, cx));
                        this.route_notice = Some(format!("Creation saved on {}. Choose New session there; take control first if needed. Reopen New via group to check its receipt.", target.endpoint.name));
                    }
                    Err(error) => this.route_error = Some(error.to_string()),
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    fn load_creations(&mut self, before: Option<u64>, window: &mut Window, cx: &mut Context<Self>) {
        if self.records_busy {
            return;
        }
        let Some(directory) = self.creation_directory.clone() else {
            return;
        };
        self.records_busy = true;
        let task = cx.background_executor().spawn(async move {
            background(async move { Store::open(directory).await?.page(before).await })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, _, cx| {
                this.records_busy = false;
                match result {
                    Ok(records) => this.creations = records,
                    Err(error) => this.route_error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn recover_creation(&mut self, id: uuid::Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if self.routing_busy {
            return;
        }
        let Some(directory) = self.creation_directory.clone() else {
            return;
        };
        self.routing_busy = true;
        self.route_error = None;
        self.resolution = None;
        let task = cx.background_executor().spawn(async move {
            background(async move {
                let store = Store::open(directory).await?;
                let intent = store
                    .get(id)
                    .await?
                    .ok_or_else(|| Error::Input("Saved creation not found".into()))?;
                let resolution = intent.resolve().await?;
                Ok((store, intent, resolution))
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.routing_busy = false;
                match result {
                    Ok((store, intent, Resolution::Prepared)) if this.choosing => {
                        let index = this.slots.iter().position(|slot| slot.endpoint.socket == intent.target.endpoint.socket);
                        if let Some(index) = index {
                            this.select(index, window, cx);
                            let host = intent.target.endpoint.name.clone();
                            this.slots[index].view.as_ref().expect("selected Host initialized").update(cx, |view, cx| view.prepare_creation(store, intent, window, cx));
                            this.route_notice = Some(format!("Saved creation prepared on {host}. Choose New session; take control first if needed."));
                        } else {
                            this.route_error = Some("Saved Host is absent from this configuration. Restore its endpoint to prepare this creation; its binding cannot be changed.".into());
                        }
                    }
                    Ok((_, intent, resolution)) => {
                        let text = match resolution {
                            Resolution::Prepared => "Prepared; no request has been sent.".into(),
                            Resolution::Pending => "Host accepted this creation; result is still pending. Check again.".into(),
                            Resolution::Created { thread_id } => format!("Created session {thread_id} on {}. Select that Host to open it from its session list.", intent.target.endpoint.name),
                            Resolution::Failed { error } => format!("Provider rejected creation: {error}"),
                            Resolution::Unknown { reason } => format!("Outcome unknown: {reason}. This creation will not be resent."),
                        };
                        this.resolution = Some((id, text));
                        if let Some(record) = this.creations.iter_mut().find(|record| record.id == id) { *record = intent; }
                    }
                    Err(error) => this.route_error = Some(format!("Could not check saved creation {id}: {error}. Its Host binding is retained; no request was resent.")),
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    fn creation_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child("Saved group creations")
                    .child(
                        Button::new("refresh-creations")
                            .small()
                            .label("Newest")
                            .disabled(self.records_busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.load_creations(None, window, cx)
                            })),
                    )
                    .when(self.creations.len() == 32, |row| {
                        row.child(
                            Button::new("older-creations")
                                .small()
                                .label("Older")
                                .disabled(self.records_busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    let before =
                                        this.creations.last().map(|intent| intent.position);
                                    this.load_creations(before, window, cx);
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .id("creation-records")
                    .max_h_64()
                    .overflow_y_scroll()
                    .text_xs()
                    .children(self.creations.iter().enumerate().map(|(index, intent)| {
                        let id = intent.id;
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .py_1()
                            .child(
                                Button::new(("recover-creation", index))
                                    .small()
                                    .label(if intent.attempted {
                                        "Check status"
                                    } else {
                                        "Prepare saved"
                                    })
                                    .disabled(self.routing_busy)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.recover_creation(id, window, cx)
                                    })),
                            )
                            .child(div().flex_1().min_w_0().truncate().child(format!(
                                "{} · {} · {} · {}",
                                intent.id, intent.route, intent.target.endpoint.name, intent.cwd
                            )))
                    })),
            )
            .when_some(self.resolution.as_ref(), |view, (id, text)| {
                view.child(div().text_sm().child(format!("{id}: {text}")))
            })
    }

    fn route_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let routes: Vec<String> = self
            .configuration
            .routes
            .iter()
            .map(|route| route.name.clone())
            .collect();
        let selected = self.route_index;
        let owner = cx.entity().downgrade();
        div()
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_lg().child("Prepare a new session"))
            .child("Find a ready Host in the selected group. Existing sessions keep running on their current Hosts.")
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child("New session via group")
                    .child(
                        Button::new("route-group")
                            .small()
                            .label(routes[selected].clone())
                            .disabled(self.routing_busy)
                            .dropdown_menu_with_anchor(
                                Anchor::BottomLeft,
                                move |mut menu, _, _| {
                                    for (index, name) in routes.iter().enumerate() {
                                        let owner = owner.clone();
                                        menu = menu.item(
                                            PopupMenuItem::new(name.clone())
                                                .checked(selected == index)
                                                .on_click(move |_, _, cx| {
                                                    if let Some(entity) = owner.upgrade() {
                                                        entity.update(cx, |this, cx| {
                                                            if this.routing_busy {
                                                                return;
                                                            }
                                                            this.route_index = index;
                                                            this.selection = None;
                                                            this.route_error = None;
                                                            this.route_notice = None;
                                                            cx.notify();
                                                        });
                                                    }
                                                }),
                                        );
                                    }
                                    menu
                                },
                            ),
                    )
                    .child(
                        div()
                            .w_80()
                            .child(Input::new(&self.workspace).disabled(self.routing_busy)),
                    )
                    .child(
                        Button::new("find-host")
                            .small()
                            .label(if self.routing_busy {
                                "Checking…"
                            } else {
                                "Find ready Host"
                            })
                            .disabled(self.routing_busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.find_host(window, cx)),
                            ),
                    )
                    .when_some(
                        self.selection
                            .as_ref()
                            .and_then(|selection| selection.target.as_ref()),
                        |row, target| {
                            row.child(
                                Button::new("use-host")
                                    .small()
                        .label(format!("Prepare on {}", target.endpoint.name))
                                    .disabled(self.routing_busy)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.use_target(window, cx)
                                    })),
                            )
                        },
                    ),
            )
            .when_some(self.selection.as_ref(), |bar, selection| {
                bar.child(
                    div()
                        .id("route-status")
                        .max_h_64()
                        .overflow_y_scroll()
                        .text_xs()
                        .when(selection.target.is_none(), |list| {
                            list.child("No ready Host in this group. No session was created.")
                        })
                        .children(
                            selection
                                .candidates
                                .iter()
                                .map(|candidate| div().child(candidate.label())),
                        ),
                )
            })
            .children(self.route_error.iter().map(|message| {
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(message.clone())
            }))
            .children(
                self.route_notice
                    .iter()
                    .map(|message| div().text_sm().child(message.clone())),
            )
            .child(self.creation_list(cx))
    }
}

impl Render for Workbench {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = &self.slots[self.selected];
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .child(self.host_choice_bar(cx))
            .when_some(self.close_error.as_ref(), |view, error| {
                view.child(
                    div()
                        .p_2()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error.clone()),
                )
            })
            .when(
                self.slots.len() > 1
                    || !self.configuration.routes.is_empty()
                    || self.creation_directory.is_some(),
                |root| {
                    root.child(
                        div()
                            .px_4()
                            .py_2()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .children(self.slots.iter().enumerate().map(|(index, slot)| {
                                let status = slot.view.as_ref().map(|view| view.read(cx).badge());
                                let label = match status {
                                    Some(status) => format!("{} · {status}", slot.endpoint.name),
                                    None => slot.endpoint.name.clone(),
                                };
                                Button::new(("host", index))
                                    .small()
                                    .ghost()
                                    .selected(index == self.selected)
                                    .label(label)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.select(index, window, cx)
                                    }))
                            }))
                            .child(
                                Button::new("open-all-sessions")
                                    .small()
                                    .label("All sessions")
                                    .selected(self.sessions.active)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_sessions(window, cx)
                                    })),
                            )
                            .child(
                                Button::new("open-attention")
                                    .small()
                                    .label("Attention")
                                    .selected(self.attention.active)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_attention(window, cx)
                                    })),
                            )
                            .when(self.creation_directory.is_some(), |bar| {
                                bar.child(
                                    Button::new("open-workspaces")
                                        .small()
                                        .label("Workspaces")
                                        .selected(self.browsing_workspaces)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.open_workspaces(window, cx)
                                        })),
                                )
                            })
                            .when(!self.configuration.routes.is_empty(), |bar| {
                                bar.child(
                                    Button::new("open-route-chooser")
                                        .small()
                                        .label("New via group")
                                        .selected(self.choosing)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.open_chooser(window, cx)
                                        })),
                                )
                            }),
                    )
                },
            )
            .when(self.choosing, |root| root.child(self.route_bar(cx)))
            .when(self.attention.active, |root| {
                root.child(self.attention_view(cx))
            })
            .when(self.sessions.active, |root| {
                root.child(self.sessions_view(cx))
            })
            .when(self.browsing_workspaces, |root| {
                root.child(
                    div().flex_1().min_h_0().child(
                        self.workspace_view
                            .clone()
                            .expect("workspace view initialized"),
                    ),
                )
            })
            .children(
                self.route_notice
                    .iter()
                    .map(|message| div().px_4().py_2().text_sm().child(message.clone())),
            )
            .when(self.file_navigation.pending.is_some(), |root| {
                root.child(div().px_4().py_2().child("Opening workspace file…"))
            })
            .children(self.file_navigation.error.iter().map(|error| {
                div()
                    .px_4()
                    .py_2()
                    .text_color(cx.theme().danger)
                    .child(error.clone())
            }))
            .when_some(self.file_navigation.view.as_ref(), |root, view| {
                root.child(div().flex_1().min_h_0().child(view.clone()))
            })
            .when(
                !self.choosing
                    && !self.browsing_workspaces
                    && !self.attention.active
                    && !self.sessions.active
                    && self.file_navigation.view.is_none(),
                |root| {
                    root.child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .id(("host-content", self.selected))
                            .children(selected.view.clone())
                            .when(selected.view.is_none(), |root| {
                                root.child("Loading saved Host choice…")
                            }),
                    )
                },
            )
    }
}
