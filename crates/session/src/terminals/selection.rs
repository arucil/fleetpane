//! Selection owns only visible cells. Incoming output cannot retarget copied text.
use super::screen::Grid;
use vt100::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point {
    pub row: u16,
    pub col: u16,
}

pub struct Viewport {
    rows: Vec<Vec<Cell>>,
    wrapped: Vec<bool>,
    cursor: Option<Point>,
}

impl Viewport {
    pub fn capture(grid: &Grid) -> Self {
        let (rows, cols) = grid.size();
        Self {
            rows: (0..rows.get())
                .map(|row| {
                    (0..cols.get())
                        .map(|col| grid.cell(row, col).expect("visible terminal cell").clone())
                        .collect()
                })
                .collect(),
            wrapped: (0..rows.get()).map(|row| grid.row_wrapped(row)).collect(),
            cursor: (!grid.hide_cursor() && grid.scrollback() == 0).then(|| {
                let (row, col) = grid.cursor_position();
                Point { row, col }
            }),
        }
    }

    pub fn rows(&self) -> &[Vec<Cell>] {
        &self.rows
    }

    pub fn cursor(&self) -> Option<Point> {
        self.cursor
    }

    pub fn contents(&self) -> String {
        let Some(row) = self
            .rows
            .iter()
            .rposition(|row| row.iter().any(Cell::has_contents))
        else {
            return String::new();
        };
        self.text(
            Point { row: 0, col: 0 },
            Point {
                row: row as u16,
                col: self.rows[row].len() as u16 - 1,
            },
        )
    }

    fn clamp(&self, point: Point) -> Point {
        Point {
            row: point.row.min(self.rows.len() as u16 - 1),
            col: point.col.min(self.rows[0].len() as u16 - 1),
        }
    }

    fn text(&self, start: Point, end: Point) -> String {
        let mut text = String::new();
        for row in start.row..=end.row {
            if row > start.row && !self.wrapped[usize::from(row - 1)] {
                text.push('\n');
            }
            let cells = &self.rows[usize::from(row)];
            let first = if row == start.row {
                usize::from(start.col)
            } else {
                0
            };
            let first = if cells[first].is_wide_continuation() {
                first - 1
            } else {
                first
            };
            let last = if row == end.row {
                usize::from(end.col)
            } else {
                cells.len() - 1
            };
            // Drop unoccupied right padding, preserving spaces actually printed by the PTY.
            if let Some(last) = (first..=last).rfind(|&col| cells[col].has_contents()) {
                for cell in &cells[first..=last] {
                    if cell.is_wide_continuation() {
                        continue;
                    }
                    text.push_str(if cell.has_contents() {
                        cell.contents()
                    } else {
                        " "
                    });
                }
            }
        }
        text
    }
}

pub struct Selection {
    pub viewport: Viewport,
    anchor: Point,
    focus: Point,
}

impl Selection {
    pub fn begin(viewport: Viewport, point: Point) -> Self {
        let point = viewport.clamp(point);
        Self {
            viewport,
            anchor: point,
            focus: point,
        }
    }

    pub fn update(&mut self, point: Point) {
        self.focus = self.viewport.clamp(point);
    }

    pub fn contains(&self, point: Point) -> bool {
        let (start, end) = self.range();
        if point.row < start.row || point.row > end.row {
            return false;
        }
        let cells = &self.viewport.rows[usize::from(point.row)];
        let col = if cells[usize::from(point.col)].is_wide_continuation() {
            point.col - 1
        } else {
            point.col
        };
        let first = if point.row == start.row { start.col } else { 0 };
        let last = if point.row == end.row {
            end.col
        } else {
            cells.len() as u16 - 1
        };
        col <= last && col + u16::from(cells[usize::from(col)].is_wide()) >= first
    }

    fn range(&self) -> (Point, Point) {
        (self.anchor.min(self.focus), self.anchor.max(self.focus))
    }

    pub fn text(&self) -> String {
        let (start, end) = self.range();
        self.viewport.text(start, end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU16;

    fn parser(cols: u16, text: &str) -> vt100::Parser {
        let mut parser = vt100::Parser::new(
            NonZeroU16::new(4).unwrap(),
            NonZeroU16::new(cols).unwrap(),
            1000,
        );
        parser.process(text.as_bytes());
        parser
    }

    #[test]
    fn reverse_unicode_selection_preserves_cells_spaces_and_hard_lines() {
        let parser = parser(20, "A中文 e\u{301}  \r\nnext");
        let mut selection =
            Selection::begin(Viewport::capture(parser.screen()), Point { row: 1, col: 3 });
        selection.update(Point { row: 0, col: 2 });
        assert_eq!(selection.text(), "中文 e\u{301}  \nnext");
        assert_eq!(selection.viewport.contents(), "A中文 e\u{301}  \nnext");
        assert!(selection.contains(Point { row: 0, col: 1 }));
        assert!(selection.contains(Point { row: 0, col: 2 }));
        assert!(!selection.contains(Point { row: 0, col: 0 }));
        let selection =
            Selection::begin(Viewport::capture(parser.screen()), Point { row: 0, col: 4 });
        assert_eq!(selection.text(), "文");
    }

    #[test]
    fn soft_wraps_join_and_frozen_cells_survive_output_resize_and_alternate_screen() {
        let mut parser = parser(6, "abcdef中文e\u{301}\r\nlast");
        let mut selection =
            Selection::begin(Viewport::capture(parser.screen()), Point { row: 0, col: 0 });
        selection.update(Point { row: 1, col: 4 });
        assert_eq!(selection.text(), "abcdef中文e\u{301}");
        parser.process(b"\x1b[?1049hREPLACED\x1b[?1049l\r\nscroll\r\nnew\r\n");
        parser.set_size(NonZeroU16::new(3).unwrap(), NonZeroU16::new(8).unwrap());
        assert_eq!(selection.text(), "abcdef中文e\u{301}");
        assert_eq!(selection.viewport.rows().len(), 4);
        assert_eq!(selection.viewport.rows()[0].len(), 6);
    }

    #[test]
    fn scrollback_capture_and_outside_drag_are_bounded_to_visible_cells() {
        let mut parser = parser(10, "zero\r\none\r\ntwo\r\nthree\r\nfour\r\nfive");
        parser.screen_mut().set_scrollback(2);
        let mut selection =
            Selection::begin(Viewport::capture(parser.screen()), Point { row: 0, col: 0 });
        selection.update(Point {
            row: u16::MAX,
            col: u16::MAX,
        });
        assert_eq!(selection.text(), "zero\none\ntwo\nthree");
        assert_eq!(selection.viewport.cursor(), None);
    }

    #[test]
    fn blank_cells_and_explicit_spaces_remain_distinct() {
        let terminal = parser(10, "\r\n  \r\n\x1b[3Cend");
        let mut selection = Selection::begin(
            Viewport::capture(terminal.screen()),
            Point { row: 0, col: 0 },
        );
        selection.update(Point { row: 2, col: 9 });
        assert_eq!(selection.text(), "\n  \n   end");
        assert_eq!(selection.viewport.contents(), "\n  \n   end");
        let blank = parser(10, "");
        assert_eq!(Viewport::capture(blank.screen()).contents(), "");
    }
}
