//! Advertised ACP commands prepare an existing draft; ordinary Send owns execution.
use super::*;

impl HostView {
    pub(super) fn render_commands(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(thread) = self.thread.as_ref().filter(|_| self.is_acp()) else {
            return div().into_any_element();
        };
        let Some(commands) = &thread.acp_commands else {
            return div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("This session has not advertised commands.")
                .into_any_element();
        };
        let body = self.composer.read(cx).value();
        let can_insert = self.draft_editable()
            && !self.draft.importing
            && !self.draft.picking
            && thread.status.kind != "archived"
            && (body.is_empty()
                || (body.starts_with('/')
                    && !body.chars().any(|ch| ch.is_whitespace() || ch.is_control())));
        let session = thread.id.clone();
        let copy_session = session.clone();
        let owner = cx.entity().downgrade();
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                Button::new("advertised-commands")
                    .small()
                    .label(format!("Commands · {}", commands.len()))
                    .disabled(!can_insert || commands.is_empty())
                    .tooltip("Choose a command to fill an empty draft or replace its slash prefix. It is sent only with Send.")
                    .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, cx| {
                        let Some(entity) = owner.upgrade() else { return menu; };
                        let view = entity.read(cx);
                        let Some(thread) = view.thread.as_ref().filter(|thread| thread.id == session) else { return menu; };
                        let Some(commands) = &thread.acp_commands else { return menu; };
                        let mut menu = menu.scrollable(true).max_h(px(300.));
                        for command in commands {
                            let name = command.name.clone();
                            let target = session.clone();
                            let owner = owner.clone();
                            let hint = command.input.as_ref().map_or("", |input| input.hint.as_str());
                            let label = format!("/{} {hint} · {}", command.name, command.description);
                            menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                                if let Some(entity) = owner.upgrade() {
                                    entity.update(cx, |view, cx| {
                                        if !view.draft_editable() || view.draft.importing || view.draft.picking { return; }
                                        let text = view.composer.read(cx).value().to_string();
                                        let result = view.thread.as_ref()
                                            .filter(|thread| thread.id == target && thread.status.kind != "archived")
                                            .ok_or_else(|| fleetpane_session::Error::Projection("Command source is no longer selected".into()))
                                            .and_then(|thread| fleetpane_session::commands::prepare_prefix(thread, &name, &text));
                                        match result {
                                            Ok(text) => {
                                                view.composer.update(cx, |input, cx| {input.set_value(text, window, cx); input.focus(window, cx);});
                                            }
                                            Err(error) => view.error = Some(format!("Command was not inserted: {error}")),
                                        }
                                        cx.notify();
                                    });
                                }
                            }));
                        }
                        menu
                    }),
            )
            .child(Button::new("copy-advertised-commands").small().ghost().label("Copy commands")
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(commands) = this.thread.as_ref()
                        .filter(|thread| thread.id == copy_session)
                        .and_then(|thread| thread.acp_commands.as_ref()) {
                        let raw = serde_json::to_string_pretty(commands).expect("typed commands serialize");
                        cx.write_to_clipboard(ClipboardItem::new_string(raw));
                    }
                })))
            .child(div().text_xs().text_color(cx.theme().muted_foreground)
                .child(if commands.is_empty() { "Provider advertised an empty command list." }
                    else if can_insert { "Selection fills the draft. Send runs it." }
                    else { "Use an empty draft or slash prefix to insert; existing text and images are kept." }))
            .into_any_element()
    }
}
