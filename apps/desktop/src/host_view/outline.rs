//! Lazy prompt menus over the currently loaded conversation only.
use super::*;

impl HostView {
    pub(super) fn render_outline(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(thread) = &self.thread else {
            return div().into_any_element();
        };
        let session_id = thread.id.clone();
        let host_id = self.snapshot.as_ref().map(|snapshot| snapshot.host_id);
        let owner = cx.entity().downgrade();
        Button::new("loaded-prompt-outline")
            .small()
            .label("Loaded prompts")
            .tooltip("Jump to an observed user message. Earlier history is loaded separately.")
            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, cx| {
                let Some(entity) = owner.upgrade() else {
                    return menu;
                };
                let view = entity.read(cx);
                if view.snapshot.as_ref().map(|snapshot| snapshot.host_id) != host_id {
                    return menu.item(PopupMenuItem::label("The prompt Host has changed"));
                }
                let Some(thread) = view
                    .thread
                    .as_ref()
                    .filter(|thread| thread.id == session_id)
                else {
                    return menu;
                };
                let mut menu = menu.scrollable(true).max_h(px(300.));
                let prompts = match fleetpane_session::outline::loaded(thread) {
                    Ok(prompts) => prompts,
                    Err(error) => {
                        return menu.item(PopupMenuItem::label(format!(
                            "Cannot read loaded prompts: {error}"
                        )));
                    }
                };
                menu = menu.item(PopupMenuItem::label(format!(
                    "{} loaded prompts · earlier pages are not fetched",
                    prompts.len()
                )));
                for (index, prompt) in prompts.into_iter().enumerate() {
                    let label = format!("{} · {}", index + 1, prompt.preview);
                    let session_id = session_id.clone();
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        if let Some(entity) = owner.upgrade() {
                            entity.update(cx, |view, cx| {
                                if view.snapshot.as_ref().map(|snapshot| snapshot.host_id)
                                    != host_id
                                {
                                    view.error =
                                        Some("Prompt was not opened: the Host has changed".into());
                                    cx.notify();
                                    return;
                                }
                                let result = view
                                    .thread
                                    .as_ref()
                                    .ok_or_else(|| {
                                        "The prompt session is no longer loaded".to_owned()
                                    })
                                    .and_then(|thread| {
                                        fleetpane_session::outline::validate(
                                            thread,
                                            &session_id,
                                            &prompt,
                                        )
                                        .map_err(|error| error.to_string())
                                    })
                                    .and_then(|()| {
                                        let key = Row::item_key(
                                            &session_id,
                                            &prompt.turn_id,
                                            &prompt.item_id,
                                        );
                                        view.row_index.get(&key).copied().ok_or_else(|| {
                                            "The user message has no rendered row".to_owned()
                                        })
                                    });
                                match result {
                                    Ok(index) => view.timeline.scroll_to(ListOffset {
                                        item_ix: index,
                                        offset_in_item: px(0.),
                                    }),
                                    Err(error) => {
                                        view.error = Some(format!("Prompt was not opened: {error}"))
                                    }
                                }
                                cx.notify();
                            });
                        }
                    }));
                }
                menu
            })
            .into_any_element()
    }
}
