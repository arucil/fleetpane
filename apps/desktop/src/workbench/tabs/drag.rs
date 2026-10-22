//! Moving tab views retains their entities and exact workspace identities.
use super::*;

#[derive(Clone)]
struct Drag {
    owner: EntityId,
    workspace: Uuid,
    label: String,
}

impl Render for Drag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1()
            .max_w_96()
            .truncate()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary)
            .text_color(cx.theme().foreground)
            .shadow_md()
            .child(self.label.clone())
    }
}

fn destination(ids: &[Uuid], source: Uuid, target: Uuid) -> Option<(usize, usize)> {
    let from = ids.iter().position(|id| *id == source)?;
    let to = ids.iter().position(|id| *id == target)?;
    (from != to).then_some((from, to))
}

impl Workbench {
    pub(in crate::workbench::tabs) fn workspace_tab_button(
        &self,
        index: usize,
        id: Uuid,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let owner = cx.entity_id();
        let dragging = self.tabs_active() && !self.closing && !self.switcher.active;
        div()
            .id(("workspace-tab-container", index))
            .flex()
            .flex_shrink_0()
            .gap_1()
            .can_drop(move |value, _, _| {
                dragging
                    && value
                        .downcast_ref::<Drag>()
                        .is_some_and(|drag| drag.owner == owner && drag.workspace != id)
            })
            .drag_over::<Drag>(move |style, drag, _, cx| {
                if dragging && drag.owner == owner && drag.workspace != id {
                    style.bg(cx.theme().secondary)
                } else {
                    style
                }
            })
            .on_drop(cx.listener(move |this, drag: &Drag, window, cx| {
                this.drop_workspace_tab(drag, id, window, cx)
            }))
            .child(
                div()
                    .id(("workspace-tab-drag", index))
                    .when(dragging, |view| {
                        view.on_drag(
                            Drag {
                                owner,
                                workspace: id,
                                label: label.clone(),
                            },
                            |drag, _, _, cx| {
                                cx.stop_propagation();
                                cx.new(|_| drag.clone())
                            },
                        )
                    })
                    .child(
                        Button::new(("workspace-tab", index))
                            .small()
                            .ghost()
                            .label(label)
                            .tooltip("Drag to reorder workspace views")
                            .selected(self.tabs_active() && self.tabs.selected == Some(id))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_workspace_tab(id, window, cx)
                            })),
                    ),
            )
            .child(
                Button::new(("close-workspace-tab", index))
                    .small()
                    .ghost()
                    .label("×")
                    .tooltip("Close view; keep tasks and terminals running")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.close_workspace_tab(id, window, cx)
                    })),
            )
            .into_any_element()
    }

    fn drop_workspace_tab(
        &mut self,
        drag: &Drag,
        target: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if drag.owner != cx.entity_id()
            || !self.tabs_active()
            || self.switcher.active
            || self.closing
            || self.export_pending(cx)
        {
            return;
        }
        let ids: Vec<_> = self
            .tabs
            .entries
            .iter()
            .map(|e| e.saved.workspace)
            .collect();
        let Some((from, to)) = destination(&ids, drag.workspace, target) else {
            return;
        };
        let entry = self.tabs.entries.remove(from);
        self.tabs.entries.insert(to, entry);
        self.tabs.scroll.scroll_to_item(to);
        self.capture_tabs(window, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::destination;
    use uuid::Uuid;

    #[test]
    fn reordering_uses_current_identities_and_preserves_intervening_order() {
        let ids: Vec<_> = (0..5).map(|_| Uuid::new_v4()).collect();
        let mut rows = ids.clone();
        let (from, to) = destination(&rows, ids[0], ids[3]).unwrap();
        let item = rows.remove(from);
        rows.insert(to, item);
        assert_eq!(rows, [ids[1], ids[2], ids[3], ids[0], ids[4]]);
        let (from, to) = destination(&rows, ids[0], ids[1]).unwrap();
        let item = rows.remove(from);
        rows.insert(to, item);
        assert_eq!(rows, ids);
        assert_eq!(destination(&rows, ids[2], ids[2]), None);
        assert_eq!(destination(&rows, Uuid::new_v4(), ids[0]), None);
        assert_eq!(destination(&rows, ids[0], Uuid::new_v4()), None);
        assert_eq!(destination(&[], ids[0], ids[1]), None);
    }
}
