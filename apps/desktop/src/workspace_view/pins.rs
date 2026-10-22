//! Pin order is a local preference; persist only an exact observed list.
use super::*;

#[derive(Clone)]
struct Drag {
    owner: EntityId,
    workspace: Uuid,
    expected: Vec<Uuid>,
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

fn moved(ids: &[Uuid], source: Uuid, target: Uuid) -> Option<Vec<Uuid>> {
    let from = ids.iter().position(|id| *id == source)?;
    let to = ids.iter().position(|id| *id == target)?;
    if from == to {
        return None;
    }
    let mut order = ids.to_vec();
    order.remove(from);
    order.insert(to, source);
    Some(order)
}

impl WorkspaceView {
    fn pin_ids(&self) -> Vec<Uuid> {
        self.pins.iter().map(|workspace| workspace.id).collect()
    }

    pub(super) fn pinned_row(
        &self,
        workspace: &Workspace,
        index: usize,
        cx: &Context<Self>,
    ) -> AnyElement {
        let owner = cx.entity_id();
        let id = workspace.id;
        let enabled = !self.busy && self.pin_error.is_none();
        div()
            .id(("pin-drop", index))
            .can_drop(move |value, _, _| {
                enabled
                    && value
                        .downcast_ref::<Drag>()
                        .is_some_and(|drag| drag.owner == owner && drag.workspace != id)
            })
            .drag_over::<Drag>(move |style, drag, _, cx| {
                if enabled && drag.owner == owner && drag.workspace != id {
                    style.bg(cx.theme().secondary)
                } else {
                    style
                }
            })
            .on_drop(cx.listener(move |this, drag: &Drag, window, cx| {
                if drag.owner != cx.entity_id() || this.busy || this.pin_error.is_some() {
                    return;
                }
                if let Some(order) = moved(&drag.expected, drag.workspace, id) {
                    this.run(Action::OrderPins(drag.expected.clone(), order), window, cx);
                }
            }))
            .child(self.workspace_row(workspace, index, true, cx))
            .into_any_element()
    }

    pub(super) fn pin_controls(
        &self,
        workspace: &Workspace,
        index: usize,
        cx: &Context<Self>,
    ) -> Div {
        let expected = self.pin_ids();
        let enabled = !self.busy && self.pin_error.is_none();
        let id = workspace.id;
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .id(("pin-drag", index))
                    .px_2()
                    .text_sm()
                    .when(enabled, |view| {
                        view.on_drag(
                            Drag {
                                owner: cx.entity_id(),
                                workspace: id,
                                expected: expected.clone(),
                                label: workspace.display_name().into_owned(),
                            },
                            |drag, _, _, cx| {
                                cx.stop_propagation();
                                cx.new(|_| drag.clone())
                            },
                        )
                    })
                    .child("↕ Drag"),
            )
            .children(
                [(true, "Up"), (false, "Down")]
                    .into_iter()
                    .map(|(up, label)| {
                        let target = if up {
                            index.checked_sub(1)
                        } else {
                            index.checked_add(1)
                        }
                        .and_then(|index| expected.get(index).copied());
                        let expected = expected.clone();
                        Button::new((if up { "pin-up" } else { "pin-down" }, index))
                            .small()
                            .ghost()
                            .label(label)
                            .disabled(!enabled || target.is_none())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(target) = target
                                    && let Some(order) = moved(&expected, id, target)
                                {
                                    this.run(
                                        Action::OrderPins(expected.clone(), order),
                                        window,
                                        cx,
                                    );
                                }
                            }))
                    }),
            )
    }

    pub(super) fn pin_status(&self, cx: &Context<Self>) -> Div {
        div()
            .when(self.pin_saving, |view| {
                view.child(div().text_sm().child("Saving workspace pins…"))
            })
            .when_some(self.pin_error.clone(), |view, error| {
                view.child(
                    div()
                        .id("catalog-pin-error")
                        .max_h(px(96.))
                        .overflow_y_scroll()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().danger)
                                .child(format!("Workspace pin save: {error}")),
                        )
                        .child(
                            Button::new("reload-catalog-pins")
                                .small()
                                .label("Reload pins")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.run(Action::ReloadPins, window, cx)
                                })),
                        )
                        .child(
                            Button::new("keep-catalog-pins")
                                .small()
                                .label("Keep saved pins")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.pin_error = None;
                                    cx.emit(NamesUpdated(vec![]));
                                    cx.notify();
                                })),
                        ),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::moved;
    use uuid::Uuid;

    #[test]
    fn pin_moves_use_saved_identities_and_preserve_other_positions() {
        let ids: Vec<_> = (0..4).map(|_| Uuid::new_v4()).collect();
        assert_eq!(
            moved(&ids, ids[0], ids[2]).unwrap(),
            [ids[1], ids[2], ids[0], ids[3]]
        );
        assert_eq!(
            moved(&ids, ids[3], ids[0]).unwrap(),
            [ids[3], ids[0], ids[1], ids[2]]
        );
        assert_eq!(moved(&ids, ids[1], ids[1]), None);
        assert_eq!(moved(&ids, Uuid::new_v4(), ids[0]), None);
        assert_eq!(moved(&ids, ids[0], Uuid::new_v4()), None);
        assert_eq!(moved(&[], ids[0], ids[1]), None);
    }
}
