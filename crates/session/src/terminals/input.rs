//! Toolkit-independent terminal keys and bounded, receipt-ordered input.
use std::collections::VecDeque;

pub const MAX_QUEUED: usize = 16 * 1024;
const PACKET: usize = fleetpane_runtime::terminal::MAX_INPUT;

#[derive(Default)]
pub struct Queue {
    waiting: VecDeque<u8>,
    flight: Option<Vec<u8>>,
    paused: bool,
}

impl Queue {
    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }
    pub fn in_flight(&self) -> bool {
        self.flight.is_some()
    }
    pub fn paused(&self) -> bool {
        self.paused
    }
    pub fn empty(&self) -> bool {
        self.waiting.is_empty() && self.flight.is_none()
    }
    pub fn push(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        if self.paused {
            return Err("Keyboard input paused; new input was not queued".into());
        }
        if bytes.len() > MAX_QUEUED - self.waiting.len() - self.flight.as_ref().map_or(0, Vec::len)
        {
            self.paused = true;
            return Err("Keyboard buffer is full; this input was not queued. Review queued keys before resuming".into());
        }
        self.waiting.extend(bytes);
        Ok(())
    }
    pub fn start(&mut self) -> Option<Vec<u8>> {
        if self.paused || self.flight.is_some() || self.waiting.is_empty() {
            return None;
        }
        let bytes: Vec<_> = self
            .waiting
            .drain(..self.waiting.len().min(PACKET))
            .collect();
        self.flight = Some(bytes.clone());
        Some(bytes)
    }
    pub fn acknowledge(&mut self) {
        self.flight
            .take()
            .expect("keyboard acknowledgement must match an in-flight packet");
    }
    pub fn pause(&mut self) {
        self.paused = true;
    }
    pub fn resume(&mut self) -> Result<(), String> {
        if self.flight.is_some() {
            return Err(
                "Resolve the keyboard operation before resuming; no input was resent".into(),
            );
        }
        self.paused = false;
        Ok(())
    }
    pub fn bytes(&self) -> (Option<&[u8]>, Vec<u8>) {
        (
            self.flight.as_deref(),
            self.waiting.iter().copied().collect(),
        )
    }
    pub fn discard(&mut self, retain_flight: bool) {
        self.waiting.clear();
        if !retain_flight {
            self.flight = None;
            self.paused = false;
        }
    }
}

#[derive(Default)]
pub struct Key<'a> {
    pub name: &'a str,
    pub text: Option<&'a str>,
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub platform: bool,
}

pub fn encode(key: Key<'_>, application_cursor: bool) -> Option<Vec<u8>> {
    if key.platform {
        return None;
    }
    let modifier = 1 + u8::from(key.shift) + 2 * u8::from(key.alt) + 4 * u8::from(key.control);
    let cursor = match key.name {
        "up" => Some('A'),
        "down" => Some('B'),
        "right" => Some('C'),
        "left" => Some('D'),
        "home" => Some('H'),
        "end" => Some('F'),
        _ => None,
    };
    if let Some(code) = cursor {
        return Some(
            if modifier != 1 {
                format!("\x1b[1;{modifier}{code}")
            } else if application_cursor {
                format!("\x1bO{code}")
            } else {
                format!("\x1b[{code}")
            }
            .into_bytes(),
        );
    }
    let tilde = match key.name {
        "insert" => Some(2),
        "delete" => Some(3),
        "pageup" => Some(5),
        "pagedown" => Some(6),
        "f5" => Some(15),
        "f6" => Some(17),
        "f7" => Some(18),
        "f8" => Some(19),
        "f9" => Some(20),
        "f10" => Some(21),
        "f11" => Some(23),
        "f12" => Some(24),
        _ => None,
    };
    if let Some(code) = tilde {
        return Some(
            if modifier == 1 {
                format!("\x1b[{code}~")
            } else {
                format!("\x1b[{code};{modifier}~")
            }
            .into_bytes(),
        );
    }
    let function = match key.name {
        "f1" => Some('P'),
        "f2" => Some('Q'),
        "f3" => Some('R'),
        "f4" => Some('S'),
        _ => None,
    };
    if let Some(code) = function {
        return Some(
            if modifier == 1 {
                format!("\x1bO{code}")
            } else {
                format!("\x1b[1;{modifier}{code}")
            }
            .into_bytes(),
        );
    }
    if key.name == "enter" && modifier != 1 {
        return Some(format!("\x1b[13;{modifier}u").into_bytes());
    }
    let mut bytes = match key.name {
        "enter" => vec![13],
        "escape" => vec![27],
        "backspace" => vec![if key.control { 8 } else { 127 }],
        "tab" if key.shift => b"\x1b[Z".to_vec(),
        "tab" => vec![9],
        _ if key.control => {
            let name = key.name.as_bytes();
            if name.len() != 1 {
                return None;
            }
            let c = name[0].to_ascii_uppercase();
            match c {
                b'@'..=b'_' => vec![c & 0x1f],
                b' ' | b'2' => vec![0],
                b'?' | b'8' => vec![127],
                _ => return None,
            }
        }
        _ => key
            .text
            .filter(|text| !text.is_empty())?
            .as_bytes()
            .to_vec(),
    };
    if key.alt {
        bytes.insert(0, 27);
    }
    Some(bytes)
}

pub fn paste(text: &str, bracketed: bool) -> Result<Vec<u8>, String> {
    if text.contains('\x1b') {
        return Err("Clipboard text contains terminal escape bytes; paste was not queued".into());
    }
    let framing = if bracketed && !text.is_empty() { 12 } else { 0 };
    if text.len() > MAX_QUEUED - framing {
        return Err(
            "Terminal paste exceeds the 16 KiB keyboard buffer; paste was not queued".into(),
        );
    }
    Ok(if bracketed && !text.is_empty() {
        format!("\x1b[200~{text}\x1b[201~").into_bytes()
    } else {
        text.as_bytes().to_vec()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modified_enter_keeps_each_modifier_and_never_aliases_plain_return() {
        for bits in 0..8 {
            let key = Key {
                name: "enter",
                shift: bits & 1 != 0,
                alt: bits & 2 != 0,
                control: bits & 4 != 0,
                ..Key::default()
            };
            let expected = if bits == 0 {
                vec![13]
            } else {
                format!("\x1b[13;{}u", bits + 1).into_bytes()
            };
            for application_cursor in [false, true] {
                assert_eq!(
                    encode(Key { ..key }, application_cursor).unwrap(),
                    expected,
                    "modifier bits {bits}, application cursor {application_cursor}"
                );
            }
        }
        assert!(
            encode(
                Key {
                    name: "enter",
                    platform: true,
                    shift: true,
                    ..Key::default()
                },
                false
            )
            .is_none()
        );
    }
    #[test]
    fn ordered_packets_pause_without_replaying_uncertain_input() {
        let mut q = Queue::default();
        let original = "中文\n".repeat(1500).into_bytes();
        q.push(original.clone()).unwrap();
        let first = q.start().unwrap();
        assert_eq!(first.len(), PACKET);
        q.push(b"tail".to_vec()).unwrap();
        assert!(q.start().is_none());
        q.pause();
        assert!(q.push(b"rejected".to_vec()).is_err());
        assert!(q.resume().is_err());
        q.acknowledge();
        assert!(q.start().is_none());
        q.resume().unwrap();
        let mut received = first;
        while let Some(bytes) = q.start() {
            received.extend(bytes);
            q.acknowledge();
        }
        assert_eq!(received, [original, b"tail".to_vec()].concat());
        assert!(q.empty());
        q.push(vec![1; MAX_QUEUED]).unwrap();
        assert!(q.push(vec![2]).is_err());
        assert_eq!(q.waiting(), MAX_QUEUED);
        assert!(q.paused());
        q.discard(false);
        assert!(q.empty());
        q.push(vec![3]).unwrap();
        q.start().unwrap();
        q.push(vec![4]).unwrap();
        q.pause();
        q.discard(true);
        assert!(q.in_flight());
        assert_eq!(q.waiting(), 0);
        assert!(q.start().is_none());
    }
    #[test]
    fn terminal_modes_modifiers_unicode_and_paste_are_explicit() {
        let key = |name| Key {
            name,
            ..Key::default()
        };
        assert_eq!(encode(key("up"), false).unwrap(), b"\x1b[A");
        assert_eq!(encode(key("up"), true).unwrap(), b"\x1bOA");
        assert_eq!(
            encode(
                Key {
                    name: "left",
                    control: true,
                    shift: true,
                    ..Key::default()
                },
                true
            )
            .unwrap(),
            b"\x1b[1;6D"
        );
        assert_eq!(
            encode(
                Key {
                    name: "c",
                    control: true,
                    ..Key::default()
                },
                false
            )
            .unwrap(),
            [3]
        );
        assert_eq!(
            encode(
                Key {
                    name: "x",
                    text: Some("中"),
                    ..Key::default()
                },
                false
            )
            .unwrap(),
            "中".as_bytes()
        );
        assert_eq!(
            encode(
                Key {
                    name: "x",
                    text: Some("x"),
                    alt: true,
                    ..Key::default()
                },
                false
            )
            .unwrap(),
            b"\x1bx"
        );
        assert!(
            encode(
                Key {
                    platform: true,
                    ..key("q")
                },
                false
            )
            .is_none()
        );
        assert_eq!(
            paste("中文\n  code", true).unwrap(),
            "\x1b[200~中文\n  code\x1b[201~".as_bytes()
        );
        assert_eq!(paste("a\nb", false).unwrap(), b"a\nb");
        assert!(paste("\x1b[201~", true).is_err());
        assert!(paste(&"x".repeat(MAX_QUEUED + 1), false).is_err());
    }
}
