//! Explicit local byte copies. A completed private spool is linked without overwrite.
use super::*;
use rustix::fs::{linkat, unlinkat};
use sha2::{Digest, Sha256};
use std::{io::Write, path::PathBuf};

pub const MAX_EXPORT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct Exported {
    pub workspace_id: Uuid,
    pub path: String,
    pub destination: PathBuf,
    pub size: u64,
    pub sha256: String,
}

fn destination_parent(path: &Path) -> Result<File, Error> {
    if !path.is_absolute()
        || path.as_os_str().len() > 4096
        || path.components().count() > 64
        || path
            .components()
            .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
        || path.file_name().is_none()
    {
        return Err(invalid(
            "export destination must be an absolute file path without parent traversal, within 4 KiB / 64 components",
        ));
    }
    let anchor = File::from(open("/", directory_flags(), Mode::empty()).map_err(invalid)?);
    let parent = path
        .parent()
        .and_then(Path::to_str)
        .ok_or_else(|| invalid("export destination parent is not UTF-8"))?;
    directory(&anchor, parent.trim_start_matches('/'))
}

fn copy_file(workspace: &Workspace, path: &str, destination: &Path) -> Result<Exported, Error> {
    let anchor = root(workspace)?;
    let mut source = target(&anchor, path)?;
    let before = source.metadata().map_err(invalid)?;
    if before.len() > MAX_EXPORT_BYTES {
        return Err(invalid("file exceeds the 64 MiB export limit"));
    }
    let folder = destination_parent(destination)?;
    let name = destination
        .file_name()
        .ok_or_else(|| invalid("choose an export filename"))?;
    let temporary = format!(".fleetpane-export-{}.tmp", Uuid::new_v4());
    let mut spool = File::from(
        openat(
            &folder,
            &temporary,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(invalid)?,
    );
    let result = (|| {
        let mut digest = Sha256::new();
        let mut size = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = source.read(&mut buffer).map_err(invalid)?;
            if read == 0 {
                break;
            }
            size += read as u64;
            if size > MAX_EXPORT_BYTES {
                return Err(invalid("file grew beyond the 64 MiB export limit"));
            }
            spool.write_all(&buffer[..read]).map_err(invalid)?;
            digest.update(&buffer[..read]);
        }
        if size != before.len()
            || fingerprint(&before) != fingerprint(&source.metadata().map_err(invalid)?)
            || !same_file(&source, &target(&root(workspace)?, path)?)?
        {
            return Err(invalid(
                "source changed during export; no destination published",
            ));
        }
        spool.sync_all().map_err(invalid)?;
        if !same_file(&folder, &destination_parent(destination)?)? {
            return Err(invalid(
                "destination folder changed during export; no destination published",
            ));
        }
        linkat(&folder, &temporary, &folder, name, AtFlags::empty()).map_err(|error| {
            invalid(format!(
                "export could not publish {} (existing files are never overwritten): {error}",
                destination.display()
            ))
        })?;
        Ok(Exported {
            workspace_id: workspace.id,
            path: path.into(),
            destination: destination.to_owned(),
            size,
            sha256: format!("{:x}", digest.finalize()),
        })
    })();
    let cleanup = unlinkat(&folder, &temporary, AtFlags::empty()).map_err(invalid);
    match (result, cleanup) {
        (Ok(exported), Ok(())) => {
            folder.sync_all().map_err(|error| {
                invalid(format!(
                    "export was published at {}; directory sync failed: {error}",
                    destination.display()
                ))
            })?;
            Ok(exported)
        }
        (Err(error), Ok(())) => Err(error),
        (result, Err(cleanup)) => Err(invalid(format!(
            "{}; spool cleanup failed for {temporary}: {cleanup}",
            match result {
                Ok(exported) =>
                    format!("export was published at {}", exported.destination.display()),
                Err(error) => error.to_string(),
            }
        ))),
    }
}

impl Workspace {
    /// Copy current bytes, not preview text. No overwrite or automatic retry.
    /// An admitted export waits instead of reporting a read timeout while its
    /// destination may still be published.
    pub async fn export_file(&self, path: &str, destination: PathBuf) -> Result<Exported, Error> {
        let path = relative(path, false)?;
        let workspace = self.clone();
        let permit = FILE_JOBS.try_acquire().map_err(|_| {
            invalid("two file operations are still running; wait before trying another")
        })?;
        let (sender, receiver) = oneshot::channel();
        std::thread::Builder::new()
            .name("fleetpane-export".into())
            .spawn(move || {
                let _permit = permit;
                let _closed = sender.send(copy_file(&workspace, &path, &destination));
            })
            .map_err(invalid)?;
        receiver.await.map_err(|_| invalid("export worker ended without a result; inspect the selected destination before retrying"))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[tokio::test]
    async fn export_preserves_raw_bytes_private_mode_and_never_overwrites() {
        let _guard = super::super::tests::SERIAL.lock().await;
        let (base, workspace) = super::super::tests::fixture().await;
        let bytes = b"\0\xffbinary\r\nexact  \n";
        let source = base.join("work/source.bin");
        std::fs::write(&source, bytes).unwrap();
        let destination = base.join("副本 %20#tag:2.bin");
        let result = workspace
            .export_file("source.bin", destination.clone())
            .await
            .unwrap();
        assert_eq!(result.size, bytes.len() as u64);
        assert_eq!(result.sha256, format!("{:x}", Sha256::digest(bytes)));
        assert_eq!(std::fs::read(&destination).unwrap(), bytes);
        assert_eq!(
            std::fs::metadata(&destination)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        std::fs::write(&source, b"changed after export").unwrap();
        assert!(
            workspace
                .export_file("source.bin", destination.clone())
                .await
                .is_err()
        );
        assert_eq!(std::fs::read(&destination).unwrap(), bytes);
        assert!(
            workspace
                .export_file("source.bin", source.clone())
                .await
                .is_err()
        );
        assert_eq!(std::fs::read(source).unwrap(), b"changed after export");
        assert!(!std::fs::read_dir(&base).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".fleetpane-export-")
        }));
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test]
    async fn export_refuses_links_special_files_limits_and_replaced_workspace() {
        let _guard = super::super::tests::SERIAL.lock().await;
        let (base, workspace) = super::super::tests::fixture().await;
        let source = base.join("work/source");
        std::fs::write(&source, b"source").unwrap();
        symlink("source", base.join("work/link")).unwrap();
        rustix::fs::mknodat(
            rustix::fs::CWD,
            base.join("work/pipe"),
            FileType::Fifo,
            Mode::RUSR | Mode::WUSR,
            0,
        )
        .unwrap();
        let large = File::create(base.join("work/large")).unwrap();
        large.set_len(MAX_EXPORT_BYTES + 1).unwrap();
        let destination = base.join("output");
        for path in [
            "link",
            "pipe",
            "large",
            "missing",
            "../source",
            "/etc/passwd",
        ] {
            assert!(
                workspace
                    .export_file(path, destination.clone())
                    .await
                    .is_err(),
                "{path}"
            );
            assert!(!destination.exists());
        }
        symlink(base.join("work"), base.join("alias")).unwrap();
        for path in [
            base.join("alias/target"),
            base.join("missing/target"),
            PathBuf::from("relative"),
            base.join("../target"),
        ] {
            assert!(workspace.export_file("source", path).await.is_err());
        }
        symlink(&source, &destination).unwrap();
        assert!(
            workspace
                .export_file("source", destination.clone())
                .await
                .is_err()
        );
        assert_eq!(std::fs::read(&source).unwrap(), b"source");
        std::fs::rename(base.join("work"), base.join("old")).unwrap();
        std::fs::create_dir(base.join("work")).unwrap();
        std::fs::write(base.join("work/source"), b"wrong replacement").unwrap();
        assert!(
            workspace
                .export_file("source", base.join("new-copy"))
                .await
                .is_err()
        );
        assert!(!base.join("new-copy").exists());
        std::fs::remove_dir_all(base).unwrap();
    }
}
