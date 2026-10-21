//! Bounded, incremental screen emulation. No toolkit or PTY ownership here.
use super::{Info, Output};
use std::num::NonZeroU16;
pub use vt100::{Color, Screen as Grid};

pub const MAX_ROWS: u16 = 60;
pub const MAX_COLS: u16 = 200;
pub const SCROLLBACK: usize = 1000;

pub struct Screen {
    pub info: Info,
    parser: vt100::Parser,
    cursor: u64,
    lost: u64,
}

fn dimensions(info: &Info) -> Result<(NonZeroU16, NonZeroU16), String> {
    if info.rows > MAX_ROWS || info.cols > MAX_COLS {
        return Err("Native screen supports up to 60 rows and 200 columns; resize the PTY before attaching.".into());
    }
    Ok((
        NonZeroU16::new(info.rows).ok_or("terminal has zero rows")?,
        NonZeroU16::new(info.cols).ok_or("terminal has zero columns")?,
    ))
}

impl Screen {
    pub fn new(info: Info) -> Result<Self, String> {
        let (rows, cols) = dimensions(&info)?;
        Ok(Self {
            info,
            parser: vt100::Parser::new(rows, cols, SCROLLBACK),
            cursor: 0,
            lost: 0,
        })
    }

    pub fn cursor(&self) -> u64 {
        self.cursor
    }
    pub fn lost(&self) -> u64 {
        self.lost
    }
    pub fn grid(&self) -> &Grid {
        self.parser.screen()
    }

    pub fn scroll(&mut self, rows: i32) {
        let screen = self.parser.screen_mut();
        let offset = screen.scrollback().saturating_add_signed(rows as isize);
        screen.set_scrollback(offset.min(SCROLLBACK));
    }

    pub fn live(&mut self) {
        self.parser.screen_mut().set_scrollback(0);
    }

    pub fn apply(&mut self, output: Output) -> Result<(), String> {
        if output.terminal.id != self.info.id
            || output.terminal.workspace_id != self.info.workspace_id
            || output.terminal.pid != self.info.pid
        {
            return Err("terminal output belongs to a different shell".into());
        }
        if output.start.checked_sub(self.cursor) != Some(output.dropped)
            || output.end.checked_sub(output.start) != Some(output.bytes.len() as u64)
            || output.bytes.len() > fleetpane_runtime::terminal::MAX_OUTPUT
        {
            return Err("terminal output cursor/length does not match the requested range".into());
        }
        let (rows, cols) = dimensions(&output.terminal)?;
        if output.dropped > 0 {
            self.lost = self
                .lost
                .checked_add(output.dropped)
                .ok_or("terminal lost-byte counter exhausted")?;
            // A lost prefix invalidates both escape parsing and partial UTF-8.
            self.parser = vt100::Parser::new(rows, cols, SCROLLBACK);
        } else if self.parser.screen().size() != (rows, cols) {
            self.parser.set_size(rows, cols);
        }
        self.parser.process(&output.bytes);
        self.cursor = output.end;
        self.info = output.terminal;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn info() -> Info {
        Info {
            id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            cwd: "/tmp/synthetic".into(),
            label: "fixture".into(),
            pid: 123,
            rows: 24,
            cols: 80,
            next_input: 0,
            input_fault: None,
            exit: None,
            output_eof: false,
            output_fault: None,
            closed: false,
        }
    }
    fn feed(screen: &mut Screen, bytes: &[u8]) {
        let start = screen.cursor();
        screen
            .apply(Output {
                terminal: screen.info.clone(),
                start,
                end: start + bytes.len() as u64,
                dropped: 0,
                bytes: bytes.into(),
            })
            .unwrap();
    }

    #[test]
    fn split_utf8_and_escapes_reconstruct_overwrites_and_alternate_screen() {
        let mut screen = Screen::new(info()).unwrap();
        let text = "中文 e\u{301}";
        for byte in format!("\x1b[31m{text}\x1b[m").bytes() {
            feed(&mut screen, &[byte]);
        }
        assert_eq!(screen.grid().contents(), text);
        assert_eq!(screen.grid().cell(0, 0).unwrap().fgcolor(), Color::Idx(1));
        assert!(screen.grid().cell(0, 1).unwrap().is_wide_continuation());
        feed(&mut screen, b"\r\x1b[2Kprogress 10%\rprogress 20%");
        assert_eq!(screen.grid().contents(), "progress 20%");
        feed(&mut screen, b"\x1b[?1049h\x1b[HALTERNATE");
        assert!(screen.grid().alternate_screen());
        assert_eq!(screen.grid().contents(), "ALTERNATE");
        feed(&mut screen, b"\x1b[?1049l");
        assert_eq!(screen.grid().contents(), "progress 20%");
    }

    #[test]
    fn cursor_loss_resets_parser_and_remains_visible_without_duplicate_output() {
        let mut screen = Screen::new(info()).unwrap();
        feed(&mut screen, b"old\x1b[3");
        let start = screen.cursor() + 100;
        screen
            .apply(Output {
                terminal: screen.info.clone(),
                start,
                end: start + 3,
                dropped: 100,
                bytes: b"new".to_vec(),
            })
            .unwrap();
        assert_eq!(screen.grid().contents(), "new");
        assert_eq!(screen.lost(), 100);
        let before = screen.cursor();
        feed(&mut screen, b"");
        assert_eq!(screen.cursor(), before);
        let mut wrong = screen.info.clone();
        wrong.id = Uuid::new_v4();
        assert!(
            screen
                .apply(Output {
                    terminal: wrong,
                    start: before,
                    end: before,
                    dropped: 0,
                    bytes: vec![]
                })
                .is_err()
        );
        assert!(
            screen
                .apply(Output {
                    terminal: screen.info.clone(),
                    start: before - 1,
                    end: before,
                    dropped: 0,
                    bytes: vec![1]
                })
                .is_err()
        );
        assert_eq!(screen.grid().contents(), "new");
    }

    #[test]
    fn screen_scrollback_and_dimensions_are_bounded() {
        let mut screen = Screen::new(info()).unwrap();
        for i in 0..2000 {
            feed(&mut screen, format!("line {i}\r\n").as_bytes());
        }
        assert!(screen.grid().scrollback_len() <= SCROLLBACK);
        screen.scroll(i32::MAX);
        assert!(screen.grid().scrollback() <= SCROLLBACK);
        screen.live();
        assert_eq!(screen.grid().scrollback(), 0);
        let mut bad = info();
        bad.rows = 0;
        assert!(Screen::new(bad).is_err());
        let mut bad = info();
        bad.cols = MAX_COLS + 1;
        assert!(Screen::new(bad).is_err());
    }
}
