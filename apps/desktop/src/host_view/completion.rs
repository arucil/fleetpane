//! Opening a result is observational and never marks it reviewed implicitly.
use super::*;
use fleetpane_session::attention::review::Item;

impl HostView {
    pub(crate) fn prepare_completion(
        &mut self,
        item: Item,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.attention_target = None;
        self.directory_target = None;
        self.completion_target = Some(item);
        self.completion_reveal = true;
        self.open_completion(window, cx);
        cx.notify();
    }

    fn open_completion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() || !self.owns() {
            return;
        }
        let Some(item) = &self.completion_target else {
            return;
        };
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        if let Err(error) = item.validate(snapshot) {
            self.error = Some(error.to_string());
            cx.notify();
            return;
        }
        let thread_id = item.record.thread_id.clone();
        self.completion_reveal = true;
        if self.reveal_completion() {
            cx.notify();
            return;
        }
        self.command(
            Command::Inspect {
                thread_id,
                resume: false,
            },
            window,
            cx,
        );
    }

    pub(super) fn reveal_completion(&mut self) -> bool {
        if !self.completion_reveal {
            return false;
        }
        let Some(item) = &self.completion_target else {
            return false;
        };
        if self
            .thread
            .as_ref()
            .is_none_or(|thread| thread.id != item.record.thread_id)
        {
            return false;
        }
        let key = Row::turn_key(&item.record.thread_id, &item.record.turn_id);
        if let Some(index) = self.row_index.get(&key) {
            scroll_to_turn(&self.timeline, *index);
            self.completion_reveal = false;
            true
        } else {
            false
        }
    }

    pub(super) fn completion_banner(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(item) = &self.completion_target else {
            return div().into_any_element();
        };
        let validation = self
            .snapshot
            .as_ref()
            .map(|snapshot| item.validate(snapshot));
        let valid = matches!(validation, Some(Ok(())));
        let loaded = self.thread.as_ref().is_some_and(|thread| {
            thread.id == item.record.thread_id
                && thread
                    .turns
                    .iter()
                    .any(|turn| turn.id == item.record.turn_id)
        });
        let text = match validation {
            None => "Connecting to the original Host…".into(),
            Some(Err(error)) => error.to_string(),
            Some(Ok(())) if !self.owns() => "Take control explicitly, then open the source history. No turn has been resumed.".into(),
            Some(Ok(())) if !loaded => "Open source history; load older pages if this turn is outside the loaded window.".into(),
            Some(Ok(())) => "The source turn is in the loaded history. Reviewing its result is a separate action in Attention.".into(),
        };
        div()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(div().text_sm().truncate().child(format!(
                "{} · Session {} · Turn {}",
                item.label(),
                item.record.thread_id,
                item.record.turn_id
            )))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().min_w_0().text_xs().child(text))
                    .child(
                        Button::new("completion-source-open")
                            .small()
                            .label("Open result session")
                            .disabled(!valid || !self.owns() || self.busy || self.pending.is_some())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_completion(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("completion-source-dismiss")
                            .small()
                            .ghost()
                            .label("Dismiss source")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.completion_target = None;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }
}

fn scroll_to_turn(timeline: &ListState, index: usize) {
    // GPUI's reveal helper retains Tail follow mode and may snap back after the
    // next layout. An explicit offset suspends following at the chosen turn.
    timeline.scroll_to(ListOffset {
        item_ix: index,
        offset_in_item: px(0.),
    });
}

#[cfg(test)]
mod tests {
    use super::scroll_to_turn;
    use gpui_kit::{FollowMode, ListAlignment, ListState, px};

    #[test]
    fn opening_a_source_turn_suspends_tail_following() {
        let timeline = ListState::new(8, ListAlignment::Bottom, px(400.));
        timeline.set_follow_mode(FollowMode::Tail);
        assert!(timeline.is_following_tail());
        scroll_to_turn(&timeline, 2);
        assert!(
            !timeline.is_following_tail(),
            "the next layout must keep the selected turn visible"
        );
    }
}
