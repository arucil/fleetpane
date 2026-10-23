use super::*;
use fleetpane_session::terminals::selection::{
    Motion, Point as CellPoint, Selection, Unit, Viewport,
};
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
    keep: bool,
    pub geometry: Rc<RefCell<Vec<Option<RowGeometry>>>>,
}

impl State {
    pub fn clear(&mut self) {
        self.selected = None;
        self.dragging = false;
        self.displayed = None;
        self.start = None;
        self.keep = false;
    }

    pub fn finish(&mut self) {
        if !self.dragging {
            return;
        }
        self.dragging = false;
        if !self.keep {
            // The next click may arrive before another frame. Keep the exact picture
            // available after a focus click so double-clicks don't depend on repaint.
            self.displayed = self.selected.take().map(|selection| selection.viewport);
            self.start = None;
        }
    }

    fn begin(&mut self, point: CellPoint, unit: Unit, extend: bool) {
        let previous = self.selected.take();
        let extending = extend && previous.is_some();
        let selected = match previous {
            Some(mut selection) if extending => {
                selection.extend(point);
                selection
            }
            Some(selection) => Selection::begin_unit(selection.viewport, point, unit),
            None => {
                let Some(viewport) = self.displayed.take() else {
                    return;
                };
                Selection::begin_unit(viewport, point, unit)
            }
        };
        self.selected = Some(selected);
        self.dragging = true;
        self.start = Some(point);
        self.keep = extending || unit != Unit::Cell;
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
            if modifiers.shift && !modifiers.control && !modifiers.alt && !modifiers.platform {
                let motion = match key.key.as_str() {
                    "left" => Some(Motion::Left),
                    "right" => Some(Motion::Right),
                    "up" => Some(Motion::Up),
                    "down" => Some(Motion::Down),
                    "home" => Some(Motion::Home),
                    "end" => Some(Motion::End),
                    _ => None,
                };
                if let Some(motion) = motion {
                    self.selection
                        .selected
                        .as_mut()
                        .expect("selected viewport")
                        .extend_key(motion);
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
            }
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
        let unit = match event.click_count {
            2 => Unit::Word,
            3.. => Unit::Line,
            _ => Unit::Cell,
        };
        self.selection.begin(point, unit, event.modifiers.shift);
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
            cx.notify();
            return;
        }
        if let Some(point) = self.selection.hit(event.position)
            && let Some(selection) = &mut self.selection.selected
        {
            self.selection.keep |= self.selection.start != Some(point);
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
            self.selection.keep |= self.selection.start != Some(point);
            selection.update(point);
        }
        self.selection.finish();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{CellPoint, Info, Output, Screen, State, Unit, Uuid, Viewport};

    #[test]
    fn multiple_clicks_use_the_displayed_picture_without_an_intervening_frame() {
        let mut screen = test_screen();
        feed(&mut screen, b"[src/main.rs] --flag\r\nnext");
        let mut state = State {
            displayed: Some(Viewport::capture(screen.grid())),
            ..State::default()
        };
        let point = CellPoint { row: 0, col: 4 };
        state.begin(point, Unit::Cell, false);
        state.finish();
        assert!(state.selected.is_none());
        // Output and its next paint have not happened yet; the second click still
        // selects the picture from the first click, even after the emulator changes.
        feed(&mut screen, b"\x1b[2J\x1b[Hchanged");
        state.begin(point, Unit::Word, false);
        state.finish();
        assert_eq!(state.selected.as_ref().unwrap().text(), "src/main.rs");
        assert!(!state.dragging);
        state.begin(point, Unit::Line, false);
        state.finish();
        assert_eq!(
            state.selected.as_ref().unwrap().text(),
            "[src/main.rs] --flag"
        );
        state.begin(CellPoint { row: 1, col: 2 }, Unit::Cell, true);
        state.finish();
        assert_eq!(
            state.selected.as_ref().unwrap().text(),
            "[src/main.rs] --flag\nnex"
        );
        state.clear();
        assert!(state.displayed.is_none());
        assert!(state.selected.is_none());
    }

    fn test_screen() -> Screen {
        Screen::new(Info {
            id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            label: "fixture".into(),
            cwd: "/tmp".into(),
            pid: 1,
            rows: 4,
            cols: 30,
            next_input: 0,
            closed: false,
            exit: None,
            input_fault: None,
            output_fault: None,
            output_eof: false,
        })
        .unwrap()
    }

    fn feed(screen: &mut Screen, bytes: &[u8]) {
        let start = screen.cursor();
        screen
            .apply(Output {
                terminal: screen.info.clone(),
                start,
                end: start + bytes.len() as u64,
                dropped: 0,
                bytes: bytes.to_vec(),
            })
            .unwrap();
    }
}
