use fleetpane_protocol::{Agent, DirectoryPage};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::{
    App, Bounds, ClipboardItem, Context, FocusHandle, KeyDownEvent, ScrollStrategy, SharedString,
    TitlebarOptions, UniformListScrollHandle, Window, WindowBounds, WindowOptions, div, prelude::*,
    px, size, uniform_list,
};
use std::net::SocketAddr;

const MAX_DIRECTORY: usize = 5_000;

#[cfg(unix)]
mod appearance;
#[cfg(unix)]
mod host_view;

#[cfg(unix)]
mod file_locations;
#[cfg(unix)]
mod files_view;
#[cfg(unix)]
mod paseo_import_view;
#[cfg(unix)]
mod session_tree_view;
#[cfg(unix)]
mod terminal_view;
#[cfg(unix)]
mod workbench;
#[cfg(unix)]
mod workspace_view;

#[cfg(unix)]
mod conversation_item;

#[cfg(unix)]
mod question_view;

enum Screen {
    Directory(gpui_kit::Entity<Fleetpane>),
    #[cfg(unix)]
    Host(gpui_kit::Entity<workbench::Workbench>),
}

#[cfg(unix)]
impl Screen {
    fn is_closing(&self, cx: &App) -> bool {
        match self {
            Self::Directory(_) => false,
            Self::Host(view) => view.read(cx).is_closing(),
        }
    }

    fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match self {
            Self::Directory(_) => true,
            Self::Host(view) => view.update(cx, |view, cx| view.request_close(window, cx)),
        }
    }
}

impl Render for Screen {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        match self {
            Self::Directory(view) => view.clone().into_any_element(),
            #[cfg(unix)]
            Self::Host(view) => view.clone().into_any_element(),
        }
    }
}

struct Fleetpane {
    address: SocketAddr,
    demo: bool,
    agents: Vec<Agent>,
    selected: Option<usize>,
    next_cursor: Option<String>,
    loading: bool,
    error: Option<String>,
    copied_id: Option<String>,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
}

impl Fleetpane {
    fn new(address: SocketAddr, demo: bool, cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            address,
            demo,
            agents: Vec::new(),
            selected: None,
            next_cursor: None,
            loading: false,
            error: None,
            copied_id: None,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
        };
        if demo {
            view.agents = (0..10_000)
                .map(|index| Agent {
                    id: format!("synthetic-{index:05}"),
                    provider: "codex".into(),
                    cwd: "/example/workspace".into(),
                    title: Some(format!("Session {index:05} · 原生工作区")),
                    status: if index % 7 == 0 { "running" } else { "idle" }.into(),
                    model: Some("synthetic-model".into()),
                    updated_at: "synthetic fixture".into(),
                })
                .collect();
            view.selected = Some(0);
        } else {
            view.load(true, cx);
        }
        view
    }

    fn load(&mut self, reset: bool, cx: &mut Context<Self>) {
        if self.demo || self.loading || (!reset && self.next_cursor.is_none()) {
            return;
        }
        if !reset && self.agents.len() >= MAX_DIRECTORY {
            self.error = Some(
                "Directory limit reached (5,000). Narrowed queries are not implemented yet.".into(),
            );
            cx.notify();
            return;
        }
        self.loading = true;
        self.error = None;
        let address = self.address;
        let cursor = if reset {
            None
        } else {
            self.next_cursor.clone()
        };
        let task = cx
            .background_executor()
            .spawn(async move { fleetpane_client::fetch_directory(address, cursor.as_deref()) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            // The window may have closed while the bounded network request ran.
            let _closed = this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(page) => this.apply_page(page, reset),
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn apply_page(&mut self, page: DirectoryPage, reset: bool) {
        let incoming: std::collections::HashSet<_> = page
            .entries
            .iter()
            .map(|entry| entry.agent.id.as_str())
            .filter(|id| !self.agents.iter().any(|agent| agent.id == *id))
            .collect();
        if !reset && self.agents.len() + incoming.len() > MAX_DIRECTORY {
            self.error =
                Some("Next page exceeds the 5,000-session limit; page was not loaded.".into());
            return;
        }
        let selected_id = self.selected.map(|index| self.agents[index].id.clone());
        if reset {
            self.agents.clear();
        }
        for entry in page.entries {
            if let Some(agent) = self
                .agents
                .iter_mut()
                .find(|agent| agent.id == entry.agent.id)
            {
                *agent = entry.agent;
            } else {
                self.agents.push(entry.agent);
            }
        }
        self.next_cursor = page.page_info.next_cursor;
        self.selected = selected_id
            .and_then(|id| self.agents.iter().position(|agent| agent.id == id))
            .or_else(|| (!self.agents.is_empty()).then_some(0));
        self.scroll
            .scroll_to_item(self.selected.unwrap_or(0), ScrollStrategy::Top);
    }

    fn copy_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.selected {
            let id = self.agents[index].id.clone();
            cx.write_to_clipboard(ClipboardItem::new_string(id.clone()));
            self.copied_id = Some(id);
            cx.notify();
        }
    }

    fn key(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if key == "r" && event.keystroke.modifiers.control {
            self.load(true, cx);
            return;
        }
        if key == "c" && event.keystroke.modifiers.control {
            self.copy_selected(cx);
            return;
        }
        if self.agents.is_empty() {
            return;
        }
        let current = self.selected.unwrap_or(0);
        let index = match key {
            "down" | "j" => (current + 1).min(self.agents.len() - 1),
            "up" | "k" => current.saturating_sub(1),
            "home" => 0,
            "end" => self.agents.len() - 1,
            _ => return,
        };
        self.selected = Some(index);
        self.scroll.scroll_to_item(index, ScrollStrategy::Center);
        cx.notify();
    }
}

impl Render for Fleetpane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let label = if self.demo {
            "Demo · synthetic data".to_owned()
        } else {
            format!("Paseo · {}", self.address)
        };
        let selected = self.selected.map(|index| self.agents[index].clone());
        let details = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .p_6()
            .flex()
            .flex_col()
            .gap_5()
            .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Session details"))
            .children(selected.map(|agent| {
                let copied = self.copied_id.as_ref() == Some(&agent.id);
                div()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap_4()
                    .child(div().text_2xl().child(agent.display_title().to_owned()))
                    .child(div().text_sm().child(format!("{} / {}", agent.provider, agent.status)))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child(format!("Workspace: {}", agent.cwd)))
                    .child(div().text_sm().child(format!("Session ID: {}", agent.id)))
                    .child(Button::new("copy-id")
                        .label(if copied { "Copied session ID" } else { "Copy session ID" })
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| this.copy_selected(cx))))
            }))
            .when(self.selected.is_none(), |view| {
                view.child(if self.loading { "Loading sessions…" } else { "No session selected" })
            })
            .child(div().mt_auto().text_sm().text_color(cx.theme().muted_foreground)
                .child("Directory preview · Conversation, prompts and terminals are not implemented yet.\n↑ ↓ to select · Home / End · Ctrl+R to refresh · Ctrl+C to copy ID"));

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .child(
                div()
                    .h_16()
                    .px_6()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(div().text_lg().child("Fleetpane"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(label),
                    ),
            )
            .children(self.error.as_ref().map(|error| {
                div()
                    .px_6()
                    .py_3()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone())
            }))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .items_stretch()
                    .child(
                        div()
                            .w_96()
                            .flex_shrink_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .bg(cx.theme().sidebar)
                            .border_r_1()
                            .border_color(cx.theme().border)
                            .child(
                                div()
                                    .px_6()
                                    .py_4()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(format!("Sessions · {}", self.agents.len()))
                                    .children((!self.demo).then(|| {
                                        Button::new("refresh")
                                            .ghost()
                                            .small()
                                            .label("Refresh")
                                            .loading(self.loading)
                                            .disabled(self.loading)
                                            .on_click(
                                                cx.listener(|this, _, _, cx| this.load(true, cx)),
                                            )
                                    })),
                            )
                            .child(
                                uniform_list(
                                    "sessions",
                                    self.agents.len(),
                                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                                        range
                                            .map(|index| {
                                                let agent = &this.agents[index];
                                                let selected = this.selected == Some(index);
                                                div()
                                                    .id(SharedString::from(agent.id.clone()))
                                                    .h_20()
                                                    .px_6()
                                                    .py_3()
                                                    .bg(if selected {
                                                        cx.theme().list_active
                                                    } else {
                                                        cx.theme().sidebar
                                                    })
                                                    .when(!selected, |row| {
                                                        row.hover(|style| {
                                                            style.bg(cx.theme().list_hover)
                                                        })
                                                    })
                                                    .border_b_1()
                                                    .border_color(cx.theme().border)
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            this.selected = Some(index);
                                                            window.focus(&this.focus, cx);
                                                            cx.notify();
                                                        },
                                                    ))
                                                    .child(
                                                        div().text_sm().truncate().child(
                                                            agent.display_title().to_owned(),
                                                        ),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(cx.theme().muted_foreground)
                                                            .child(format!(
                                                                "{} · {}",
                                                                agent.provider, agent.status
                                                            )),
                                                    )
                                            })
                                            .collect()
                                    }),
                                )
                                .track_scroll(&self.scroll)
                                .flex_1(),
                            )
                            .children(self.next_cursor.as_ref().map(|_| {
                                div().px_6().py_4().child(
                                    Button::new("load-more")
                                        .label("Load next page")
                                        .small()
                                        .loading(self.loading)
                                        .disabled(self.loading)
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.load(false, cx)),
                                        ),
                                )
                            })),
                    )
                    .child(details),
            )
    }
}

fn main() {
    let mut address: SocketAddr = "127.0.0.1:6767".parse().expect("constant loopback address");
    let mut demo = false;
    let mut host = None;
    let mut hosts_file = None;
    let mut preferences_directory = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--demo" {
            demo = true;
        } else if arg == "--host" {
            if host.is_some() {
                eprintln!(
                    "--host may be supplied once; use --hosts CONFIG.json for multiple Hosts"
                );
                std::process::exit(2);
            }
            host = Some(std::path::PathBuf::from(args.next().unwrap_or_else(|| {
                eprintln!("--host requires a Unix socket path");
                std::process::exit(2);
            })));
        } else if arg == "--hosts" {
            if hosts_file.is_some() {
                eprintln!("--hosts may be supplied once");
                std::process::exit(2);
            }
            hosts_file = Some(std::path::PathBuf::from(args.next().unwrap_or_else(|| {
                eprintln!("--hosts requires a configuration file path");
                std::process::exit(2);
            })));
        } else if arg == "--preferences-dir" {
            if preferences_directory.is_some() {
                eprintln!("--preferences-dir may be supplied once");
                std::process::exit(2);
            }
            preferences_directory =
                Some(std::path::PathBuf::from(args.next().unwrap_or_else(|| {
                    eprintln!("--preferences-dir requires an absolute directory");
                    std::process::exit(2);
                })));
        } else if arg == "--help" {
            println!(
                "fleetpane-desktop [127.0.0.1:PORT] [--demo]\nfleetpane-desktop --host /absolute/private/control.sock\nfleetpane-desktop --hosts CONFIG.json\nUnix: optional --preferences-dir ABSOLUTE_DIRECTORY overrides device-local appearance storage.\nPaseo directory preview, synthetic demo, or named local Codex/ACP Hosts."
            );
            return;
        } else {
            match arg.parse::<SocketAddr>() {
                Ok(parsed) if parsed.ip().is_loopback() => address = parsed,
                _ => {
                    eprintln!("expected a loopback IP:PORT or --demo");
                    std::process::exit(2);
                }
            }
        }
    }
    if (demo && (host.is_some() || hosts_file.is_some()))
        || (host.is_some() && hosts_file.is_some())
    {
        eprintln!("choose one of --demo, --host or --hosts");
        std::process::exit(2);
    }
    #[cfg(not(unix))]
    if host.is_some() || hosts_file.is_some() || preferences_directory.is_some() {
        eprintln!("Host conversations and persistent appearance currently require Unix");
        std::process::exit(2);
    }
    #[cfg(unix)]
    let preferences_directory = preferences_directory.map_or_else(
        fleetpane_session::appearance::default_directory,
        |path| {
            if path.is_absolute() {
                Ok(path)
            } else {
                Err("--preferences-dir must be absolute".into())
            }
        },
    );
    #[cfg(unix)]
    let creation_directory = hosts_file
        .as_ref()
        .map(|path| path.with_extension("creations"))
        .or_else(|| host.as_ref().map(|path| path.with_extension("controller")))
        .map(|path| {
            std::path::absolute(path).unwrap_or_else(|error| {
                eprintln!("creation store path: {error}");
                std::process::exit(2);
            })
        });
    #[cfg(unix)]
    let hosts = match (host, hosts_file) {
        (Some(socket), None) => Some(fleetpane_session::hosts::Configuration::single(socket)),
        (None, Some(path)) => Some(fleetpane_session::hosts::Configuration::read(&path)),
        (None, None) => None,
        (Some(_), Some(_)) => unreachable!("mutually exclusive arguments checked"),
    }
    .map(|configuration| {
        configuration.unwrap_or_else(|error| {
            eprintln!("{error}");
            std::process::exit(2);
        })
    });
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx: &mut App| {
            gpui_kit::init(cx);
            #[cfg(unix)]
            terminal_view::init(cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    // GPUI's X11 close callback still borrows platform state.
                    // Post the platform action after that callback releases it.
                    cx.spawn(async move |cx| cx.update(|cx| cx.quit())).detach();
                }
            })
            .detach();
            let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
            match gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(900.), px(560.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Fleetpane".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let screen = cx.new(|cx| {
                        #[cfg(unix)]
                        if let Some(configuration) = hosts {
                            return Screen::Host(cx.new(|cx| {
                                workbench::Workbench::new(
                                    configuration,
                                    creation_directory,
                                    window,
                                    cx,
                                )
                            }));
                        }
                        Screen::Directory(cx.new(|cx| {
                            let view = Fleetpane::new(address, demo, cx);
                            window.focus(&view.focus, cx);
                            view
                        }))
                    });
                    #[cfg(unix)]
                    let screen = cx.new(|cx| {
                        appearance::Frame::new(screen, preferences_directory, window, cx)
                    });
                    screen
                },
            ) {
                Ok(_) => cx.activate(true),
                Err(error) => {
                    eprintln!("fleetpane desktop: {error}");
                    // No window exists. GPUI's Linux event loop has not started;
                    // its run() resets an earlier quit signal. Fail at startup.
                    std::process::exit(1);
                }
            }
        });
}
