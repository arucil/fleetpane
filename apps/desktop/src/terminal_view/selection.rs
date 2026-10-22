use super::*;
use fleetpane_session::terminals::selection::{Point as CellPoint, Selection, Viewport};
use std::{cell::RefCell, rc::Rc};

pub(super) struct RowGeometry {
    pub bounds: Bounds<Pixels>,
    pub line: ShapedLine,
    pub columns: Vec<(usize, u16)>,
}

#[derive(Default)]
pub(super) struct State {
    pub selected: Option<Selection>,
    pub displayed: Option<Viewport>,
    pub dragging: bool,
    start: Option<CellPoint>,
    moved: bool,
    pub geometry: Rc<RefCell<Vec<Option<RowGeometry>>>>,
}

impl State {
    pub fn clear(&mut self) {
        self.selected = None;
        self.dragging = false;
        self.displayed = None;
        self.start = None;
        self.moved = false;
    }

    pub fn finish(&mut self) {
        if !self.dragging {
            return;
        }
        self.dragging = false;
        if !self.moved {
            self.clear();
        }
    }

    fn hit(&self, position: Point<Pixels>) -> Option<CellPoint> {
        let geometry = self.geometry.borrow();
        let row = geometry
            .iter()
            .position(|row| {
                row.as_ref()
                    .is_some_and(|row| position.y < row.bounds.bottom())
            })
            .or_else(|| geometry.len().checked_sub(1))?;
        let geometry = geometry[row].as_ref()?;
        let byte = geometry
            .line
            .index_for_x((position.x - geometry.bounds.left()).max(px(0.)))
            .unwrap_or(geometry.line.len());
        let col = geometry
            .columns
            .iter()
            .rev()
            .find(|(start, _)| *start <= byte)?
            .1;
        Some(CellPoint {
            row: row as u16,
            col,
        })
    }
}

impl TerminalView {
    pub(super) fn selection_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = &event.keystroke;
        let modifiers = key.modifiers;
        if self.selection.selected.is_some() {
            if !modifiers.alt
                && ((modifiers.control && modifiers.shift && !modifiers.platform)
                    || modifiers.platform)
                && key.key.eq_ignore_ascii_case("c")
            {
                cx.stop_propagation();
                cx.write_to_clipboard(ClipboardItem::new_string(
                    self.selection
                        .selected
                        .as_ref()
                        .expect("selected viewport")
                        .text(),
                ));
                return;
            }
            if key.key == "escape" {
                self.selection.clear();
                cx.stop_propagation();
                cx.notify();
                return;
            }
            // A fixed selection never silently accepts terminal input.
            cx.stop_propagation();
            return;
        }
        self.keyboard_key(event, window, cx);
    }

    pub(super) fn selection_start(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(point) = self.selection.hit(event.position) else {
            return;
        };
        let viewport = match self.selection.selected.take() {
            Some(selection) => selection.viewport,
            None => {
                let Some(viewport) = self.selection.displayed.take() else {
                    return;
                };
                viewport
            }
        };
        self.selection.selected = Some(Selection::begin(viewport, point));
        self.selection.dragging = true;
        self.selection.start = Some(point);
        self.selection.moved = false;
        window.focus(&self.keyboard.focus, cx);
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn selection_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.selection.dragging {
            return;
        }
        if !event.dragging() {
            self.selection.finish();
            return;
        }
        if let Some(point) = self.selection.hit(event.position)
            && let Some(selection) = &mut self.selection.selected
        {
            self.selection.moved |= self.selection.start != Some(point);
            selection.update(point);
            cx.notify();
        }
    }

    pub(super) fn selection_end(
        &mut self,
        event: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.selection.dragging {
            return;
        }
        if let Some(point) = self.selection.hit(event.position)
            && let Some(selection) = &mut self.selection.selected
        {
            self.selection.moved |= self.selection.start != Some(point);
            selection.update(point);
        }
        self.selection.finish();
        cx.notify();
    }
}
