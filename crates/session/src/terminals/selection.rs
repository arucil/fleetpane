//! Selection owns only visible cells. Incoming output cannot retarget copied text.
use super::screen::Grid;
use vt100::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point {
    pub row: u16,
    pub col: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Cell,
    Word,
    Line,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
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

    fn leading(&self, point: Point) -> Point {
        let mut point = self.clamp(point);
        if self.rows[usize::from(point.row)][usize::from(point.col)].is_wide_continuation() {
            point.col -= 1;
        }
        point
    }

    fn bounds(&self, point: Point, unit: Unit) -> (Point, Point) {
        let point = self.leading(point);
        let mut start = point;
        let mut end = point;
        match unit {
            Unit::Cell => {}
            Unit::Word => {
                let cells = &self.rows[usize::from(point.row)];
                // Keep shell paths/flags together. Whitespace and shell quoting/brackets
                // select one cell, matching the reference terminal's word boundaries.
                let separator = |col: u16| {
                    cells[usize::from(col)]
                        .contents()
                        .chars()
                        .next()
                        .is_none_or(|c| c.is_whitespace() || "()[]{}',\"`".contains(c))
                };
                if !separator(point.col) {
                    while start.col > 0 {
                        let previous = self.leading(Point {
                            col: start.col - 1,
                            ..start
                        });
                        if separator(previous.col) {
                            break;
                        }
                        start = previous;
                    }
                    loop {
                        let next = end.col + 1 + u16::from(cells[usize::from(end.col)].is_wide());
                        if usize::from(next) >= cells.len() || separator(next) {
                            break;
                        }
                        end.col = next;
                    }
                }
            }
            Unit::Line => {
                while start.row > 0 && self.wrapped[usize::from(start.row - 1)] {
                    start.row -= 1;
                }
                while usize::from(end.row) + 1 < self.rows.len()
                    && self.wrapped[usize::from(end.row)]
                {
                    end.row += 1;
                }
                start.col = 0;
                end.col = self.rows[usize::from(end.row)].len() as u16 - 1;
            }
        }
        (start, end)
    }

    fn step(&self, point: Point, motion: Motion) -> Point {
        let mut point = self.leading(point);
        let last_row = self.rows.len() as u16 - 1;
        let last_col = self.rows[usize::from(point.row)].len() as u16 - 1;
        match motion {
            Motion::Left => {
                if point.col > 0 {
                    point.col -= 1;
                } else if point.row > 0 {
                    point.row -= 1;
                    point.col = last_col;
                }
            }
            Motion::Right => {
                let next = point.col
                    + 1
                    + u16::from(
                        self.rows[usize::from(point.row)][usize::from(point.col)].is_wide(),
                    );
                if next <= last_col {
                    point.col = next;
                } else if point.row < last_row {
                    point.row += 1;
                    point.col = 0;
                }
            }
            Motion::Up => point.row = point.row.saturating_sub(1),
            Motion::Down => point.row = (point.row + 1).min(last_row),
            Motion::Home => point.col = 0,
            Motion::End => {
                point.col = self.rows[usize::from(point.row)]
                    .iter()
                    .rposition(Cell::has_contents)
                    .map_or(0, |col| col as u16);
            }
        }
        self.leading(point)
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
    unit: Unit,
    range: (Point, Point),
}

impl Selection {
    pub fn begin(viewport: Viewport, point: Point) -> Self {
        Self::begin_unit(viewport, point, Unit::Cell)
    }

    pub fn begin_unit(viewport: Viewport, point: Point, unit: Unit) -> Self {
        let point = viewport.leading(point);
        Self {
            anchor: point,
            focus: point,
            unit,
            range: viewport.bounds(point, unit),
            viewport,
        }
    }

    pub fn update(&mut self, point: Point) {
        self.focus = self.viewport.leading(point);
        self.refresh_range();
    }

    fn cells(&mut self) {
        if self.unit != Unit::Cell {
            let (start, end) = self.range();
            (self.anchor, self.focus) = if self.focus < self.anchor {
                (end, start)
            } else {
                (start, end)
            };
            self.unit = Unit::Cell;
            self.refresh_range();
        }
    }

    /// Extend an existing range precisely, retaining its opposite anchor edge.
    pub fn extend(&mut self, point: Point) {
        self.cells();
        self.update(point);
    }

    pub fn extend_key(&mut self, motion: Motion) {
        self.cells();
        self.focus = self.viewport.step(self.focus, motion);
        self.refresh_range();
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
        self.range
    }

    fn refresh_range(&mut self) {
        let anchor = self.viewport.bounds(self.anchor, self.unit);
        let focus = self.viewport.bounds(self.focus, self.unit);
        self.range = (anchor.0.min(focus.0), anchor.1.max(focus.1));
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

    #[test]
    fn word_boundaries_preserve_shell_paths_wide_cells_combining_marks_and_separators() {
        let terminal = parser(50, "[src/main.rs] --flag=中文 e\u{301}\tend");
        for (col, expected) in [
            (4, "src/main.rs"),
            (0, "["),
            (13, " "),
            (24, "--flag=中文"),
            (26, "e\u{301}"),
            (29, ""),
        ] {
            let selection = Selection::begin_unit(
                Viewport::capture(terminal.screen()),
                Point { row: 0, col },
                Unit::Word,
            );
            assert_eq!(selection.text(), expected, "column {col}");
        }
        let selection = Selection::begin_unit(
            Viewport::capture(terminal.screen()),
            Point { row: 0, col: 24 },
            Unit::Word,
        );
        assert!(selection.contains(Point { row: 0, col: 21 }));
        assert!(selection.contains(Point { row: 0, col: 24 }));
        assert!(!selection.contains(Point { row: 0, col: 25 }));
    }

    #[test]
    fn word_drags_expand_whole_words_in_both_directions_and_shift_changes_to_cells() {
        let terminal = parser(30, "one two three\r\nfour five");
        let mut selection = Selection::begin_unit(
            Viewport::capture(terminal.screen()),
            Point { row: 0, col: 5 },
            Unit::Word,
        );
        selection.update(Point { row: 0, col: 10 });
        assert_eq!(selection.text(), "two three");
        selection.update(Point { row: 0, col: 1 });
        assert_eq!(selection.text(), "one two");
        selection.extend_key(Motion::Right);
        assert_eq!(selection.text(), "ne two");
        selection.extend(Point { row: 1, col: 2 });
        assert_eq!(selection.text(), "o three\nfou");
    }

    #[test]
    fn line_selection_joins_only_visible_soft_wraps_and_preserves_printed_spaces() {
        let mut terminal = parser(8, "abcdefghij  \r\nnext\r\nlast");
        let mut selection = Selection::begin_unit(
            Viewport::capture(terminal.screen()),
            Point { row: 1, col: 1 },
            Unit::Line,
        );
        assert_eq!(selection.text(), "abcdefghij  ");
        selection.update(Point { row: 2, col: 1 });
        assert_eq!(selection.text(), "abcdefghij  \nnext");
        terminal.process(b"\x1b[2J\x1b[Hchanged");
        assert_eq!(selection.text(), "abcdefghij  \nnext");
        let terminal = parser(8, "zeroabcdefghij\r\nlast\r\nmore\r\n");
        let selection = Selection::begin_unit(
            Viewport::capture(terminal.screen()),
            Point { row: 0, col: 1 },
            Unit::Line,
        );
        assert_eq!(selection.text(), "efghij");
    }

    #[test]
    fn keyboard_extensions_keep_unicode_whole_and_clamp_to_the_fixed_viewport() {
        let terminal = parser(10, "A中文e\u{301}  \r\nnext");
        let mut selection = Selection::begin(
            Viewport::capture(terminal.screen()),
            Point { row: 0, col: 1 },
        );
        selection.extend_key(Motion::Right);
        assert_eq!(selection.text(), "中文");
        selection.extend_key(Motion::Right);
        assert_eq!(selection.text(), "中文e\u{301}");
        selection.extend_key(Motion::Left);
        assert_eq!(selection.text(), "中文");
        selection.extend_key(Motion::Home);
        assert_eq!(selection.text(), "A中");
        selection.extend_key(Motion::Up);
        assert_eq!(selection.text(), "A中");
        selection.extend_key(Motion::End);
        assert_eq!(selection.text(), "中文e\u{301}  ");
        selection.extend_key(Motion::Down);
        selection.extend_key(Motion::End);
        assert_eq!(selection.text(), "中文e\u{301}  \nnext");
        for _ in 0..30 {
            selection.extend_key(Motion::Down);
            selection.extend_key(Motion::Right);
        }
        assert_eq!(selection.range().1, Point { row: 3, col: 9 });
        let mut last = Selection::begin(
            Viewport::capture(terminal.screen()),
            Point { row: 1, col: 0 },
        );
        last.extend_key(Motion::Left);
        assert_eq!(last.text(), "\nn");
    }
}
