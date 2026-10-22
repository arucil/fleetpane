//! A fresh observational read precedes each explicit global request navigation.
use super::*;
use gpui_kit::component::WindowExt;

fn shortcut(key: &Keystroke) -> bool {
    let mods = key.modifiers;
    let primary = if cfg!(target_os = "macos") {
        mods.platform && !mods.control
    } else {
        mods.control && !mods.platform
    };
    primary && mods.shift && !mods.alt && !mods.function && key.key == "j"
}

impl Workbench {
    pub(in crate::workbench) fn next_request_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !shortcut(&event.keystroke)
            || event.prefer_character_input
            || self.next_request_blocked(window, cx)
        {
            return;
        }
        // One gesture starts one read; held keys never forward bytes to a PTY.
        cx.stop_propagation();
        if !event.is_held {
            self.next_request(window, cx);
        }
    }

    pub(super) fn next_request_blocked(&self, window: &mut Window, cx: &mut App) -> bool {
        self.switcher.active
            || self.notifications.open
            || self.closing
            || window.has_active_dialog(cx)
            || window.has_active_sheet(cx)
            || window
                .context_stack()
                .iter()
                .any(|context| context.contains("PopupMenu") || context.contains("SearchPanel"))
            || crate::input_composition::active(&self.workspace, window, cx)
            || self.sessions.composing(window, cx)
            || (self.tabs_active() && self.tab_shortcut_blocked(window, cx))
            || (!self.tabs_active()
                && !self.choosing
                && !self.browsing_workspaces
                && !self.attention.active
                && !self.sessions.active
                && !self.diagnostics.active
                && self.file_navigation.view.is_none()
                && self.slots[self.selected].view.as_ref().is_some_and(|view| {
                    view.update(cx, |view, cx| view.tab_shortcut_blocked(window, cx))
                }))
            || self.file_navigation.view.as_ref().is_some_and(|view| {
                view.update(cx, |view, cx| view.tab_shortcut_blocked(window, cx))
            })
            || (self.browsing_workspaces
                && self.workspace_view.as_ref().is_some_and(|view| {
                    view.update(cx, |view, cx| view.shortcut_blocked(window, cx))
                }))
    }

    fn next_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.export_pending(cx)
            || self.next_request_blocked(window, cx)
            || self.attention.next_pending.is_some()
            || self.attention.opening.is_some()
            || self.attention.review_busy
        {
            return;
        }
        self.attention.next_pending = Some(uuid::Uuid::new_v4());
        self.attention.show_completions = false;
        self.attention.selection_notice = None;
        self.open_attention(window, cx);
    }

    pub(super) fn finish_next_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.attention.next_pending = None;
        if self.attention.error.is_some() {
            return;
        }
        if self.next_request_blocked(window, cx) || self.export_pending(cx) {
            self.attention.navigation_error = Some(
                "Navigation cancelled while another operation is active. Choose the request again."
                    .into(),
            );
            return;
        }
        if let Some(index) = self.attention.next_cursor.cycle_next(&self.attention.items) {
            let item = self.attention.items[index].clone();
            self.open_attention_item(item, window, cx);
        } else {
            self.attention.selection_notice = Some(
                "No pending request with a session target was observed. Check unavailable Hosts above."
                    .into(),
            );
        }
    }

    pub(in crate::workbench) fn next_request_button(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        Button::new("next-pending-request")
            .small()
            .label("Next request")
            .tooltip(if cfg!(target_os = "macos") {
                "Cmd+Shift+J · Check all Hosts and open the next pending request"
            } else {
                "Ctrl+Shift+J · Check all Hosts and open the next pending request"
            })
            .disabled(
                self.closing
                    || self.switcher.active
                    || self.attention.next_pending.is_some()
                    || self.attention.opening.is_some()
                    || self.attention.review_busy,
            )
            .on_click(cx.listener(|this, _, window, cx| this.next_request(window, cx)))
    }
}

#[cfg(test)]
mod tests {
    use super::shortcut;
    use gpui_kit::Keystroke;

    #[test]
    fn next_request_requires_exact_modifiers() {
        let primary = if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        };
        assert!(shortcut(
            &Keystroke::parse(&format!("{primary}-shift-j")).unwrap()
        ));
        for key in [
            "j",
            "shift-j",
            "ctrl-j",
            "cmd-j",
            "ctrl-alt-shift-j",
            "cmd-alt-shift-j",
            "ctrl-cmd-shift-j",
            "ctrl-shift-k",
        ] {
            assert!(!shortcut(&Keystroke::parse(key).unwrap()), "{key}");
        }
    }
}
