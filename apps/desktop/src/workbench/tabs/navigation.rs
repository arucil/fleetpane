//! Presentation-only tab commands. Source identities and close guards stay in tabs.
use super::*;
use gpui_kit::component::WindowExt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Step(bool),
    Move(bool),
    Position(usize),
    Last,
    Close,
}

fn command(key: &Keystroke) -> Option<Command> {
    let mods = key.modifiers;
    let primary = if cfg!(target_os = "macos") {
        mods.platform && !mods.control
    } else {
        mods.control && !mods.platform
    };
    if !primary || mods.alt || mods.function {
        return None;
    }
    match key.key.as_str() {
        "pageup" => Some(if mods.shift {
            Command::Move(false)
        } else {
            Command::Step(false)
        }),
        "pagedown" => Some(if mods.shift {
            Command::Move(true)
        } else {
            Command::Step(true)
        }),
        "w" if mods.shift == !cfg!(target_os = "macos") => Some(Command::Close),
        "9" if !mods.shift => Some(Command::Last),
        digit if !mods.shift && matches!(digit, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8") => {
            Some(Command::Position(usize::from(digit.as_bytes()[0] - b'1')))
        }
        _ => None,
    }
}

fn step(index: usize, count: usize, forward: bool, wrap: bool) -> Option<usize> {
    if index >= count {
        return None;
    }
    if forward {
        if index + 1 < count {
            Some(index + 1)
        } else {
            wrap.then_some(0)
        }
    } else if index > 0 {
        Some(index - 1)
    } else {
        wrap.then_some(count - 1)
    }
}

impl Workbench {
    pub(in crate::workbench) fn tab_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(command) = command(&event.keystroke) else {
            return;
        };
        if event.prefer_character_input
            || !self.tabs_active()
            || self.switcher.active
            || self.closing
            || window.has_active_dialog(cx)
            || window.has_active_sheet(cx)
            || window
                .context_stack()
                .iter()
                .any(|context| context.contains("PopupMenu") || context.contains("SearchPanel"))
        {
            return;
        }
        if self.tab_shortcut_blocked(window, cx) {
            return;
        }
        // Consume even a refused close: the same chord must never enter a shell.
        cx.stop_propagation();
        if !event.is_held {
            self.tab_command(command, window, cx);
        }
    }

    pub(in crate::workbench) fn tab_shortcut_blocked(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let Some(entry) = self
            .tabs
            .entries
            .iter()
            .find(|entry| Some(entry.saved.workspace) == self.tabs.selected)
        else {
            return false;
        };
        match entry.saved.panel {
            fleetpane_session::workspace_layout::Panel::Conversation => {
                entry.host.as_ref().is_some_and(|view| {
                    view.update(cx, |view, cx| view.tab_shortcut_blocked(window, cx))
                })
            }
            fleetpane_session::workspace_layout::Panel::Files => {
                entry.files.as_ref().is_some_and(|view| {
                    view.update(cx, |view, cx| view.tab_shortcut_blocked(window, cx))
                })
            }
            fleetpane_session::workspace_layout::Panel::Terminal => {
                entry.terminals.as_ref().is_some_and(|view| {
                    view.update(cx, |view, cx| view.tab_shortcut_blocked(window, cx))
                })
            }
        }
    }

    fn tab_command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tabs_active() || self.switcher.active || self.closing || self.export_pending(cx) {
            return;
        }
        let Some(index) = self
            .tabs
            .entries
            .iter()
            .position(|entry| Some(entry.saved.workspace) == self.tabs.selected)
        else {
            return;
        };
        let count = self.tabs.entries.len();
        match command {
            Command::Close => {
                self.close_workspace_tab(self.tabs.entries[index].saved.workspace, window, cx)
            }
            Command::Move(forward) => {
                if let Some(next) = step(index, count, forward, false) {
                    self.tabs.entries.swap(index, next);
                    self.tabs.scroll.scroll_to_item(next);
                    self.capture_tabs(window, cx);
                    cx.notify();
                }
            }
            selection => {
                let next = match selection {
                    Command::Step(forward) => step(index, count, forward, true),
                    Command::Position(position) => (position < count).then_some(position),
                    Command::Last => Some(count - 1),
                    _ => unreachable!(),
                };
                if let Some(next) = next
                    && next != index
                {
                    self.select_workspace_tab(self.tabs.entries[next].saved.workspace, window, cx);
                }
            }
        }
    }

    pub(super) fn tab_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.entity().downgrade();
        let primary = if cfg!(target_os = "macos") {
            "Cmd"
        } else {
            "Ctrl"
        };
        let close = if cfg!(target_os = "macos") {
            "Cmd+W"
        } else {
            "Ctrl+Shift+W"
        };
        Button::new("workspace-tab-menu")
            .small()
            .label("Tabs")
            .disabled(!self.tabs_active() || self.switcher.active || self.closing)
            .tooltip(format!(
                "{primary}+1…8 selects a tab · {primary}+9 selects the last tab"
            ))
            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |mut menu, _, _| {
                for (command, label) in [
                    (
                        Command::Step(false),
                        format!("Previous tab · {primary}+PageUp"),
                    ),
                    (
                        Command::Step(true),
                        format!("Next tab · {primary}+PageDown"),
                    ),
                    (
                        Command::Move(false),
                        format!("Move tab left · {primary}+Shift+PageUp"),
                    ),
                    (
                        Command::Move(true),
                        format!("Move tab right · {primary}+Shift+PageDown"),
                    ),
                    (Command::Close, format!("Close tab · {close}")),
                ] {
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                        if let Some(owner) = owner.upgrade() {
                            owner.update(cx, |this, cx| this.tab_command(command, window, cx));
                        }
                    }));
                }
                menu
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, command, step};
    use gpui_kit::Keystroke;

    #[test]
    fn selection_wraps_but_reordering_stops_at_edges() {
        assert_eq!(step(0, 0, true, true), None);
        assert_eq!(step(3, 3, true, true), None);
        assert_eq!(step(0, 1, false, true), Some(0));
        assert_eq!(step(0, 1, true, false), None);
        assert_eq!(step(0, 3, false, true), Some(2));
        assert_eq!(step(2, 3, true, true), Some(0));
        assert_eq!(step(0, 3, false, false), None);
        assert_eq!(step(2, 3, true, false), None);
        assert_eq!(step(1, 3, false, false), Some(0));
    }

    #[test]
    fn shortcuts_require_exact_modifiers_and_keep_editor_keys() {
        let primary = if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        };
        for (chord, expected) in [
            (format!("{primary}-pageup"), Some(Command::Step(false))),
            (
                format!("{primary}-shift-pagedown"),
                Some(Command::Move(true)),
            ),
            (format!("{primary}-1"), Some(Command::Position(0))),
            (format!("{primary}-8"), Some(Command::Position(7))),
            (format!("{primary}-9"), Some(Command::Last)),
            (
                if cfg!(target_os = "macos") {
                    "cmd-w".into()
                } else {
                    "ctrl-shift-w".into()
                },
                Some(Command::Close),
            ),
            ("pageup".into(), None),
            ("ctrl-tab".into(), None),
            (format!("{primary}-alt-1"), None),
            (format!("{primary}-shift-1"), None),
            (format!("{primary}-0"), None),
            ("ctrl-alt-w".into(), None),
        ] {
            assert_eq!(
                command(&Keystroke::parse(&chord).unwrap()),
                expected,
                "{chord}"
            );
        }
    }
}
