//! Versioned private scratch. Writers share a process lock; collection requires
//! exclusive ownership. Legacy files have no such contract and remain untouched.
use super::invalid;
use crate::{client::Error, storage::Store};
use serde::Serialize;
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const DIRECTORY: &str = "paseo-scratch-v1";
const MAX_ENTRIES: usize = 40_000;

#[derive(Debug, Default, Serialize)]
pub struct ScratchReport {
    pub files: usize,
    pub bytes: u64,
    pub removed: bool,
    pub legacy_files_retained: usize,
}

fn lock(root: &Path, exclusive: bool) -> Result<File, Error> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK)
                .bits()
                .try_into()
                .map_err(invalid)?,
        )
        .open(root.join("paseo-scratch-v1.lock"))
        .map_err(invalid)?;
    let metadata = file.metadata().map_err(invalid)?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(invalid("scratch lock must be a private regular file"));
    }
    let operation = if exclusive {
        rustix::fs::FlockOperation::NonBlockingLockExclusive
    } else {
        rustix::fs::FlockOperation::NonBlockingLockShared
    };
    match rustix::fs::flock(&file, operation) {
        Ok(()) => Ok(file),
        Err(rustix::io::Errno::WOULDBLOCK) => Err(invalid(
            "history copy, fork export or scratch collection is active; wait for it to finish",
        )),
        Err(error) => Err(invalid(error)),
    }
}

/// Owned inside the blocking copy/export, including publication and cleanup.
pub(super) struct Writer {
    _lock: File,
    directory: PathBuf,
}
impl Writer {
    pub fn open(root: &Path) -> Result<Self, Error> {
        let guard = lock(root, false)?;
        let directory = root.join(DIRECTORY);
        fleetpane_runtime::unix::prepare_directory(&directory).map_err(invalid)?;
        Ok(Self {
            _lock: guard,
            directory,
        })
    }
    pub fn path(&self) -> PathBuf {
        self.directory.join(format!("{}.tmp", Uuid::new_v4()))
    }
}

struct Entry {
    path: PathBuf,
    metadata: std::fs::Metadata,
}
fn entries(root: &Path) -> Result<Vec<Entry>, Error> {
    let directory = root.join(DIRECTORY);
    let metadata = match std::fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(invalid(error)),
    };
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(invalid("scratch must be a private non-symlink directory"));
    }
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(directory).map_err(invalid)? {
        if entries.len() == MAX_ENTRIES {
            return Err(invalid(
                "scratch exceeds 40,000 entries; nothing was collected",
            ));
        }
        let entry = entry.map_err(invalid)?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| invalid("unrecognized scratch filename; nothing was collected"))?;
        let stem = name
            .strip_suffix(".tmp")
            .or_else(|| name.strip_suffix(".tmp-journal"));
        if !stem.is_some_and(|stem| Uuid::parse_str(stem).is_ok_and(|id| id.to_string() == stem)) {
            return Err(invalid(format!(
                "unrecognized scratch file {name}; nothing was collected"
            )));
        }
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).map_err(invalid)?;
        if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
            return Err(invalid(format!(
                "scratch file {name} must be private and regular; nothing was collected"
            )));
        }
        entries.push(Entry { path, metadata });
    }
    Ok(entries)
}

fn collect(root: &Path, remove: bool) -> Result<ScratchReport, Error> {
    let _guard = lock(root, true)?;
    // Preflight the entire bounded collection before unlinking the first file.
    let entries = entries(root)?;
    let report = ScratchReport {
        files: entries.len(),
        bytes: entries.iter().map(|entry| entry.metadata.len()).sum(),
        removed: remove,
        legacy_files_retained: legacy_files(root)?,
    };
    if remove {
        let mut removed = 0;
        for entry in entries {
            let action = || -> Result<(), Error> {
                let current = std::fs::symlink_metadata(&entry.path).map_err(invalid)?;
                if !current.is_file()
                    || current.dev() != entry.metadata.dev()
                    || current.ino() != entry.metadata.ino()
                    || current.permissions().mode() & 0o077 != 0
                {
                    return Err(invalid("scratch entry changed during collection"));
                }
                std::fs::remove_file(&entry.path).map_err(invalid)
            };
            action().map_err(|error| {
                invalid(format!("collection stopped after {removed} files: {error}"))
            })?;
            removed += 1;
        }
        if removed > 0 {
            File::open(root.join(DIRECTORY))
                .and_then(|file| file.sync_all())
                .map_err(|error| {
                    invalid(format!(
                        "removed {removed} scratch files but directory sync failed: {error}"
                    ))
                })?;
        }
    }
    Ok(report)
}

// Older binaries do not participate in the scratch lock. Only report these
// files; an available new lock proves nothing about an older worker's lifetime.
fn legacy_files(root: &Path) -> Result<usize, Error> {
    let mut files = 0;
    for directory in ["paseo-history", "paseo-forks"] {
        let directory = root.join(directory);
        let metadata = match std::fs::symlink_metadata(&directory) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(invalid(error)),
        };
        if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
            return Err(invalid(
                "legacy history/fork storage must be a private non-symlink directory",
            ));
        }
        for (index, entry) in std::fs::read_dir(directory).map_err(invalid)?.enumerate() {
            if index == MAX_ENTRIES {
                return Err(invalid(
                    "legacy history/fork storage exceeds 40,000 entries; nothing was collected",
                ));
            }
            let entry = entry.map_err(invalid)?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".tmp") || name.ends_with(".tmp-journal") {
                files += 1;
            }
        }
    }
    Ok(files)
}

impl Store {
    /// Explicit observation or collection; opening a Store never scans/deletes.
    /// All current copies/exports must finish before either operation is allowed.
    pub async fn paseo_scratch(&self, remove: bool) -> Result<ScratchReport, Error> {
        let root = self.root().to_owned();
        tokio::task::spawn_blocking(move || collect(&root, remove))
            .await
            .map_err(invalid)?
    }
}

#[cfg(test)]
mod tests;
