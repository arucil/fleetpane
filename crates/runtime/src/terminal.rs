//! Host-owned PTYs. Raw bytes and byte cursors belong to the engine, not a renderer.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

pub const READ_METHOD: &str = "fleetpane/terminals";
pub const MAX_OUTPUT: usize = 64 * 1024;
pub const MAX_INPUT: usize = 4096;
pub const MAX_TERMINALS: usize = 16;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Mutation {
    Create {
        workspace_id: Uuid,
        cwd: PathBuf,
        device: u64,
        inode: u64,
        label: String,
        rows: u16,
        cols: u16,
    },
    Input {
        workspace_id: Uuid,
        id: Uuid,
        expected_input: u64,
        bytes: Vec<u8>,
    },
    Resize {
        workspace_id: Uuid,
        id: Uuid,
        rows: u16,
        cols: u16,
    },
    Rename {
        workspace_id: Uuid,
        id: Uuid,
        label: String,
    },
    Close {
        workspace_id: Uuid,
        id: Uuid,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Read {
    List {
        workspace_id: Uuid,
    },
    Output {
        workspace_id: Uuid,
        id: Uuid,
        after: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Info {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub cwd: PathBuf,
    pub label: String,
    pub pid: u32,
    pub rows: u16,
    pub cols: u16,
    pub next_input: u64,
    pub input_fault: Option<String>,
    pub exit: Option<Exit>,
    pub output_eof: bool,
    pub output_fault: Option<String>,
    pub closed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Exit {
    pub code: Option<i32>,
    pub signal: Option<i32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Output {
    pub terminal: Info,
    pub start: u64,
    pub end: u64,
    pub dropped: u64,
    pub bytes: Vec<u8>,
}

impl Mutation {
    pub fn method(&self) -> &'static str {
        match self {
            Self::Create { .. } => "fleetpane/terminal/create",
            Self::Input { .. } => "fleetpane/terminal/input",
            Self::Resize { .. } => "fleetpane/terminal/resize",
            Self::Rename { .. } => "fleetpane/terminal/rename",
            Self::Close { .. } => "fleetpane/terminal/close",
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Create {
                cwd,
                label,
                rows,
                cols,
                ..
            } => {
                if !cwd.is_absolute() || cwd.as_os_str().len() > 4096 {
                    return Err("terminal cwd must be an absolute path up to 4 KiB".into());
                }
                validate_label(label)?;
                validate_size(*rows, *cols)
            }
            Self::Input { bytes, .. } if bytes.is_empty() || bytes.len() > MAX_INPUT => Err(
                "input must contain 1..4096 bytes; Enter and control keys are explicit bytes"
                    .into(),
            ),
            Self::Resize { rows, cols, .. } => validate_size(*rows, *cols),
            Self::Rename { label, .. } => validate_label(label),
            _ => Ok(()),
        }
    }
}

pub fn is_mutation(method: &str) -> bool {
    matches!(
        method,
        "fleetpane/terminal/create"
            | "fleetpane/terminal/input"
            | "fleetpane/terminal/resize"
            | "fleetpane/terminal/rename"
            | "fleetpane/terminal/close"
    )
}

fn validate_label(label: &str) -> Result<(), String> {
    if label.trim().is_empty() || label.len() > 128 || label.chars().any(char::is_control) {
        return Err("terminal label needs 1..128 UTF-8 bytes without control characters".into());
    }
    Ok(())
}
fn validate_size(rows: u16, cols: u16) -> Result<(), String> {
    if rows == 0 || cols == 0 || rows > 1000 || cols > 1000 {
        return Err("terminal size must be 1..1000 rows and columns".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn input_labels_size_and_drain_are_bounded() {
        let workspace_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let input = |bytes| Mutation::Input {
            workspace_id,
            id,
            expected_input: 0,
            bytes,
        };
        assert!(input(vec![]).validate().is_err());
        assert!(input(vec![0; MAX_INPUT + 1]).validate().is_err());
        assert!(
            input("中文\n$() `literal` ".as_bytes().to_vec())
                .validate()
                .is_ok()
        );
        for size in [(0, 80), (24, 0), (1001, 80), (24, 1001)] {
            assert!(validate_size(size.0, size.1).is_err());
        }
        for name in ["", "   ", "bad\nlabel"] {
            assert!(validate_label(name).is_err());
        }
        let admission = crate::lifecycle::Admission {
            generation: 1,
            mode: crate::lifecycle::Mode::Draining,
        };
        assert!(!admission.permits("fleetpane/terminal/create"));
        for method in ["input", "resize", "rename", "close"] {
            assert!(admission.permits(&format!("fleetpane/terminal/{method}")));
        }
        assert!(!admission.permits("fleetpane/terminal/unknown"));
        assert!(!is_mutation("fleetpane/terminal/unknown"));
    }
}
#[cfg(unix)]
mod host;
#[cfg(unix)]
pub(crate) use host::Manager;
