//! Application-wide device appearance; persistence completes before theme changes.
use crate::{Screen, host_view::background};
use fleetpane_session::{
    appearance::{Preferences, Saved, Theme},
    creations::Store,
};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
    menu::{DropdownMenu, PopupMenuItem},
    theme::{Theme as KitTheme, ThemeMode},
};
use gpui_kit::{prelude::*, *};
use std::path::PathBuf;

#[derive(Clone, Copy)]
enum Pending {
    Loading,
    Saving,
}

pub struct Frame {
    screen: Entity<Screen>,
    directory: Result<PathBuf, String>,
    store: Option<Store>,
    saved: Option<Saved>,
    pending: Option<Pending>,
    closing: bool,
    error: Option<String>,
    _appearance: Subscription,
}

fn apply(theme: Theme, window: &mut Window, cx: &mut App) {
    match theme {
        Theme::System => KitTheme::sync_system_appearance(Some(window), cx),
        Theme::Light => KitTheme::change(ThemeMode::Light, Some(window), cx),
        Theme::Dark => KitTheme::change(ThemeMode::Dark, Some(window), cx),
    }
}

impl Frame {
    pub fn new(
        screen: Entity<Screen>,
        directory: Result<PathBuf, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        apply(Theme::System, window, cx);
        let observer = cx.observe_window_appearance(window, |this, window, cx| {
            if this
                .saved
                .is_none_or(|saved| saved.value.theme == Theme::System)
            {
                apply(Theme::System, window, cx);
            }
        });
        let mut view = Self {
            screen,
            directory,
            store: None,
            saved: None,
            pending: None,
            closing: false,
            error: None,
            _appearance: observer,
        };
        let owner = cx.entity().downgrade();
        // Compose appearance persistence with the existing conversation draft
        // guard; registering only an appearance guard would replace that guard.
        window.on_window_should_close(cx, move |window, cx| {
            owner.upgrade().is_none_or(|view| {
                view.update(cx, |this, cx| {
                    if this.pending.is_some() {
                        this.closing = true;
                        cx.notify();
                        false
                    } else {
                        let allowed = this
                            .screen
                            .update(cx, |screen, cx| screen.request_close(window, cx));
                        cx.notify();
                        allowed
                    }
                })
            })
        });
        view.reload(window, cx);
        view
    }

    fn finish_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing {
            self.closing = false;
            if self.error.is_none()
                && self
                    .screen
                    .update(cx, |screen, cx| screen.request_close(window, cx))
            {
                window.remove_window();
            }
        }
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending.is_some() || self.screen.read(cx).is_closing(cx) {
            return;
        }
        let directory = match &self.directory {
            Ok(directory) => directory.clone(),
            Err(error) => {
                self.error = Some(format!("Appearance preferences unavailable: {error}"));
                cx.notify();
                return;
            }
        };
        self.pending = Some(Pending::Loading);
        self.error = None;
        let task = cx.background_executor().spawn(async move {
            background(async move {
                let store = Store::open(directory).await?;
                let saved = store.appearance().await?;
                Ok((store, saved))
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.pending = None;
                match result {
                    Ok((store, saved)) => {
                        this.store = Some(store);
                        this.saved = Some(saved);
                        apply(saved.value.theme, window, cx);
                    }
                    Err(error) => {
                        this.error = Some(format!("Appearance preferences unavailable: {error}"))
                    }
                }
                this.finish_close(window, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn choose(&mut self, theme: Theme, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending.is_some() || self.screen.read(cx).is_closing(cx) {
            return;
        }
        let (Some(store), Some(saved)) = (self.store.clone(), self.saved) else {
            self.error = Some("Appearance preferences unavailable. Reload before saving.".into());
            cx.notify();
            return;
        };
        self.pending = Some(Pending::Saving);
        self.error = None;
        let task = cx.background_executor().spawn(async move {
            background(store.save_appearance(saved.revision, Preferences { theme }))
        });
        cx.spawn_in(window,async move |this,cx| {
            let result=task.await;
            let _closed=this.update_in(cx,|this,window,cx| {
                this.pending=None;
                match result {
                    Ok(saved) => {this.saved=Some(saved);apply(saved.value.theme,window,cx);}
                    Err(error) => this.error=Some(format!("Theme not saved: {error}. Reload appearance preferences before trying again.")),
                }
                this.finish_close(window,cx);
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
}

impl Render for Frame {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let label = match self.pending {
            Some(Pending::Loading) => "Appearance: loading…".to_string(),
            Some(Pending::Saving) => "Appearance: saving…".to_string(),
            None => self.saved.map_or_else(
                || "Appearance unavailable".into(),
                |saved| format!("Appearance: {}", saved.value.theme.label()),
            ),
        };
        let selected = self.saved.map(|saved| saved.value.theme);
        let unavailable =
            self.pending.is_some() || self.closing || self.screen.read(cx).is_closing(cx);
        let owner = cx.entity().downgrade();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .child(
                div()
                    .px_4()
                    .py_1()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("appearance-choice")
                            .small()
                            .ghost()
                            .label(label)
                            .disabled(unavailable || self.saved.is_none())
                            .dropdown_menu_with_anchor(Anchor::BottomRight, move |menu, _, _| {
                                let mut menu = menu;
                                for theme in [Theme::System, Theme::Light, Theme::Dark] {
                                    let owner = owner.clone();
                                    menu = menu.item(
                                        PopupMenuItem::new(theme.label())
                                            .checked(selected == Some(theme))
                                            .on_click(move |_, window, cx| {
                                                if let Some(view) = owner.upgrade() {
                                                    view.update(cx, |this, cx| {
                                                        this.choose(theme, window, cx)
                                                    });
                                                }
                                            }),
                                    );
                                }
                                menu
                            }),
                    )
                    .child(
                        Button::new("appearance-reload")
                            .small()
                            .ghost()
                            .label("Reload appearance")
                            .disabled(unavailable)
                            .on_click(cx.listener(|this, _, window, cx| this.reload(window, cx))),
                    ),
            )
            .when(self.closing, |view| {
                view.child(
                    div()
                        .px_4()
                        .py_1()
                        .text_sm()
                        .child("Saving appearance before closing…"),
                )
            })
            .children(self.error.iter().map(|error| {
                div()
                    .px_4()
                    .py_1()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone())
            }))
            .child(div().flex_1().min_h_0().child(self.screen.clone()))
    }
}
