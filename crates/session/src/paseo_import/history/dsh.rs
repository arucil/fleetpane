//! Released DSH v3/v4 keep the same physical row framing. Select the highest
//! canonical generation, as JsonlSessionPersistence does; never fall back to an
//! older snapshot when the authoritative generation is unsupported or broken.
use super::{Error, invalid};
use std::path::{Path, PathBuf};

pub(super) fn generation(name: &str) -> Option<(u64, bool)> {
    let compressed = name.ends_with(".zstd");
    let base = if compressed {
        name.strip_suffix(".zstd")?
    } else {
        name
    };
    if base == "session.jsonl" {
        return Some((0, compressed));
    }
    let digits = base.strip_prefix("session.v")?.strip_suffix(".jsonl")?;
    if digits.starts_with('0') || digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let version = digits.parse::<u64>().ok()?;
    (version <= 9_007_199_254_740_991).then_some((version, compressed))
}

pub(super) fn source(directory: &Path, root: &Path) -> Result<(PathBuf, u64), Error> {
    let directory = std::fs::canonicalize(directory).map_err(invalid)?;
    if !directory.starts_with(root) {
        return Err(invalid("DSH history path escapes configured provider root"));
    }
    let mut selected: Option<(u64, PathBuf)> = None;
    let mut encoding = None;
    for (count, entry) in std::fs::read_dir(&directory).map_err(invalid)?.enumerate() {
        if count >= 1024 {
            return Err(invalid("DSH session directory exceeds 1024 entries"));
        }
        let entry = entry.map_err(invalid)?;
        let name = entry.file_name();
        let Some((version, compressed)) = name.to_str().and_then(generation) else {
            continue;
        };
        if encoding.is_some_and(|previous| previous != compressed) {
            return Err(invalid(
                "DSH session has mixed plain/Zstandard encodings; select an unambiguous source export",
            ));
        }
        encoding = Some(compressed);
        if selected
            .as_ref()
            .is_none_or(|(current, _)| version > *current)
        {
            selected = Some((version, entry.path()));
        }
    }
    let (version, path) =
        selected.ok_or_else(|| invalid("DeepSeek Harness history file is missing"))?;
    if !matches!(version, 3 | 4) {
        return Err(invalid(format!(
            "unsupported DeepSeek Harness history version {version}; supported: 3, 4; older generations were not copied"
        )));
    }
    let path = std::fs::canonicalize(path).map_err(invalid)?;
    if !path.starts_with(root) {
        return Err(invalid("DSH history path escapes configured provider root"));
    }
    Ok((path, version))
}

#[test]
fn canonical_generation_names() {
    for (name, expected) in [
        ("session.jsonl", Some((0, false))),
        ("session.v3.jsonl.zstd", Some((3, true))),
        ("session.v4.jsonl", Some((4, false))),
        ("session.v12.jsonl", Some((12, false))),
        ("session.v0.jsonl", None),
        ("session.v04.jsonl", None),
        ("session.v4.jsonl.tmp", None),
        ("session.v+4.jsonl", None),
        ("session.v9007199254740992.jsonl", None),
    ] {
        assert_eq!(generation(name), expected, "{name}");
    }
}
