//! Kit owns search and keyboard selection; targets are frozen full identities.
use super::*;
use fleetpane_session::{directory::PageRequest, fleet_directory, fleet_directory::switcher};
use gpui_kit::component::command::{Command as Palette, CommandItem, CommandState};

gpui_kit::actions!(fleetpane_switcher, [Toggle, Close, FocusSearch]);

pub(super) fn init(cx: &mut App) {
    let shortcut = if cfg!(target_os = "macos") {
        "cmd-k"
    } else {
        "ctrl-k"
    };
    cx.bind_keys([
        KeyBinding::new(shortcut, Toggle, Some("FleetpaneWorkbench")),
        KeyBinding::new("escape", Close, Some("FleetpaneSwitcher")),
        KeyBinding::new("escape", Close, Some("FleetpaneSwitcher > Command")),
        KeyBinding::new("escape", Close, Some("FleetpaneSwitcher > Input")),
        KeyBinding::new("tab", FocusSearch, Some("FleetpaneSwitcher")),
        KeyBinding::new("shift-tab", FocusSearch, Some("FleetpaneSwitcher")),
        KeyBinding::new("tab", FocusSearch, Some("FleetpaneSwitcher > Input")),
        KeyBinding::new("shift-tab", FocusSearch, Some("FleetpaneSwitcher > Input")),
    ]);
}

pub(super) struct State {
    active: bool,
    busy: bool,
    generation: u64,
    palette: Entity<CommandState>,
    catalog: std::sync::Arc<switcher::Catalog>,
    return_focus: Option<FocusHandle>,
    error: Option<String>,
}

impl State {
    pub fn new(window: &mut Window, cx: &mut Context<Workbench>) -> Self {
        Self {
            active: false,
            busy: false,
            generation: 0,
            palette: cx.new(|cx| CommandState::new(window, cx)),
            catalog: Default::default(),
            return_focus: None,
            error: None,
        }
    }
}

impl Workbench {
    pub(super) fn toggle_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.switcher.active {
            self.close_switcher(window, cx);
            return;
        }
        if self.export_pending(cx) || self.closing {
            return;
        }
        self.switcher.generation += 1;
        self.switcher.return_focus = window.focused(cx);
        self.switcher.palette = cx.new(|cx| CommandState::new(window, cx));
        self.switcher.active = true;
        self.switcher.error = None;
        self.switcher.catalog = Default::default();
        self.switcher.palette.focus_handle(cx).focus(window, cx);
        // Closing and reopening reuses the one bounded metadata read.
        if self.switcher.busy {
            cx.notify();
            return;
        }
        self.switcher.busy = true;
        let configuration = self.configuration.clone();
        let task = cx.background_executor().spawn(async move {
            background(fleet_directory::collect(configuration, Default::default()))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, _, cx| {
                this.switcher.busy = false;
                match result {
                    Ok(reports) => {
                        let groups = reports
                            .into_iter()
                            .map(|report| {
                                let mut group = fleet_directory::Group::new(report.endpoint);
                                group.apply(
                                    &PageRequest::default(),
                                    report.observation.ok_or_else(|| {
                                        Error::Input(
                                            report.error.expect("failed observation has an error"),
                                        )
                                    }),
                                );
                                group
                            })
                            .collect::<Vec<_>>();
                        this.switcher.catalog = std::sync::Arc::new(switcher::catalog(&groups));
                    }
                    Err(error) => this.switcher.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn close_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.switcher.active = false;
        if let Some(focus) = self.switcher.return_focus.take() {
            focus.focus(window, cx);
        }
        cx.notify();
    }

    fn confirm_switcher(
        &mut self,
        generation: u64,
        entry: &switcher::Entry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.switcher.active
            || self.switcher.generation != generation
            || self.switcher.busy
            || self.export_pending(cx)
        {
            return;
        }
        let Some(index) = self
            .slots
            .iter()
            .position(|slot| slot.endpoint.socket == entry.endpoint.socket)
        else {
            self.switcher.error = Some("The selected Host is no longer configured.".into());
            cx.notify();
            return;
        };
        self.switcher.active = false;
        self.switcher.return_focus = None;
        self.select(index, window, cx);
        self.slots[index]
            .view
            .as_ref()
            .expect("selected Host initialized")
            .update(cx, |view, cx| {
                view.prepare_directory_session(entry.source.clone(), window, cx);
            });
    }

    pub(super) fn focus_switcher(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.switcher.active {
            self.switcher.palette.focus_handle(cx).focus(window, cx);
        }
    }

    pub(super) fn switcher_view(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        if !self.switcher.active {
            return div().into_any_element();
        }
        let generation = self.switcher.generation;
        let entries = self.switcher.catalog.clone();
        let items = entries
            .entries
            .iter()
            .map(|entry| {
                let label = format!("{} · {}", entry.endpoint.name, entry.title);
                let detail = format!(
                    "{} · {} · {}",
                    entry.status, entry.source.thread_id, entry.cwd
                );
                CommandItem::new()
                    .label(label.clone())
                    .keywords([entry.cwd.clone(), entry.source.thread_id.clone()])
                    .child(move |_, _| {
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().truncate().child(label.clone()))
                            .child(div().text_xs().truncate().child(detail.clone()))
                    })
            })
            .collect::<Vec<_>>();
        let owner = cx.entity().downgrade();
        div().id("session-switcher").key_context("FleetpaneSwitcher").absolute().inset_0().occlude().bg(cx.theme().background.opacity(0.96)).p_4()
            .flex().flex_col().gap_2()
            .on_mouse_down(MouseButton::Left, |_,_,cx| cx.stop_propagation())
            .child(div().flex().items_center().gap_2()
                .child(div().flex_1().child("Switch session · Ctrl/Cmd+K · Esc returns to previous input"))
                .child(Button::new("close-switcher").small().label("Close").on_click(cx.listener(|this,_,window,cx|this.close_switcher(window,cx)))))
            .child(div().text_xs().child(if self.switcher.busy {"Reading current session metadata…".into()} else {format!("{} entries · {} loaded rows omitted · {} Hosts have more pages. Search title, Host, cwd or ID; All sessions provides full paging.",entries.entries.len(),entries.omitted,entries.more_hosts)}))
            .child(div().id("switcher-errors").max_h(px(72.)).overflow_y_scroll()
                .children(entries.unavailable.iter().map(|error|div().text_xs().text_color(cx.theme().danger).child(error.clone()))))
            .children(self.switcher.error.iter().map(|error|div().text_xs().text_color(cx.theme().danger).child(error.clone())))
            .child(Palette::new(&self.switcher.palette).items(items)
                .placeholder("Search sessions on configured Hosts")
                .max_h((window.viewport_size().height - px(245.)).clamp(px(100.), px(360.)))
                .on_confirm(move |path,window,cx| {
                    if let Some(entry)=entries.entries.get(path.row) && let Some(owner)=owner.upgrade() {
                        owner.update(cx,|view,cx|view.confirm_switcher(generation,entry,window,cx));
                    }
                }))
            .child(div().text_xs().child("↑/↓ select · Enter opens saved history · No implicit resume, takeover or message."))
            .into_any_element()
    }
}
