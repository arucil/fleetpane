//! Check the focused Kit editor before changing its workspace.
use gpui_kit::{App, Entity, EntityInputHandler, Focusable, Window};

pub(crate) fn active<T: EntityInputHandler + Focusable>(
    input: &Entity<T>,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    input.read(cx).focus_handle(cx).is_focused(window)
        && input.update(cx, |input, cx| {
            input.marked_text_range(window, cx).is_some()
        })
}
