//! Demand-loaded local file observations, independent of Host/provider lifetime.
mod export;
use crate::{client::Error, workspaces::Workspace};
use base64::{Engine, engine::general_purpose::STANDARD};
pub use export::{Exported, MAX_EXPORT_BYTES};
use fleetpane_runtime::attachments::{Image, MAX_IMAGE_BYTES};
use rustix::fs::{AtFlags, Dir, FileType, Mode, OFlags, open, openat, statat};
use serde::Serialize;
use std::{
    fs::File,
    io::Read,
    num::NonZeroUsize,
    os::unix::fs::MetadataExt,
    path::{Component, Path},
    time::Duration,
};
use tokio::sync::{Semaphore, oneshot};
use uuid::Uuid;

pub const MAX_ENTRIES: usize = 500;
pub const MAX_TEXT_BYTES: usize = 256 * 1024;
const MAX_LIST_BYTES: usize = 256 * 1024;
static FILE_JOBS: Semaphore = Semaphore::const_new(2);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Directory,
    File,
    Symlink,
    Other,
}

#[derive(Debug, Serialize)]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub kind: Kind,
    pub size: u64,
}

#[derive(Debug, Serialize)]
pub struct Listing {
    pub workspace_id: Uuid,
    pub path: String,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Content {
    Text { text: String },
    Image { image: Image, data_base64: String },
    Unavailable { reason: String },
    Binary,
    TooLarge { limit: usize },
}

#[derive(Debug, Serialize)]
pub struct Readback {
    pub workspace_id: Uuid,
    pub path: String,
    pub size: u64,
    pub content: Content,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum Opened {
    Directory(Listing),
    File(Readback),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub path: String,
    /// One-based source line, supplied by an explicit Markdown link.
    pub line: Option<NonZeroUsize>,
}

impl Target {
    /// Resolve a literal provider path against the command's directory, not the
    /// conversation directory. Workspace identity and confinement are checked on open.
    pub fn in_directory(directory: &str, path: &str) -> Result<Self, Error> {
        if path.is_empty() || path.len() > 4096 || path.contains('\0') {
            return Err(invalid("choose a nonempty file path within 4 KiB"));
        }
        let path = Path::new(path);
        let resolved = if path.is_absolute() {
            path.to_owned()
        } else {
            if !Path::new(directory).is_absolute() || directory.contains('\0') {
                return Err(invalid("command directory must be an absolute local path"));
            }
            Path::new(directory).join(path)
        };
        let path = resolved
            .to_str()
            .ok_or_else(|| invalid("file path is not UTF-8"))?;
        if path.len() > 4096 {
            return Err(invalid("resolved file path exceeds 4 KiB"));
        }
        Ok(Self {
            path: path.into(),
            line: None,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Link {
    Web(String),
    Local(Target),
}

impl Link {
    /// Decode explicit links only. Ordinary inline code is never guessed to be a path.
    pub fn parse(href: &str) -> Result<Self, Error> {
        if href.is_empty() || href.len() > 3 * 4096 {
            return Err(invalid("file link is empty or exceeds 12 KiB"));
        }
        let scheme = href
            .split_once(':')
            .map(|(scheme, _)| scheme.to_ascii_lowercase());
        if matches!(scheme.as_deref(), Some("https" | "http")) {
            return Ok(Self::Web(href.into()));
        }
        let local = if scheme.as_deref() == Some("file") {
            let path = &href[5..];
            if let Some(authority) = path.strip_prefix("//") {
                if authority.starts_with('/') {
                    authority
                } else if let Some(path) = authority
                    .strip_prefix("localhost")
                    .filter(|path| path.starts_with('/'))
                {
                    path
                } else {
                    return Err(invalid("only local file URLs are supported"));
                }
            } else if path.starts_with('/') {
                path
            } else {
                return Err(invalid("file URLs require an absolute local path"));
            }
        } else {
            href
        };
        if local.starts_with("//") {
            return Err(invalid("network file links are not supported"));
        }
        let (path, fragment) = local
            .split_once('#')
            .map_or((local, None), |(path, fragment)| (path, Some(fragment)));
        let (path, line) = if let Some(fragment) = fragment {
            let line = fragment
                .strip_prefix('L')
                .ok_or_else(|| invalid("file anchors use #L1 or #L1-L2"))?;
            let (first, last) = line
                .split_once("-L")
                .map_or((line, None), |(first, last)| (first, Some(last)));
            let first = source_line(first)?;
            if let Some(last) = last
                && source_line(last)? < first
            {
                return Err(invalid("file line range is reversed"));
            }
            (path, Some(first))
        } else if let Some((base, suffix)) = path.rsplit_once(':') {
            if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) {
                (base, Some(source_line(suffix)?))
            } else {
                (path, None)
            }
        } else {
            (path, None)
        };
        // Reject schemes before decoding; an encoded colon is a literal filename character.
        if let Some((scheme, _)) = path.split_once(':')
            && !scheme.contains('/')
            && !scheme.contains('\\')
        {
            return Err(invalid("unsupported link scheme"));
        }
        let path = percent_encoding::percent_decode_str(path)
            .decode_utf8()
            .map_err(|_| invalid("file link is not UTF-8"))?
            .into_owned();
        if path.is_empty() || path.len() > 4096 || path.contains('\0') {
            return Err(invalid("choose a nonempty file path within 4 KiB"));
        }
        Ok(Self::Local(Target { path, line }))
    }
}

fn source_line(value: &str) -> Result<NonZeroUsize, Error> {
    value
        .parse::<NonZeroUsize>()
        .map_err(|_| invalid("source lines must be positive integers"))
}

fn invalid(error: impl std::fmt::Display) -> Error {
    Error::Input(format!("Workspace files: {error}"))
}

fn relative(path: &str, allow_root: bool) -> Result<String, Error> {
    if allow_root && matches!(path, "" | ".") {
        return Ok(String::new());
    }
    let components: Vec<_> = Path::new(path).components().collect();
    if path.is_empty()
        || path.len() > 4096
        || path.contains('\0')
        || components.len() > 64
        || components
            .iter()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(invalid(
            "choose a relative path within the saved workspace (no parent traversal, up to 4 KiB / 64 components)",
        ));
    }
    Ok(components
        .iter()
        .map(|part| part.as_os_str())
        .collect::<std::path::PathBuf>()
        .to_str()
        .ok_or_else(|| invalid("path is not UTF-8"))?
        .to_owned())
}

// Detached OS workers keep their permits even after a timeout or closed window.
// Dropping a per-command Tokio runtime must not wait on blocked filesystem I/O.
async fn job<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, Error> + Send + 'static,
) -> Result<T, Error> {
    let permit = FILE_JOBS.try_acquire().map_err(|_| {
        invalid("two file operations are still running; wait before trying another")
    })?;
    let (sender, receiver) = oneshot::channel();
    std::thread::Builder::new()
        .name("fleetpane-files".into())
        .spawn(move || {
            let _permit = permit;
            let _closed = sender.send(work());
        })
        .map_err(invalid)?;
    tokio::time::timeout(Duration::from_secs(5), receiver).await
        .map_err(|_|invalid("file operation exceeded 5 seconds; any blocked worker remains bounded until it exits"))?
        .map_err(|_|invalid("file worker ended without a result"))?
}

fn directory_flags() -> OFlags {
    OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NOFOLLOW
}

fn root(workspace: &Workspace) -> Result<File, Error> {
    let saved = &workspace.registration.directory;
    let file = File::from(open(&saved.path, directory_flags(), Mode::empty()).map_err(invalid)?);
    let metadata = file.metadata().map_err(invalid)?;
    if metadata.dev() != saved.device || metadata.ino() != saved.inode {
        return Err(invalid(
            "saved directory was replaced; register the new workspace explicitly",
        ));
    }
    Ok(file)
}

fn directory(root: &File, path: &str) -> Result<File, Error> {
    let mut current = root.try_clone().map_err(invalid)?;
    for part in Path::new(path).components() {
        current = File::from(
            openat(&current, part.as_os_str(), directory_flags(), Mode::empty())
                .map_err(invalid)?,
        );
    }
    Ok(current)
}

fn target(root: &File, path: &str) -> Result<File, Error> {
    let path = Path::new(path);
    let parent = directory(
        root,
        path.parent()
            .and_then(Path::to_str)
            .ok_or_else(|| invalid("invalid parent path"))?,
    )?;
    let name = path.file_name().ok_or_else(|| invalid("choose a file"))?;
    let entry = statat(&parent, name, AtFlags::SYMLINK_NOFOLLOW).map_err(invalid)?;
    if FileType::from_raw_mode(entry.st_mode) != FileType::RegularFile {
        return Err(invalid(
            "only regular files can be read; symlinks and special files are not followed",
        ));
    }
    let file = File::from(
        openat(
            &parent,
            name,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(invalid)?,
    );
    if !file.metadata().map_err(invalid)?.is_file() {
        return Err(invalid("file type changed while opening; refresh"));
    }
    Ok(file)
}

fn same_file(a: &File, b: &File) -> Result<bool, Error> {
    let a = a.metadata().map_err(invalid)?;
    let b = b.metadata().map_err(invalid)?;
    Ok(a.dev() == b.dev() && a.ino() == b.ino())
}

fn fingerprint(metadata: &std::fs::Metadata) -> (u64, i64, i64, i64, i64) {
    (
        metadata.len(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec(),
    )
}

fn listing(workspace: &Workspace, path: &str) -> Result<Listing, Error> {
    let anchor = root(workspace)?;
    let folder = directory(&anchor, path)?;
    let mut entries = Vec::new();
    let mut bytes = 0;
    for entry in Dir::read_from(&folder).map_err(invalid)? {
        let entry = entry.map_err(invalid)?;
        let name = entry.file_name().to_str().map_err(|_| {
            invalid(
                "directory contains a non-UTF-8 filename; use a filesystem tool for this directory",
            )
        })?;
        if matches!(name, "." | "..") {
            continue;
        }
        let stat =
            statat(&folder, entry.file_name(), AtFlags::SYMLINK_NOFOLLOW).map_err(invalid)?;
        let kind = match FileType::from_raw_mode(stat.st_mode) {
            FileType::Directory => Kind::Directory,
            FileType::RegularFile => Kind::File,
            FileType::Symlink => Kind::Symlink,
            _ => Kind::Other,
        };
        let child = if path.is_empty() {
            name.to_owned()
        } else {
            format!("{path}/{name}")
        };
        bytes += name.len() + child.len();
        if entries.len() >= MAX_ENTRIES || bytes > MAX_LIST_BYTES {
            return Err(invalid(
                "directory exceeds 500 entries / 256 KiB of path data; choose a subdirectory",
            ));
        }
        entries.push(Entry {
            name: name.into(),
            path: child,
            kind,
            size: u64::try_from(stat.st_size).map_err(invalid)?,
        });
    }
    if !same_file(&folder, &directory(&root(workspace)?, path)?)? {
        return Err(invalid("directory changed while listing; refresh"));
    }
    entries.sort_by(|a, b| {
        (a.kind != Kind::Directory, &a.name).cmp(&(b.kind != Kind::Directory, &b.name))
    });
    Ok(Listing {
        workspace_id: workspace.id,
        path: path.into(),
        entries,
    })
}

fn readback(workspace: &Workspace, path: &str) -> Result<Readback, Error> {
    let anchor = root(workspace)?;
    let mut file = target(&anchor, path)?;
    let before = file.metadata().map_err(invalid)?;
    // Inspect a bounded header first; extensions alone cannot identify image data.
    let mut bytes = Vec::new();
    (&mut file)
        .take(32)
        .read_to_end(&mut bytes)
        .map_err(invalid)?;
    let format = image::guess_format(&bytes).ok();
    let extension = Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase);
    let expected_image = extension.as_deref().is_some_and(|s| {
        matches!(
            s,
            "png"
                | "jpg"
                | "jpeg"
                | "webp"
                | "gif"
                | "bmp"
                | "tif"
                | "tiff"
                | "avif"
                | "svg"
                | "ico"
                | "heic"
        )
    });
    let is_image = format.is_some() || expected_image;
    let limit = if is_image {
        usize::try_from(MAX_IMAGE_BYTES).map_err(invalid)?
    } else {
        MAX_TEXT_BYTES
    };
    let content = if before.len() > u64::try_from(limit).map_err(invalid)? {
        Content::TooLarge { limit }
    } else {
        (&mut file)
            .take(u64::try_from((limit + 1).saturating_sub(bytes.len())).map_err(invalid)?)
            .read_to_end(&mut bytes)
            .map_err(invalid)?;
        if bytes.len() > limit {
            return Err(invalid(
                "file grew beyond its preview limit while reading; refresh",
            ));
        }
        if is_image {
            let name = Path::new(path)
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| invalid("invalid image name"))?;
            match Image::inspect(&bytes, name.into()) {
                Ok(image) => Content::Image {
                    image,
                    data_base64: STANDARD.encode(bytes),
                },
                Err(reason) => Content::Unavailable { reason },
            }
        } else if bytes.contains(&0) {
            Content::Binary
        } else {
            match String::from_utf8(bytes) {
                Ok(text) => Content::Text { text },
                Err(_) => Content::Binary,
            }
        }
    };
    if fingerprint(&before) != fingerprint(&file.metadata().map_err(invalid)?)
        || !same_file(&file, &target(&root(workspace)?, path)?)?
    {
        return Err(invalid("file changed while reading; refresh"));
    }
    Ok(Readback {
        workspace_id: workspace.id,
        path: path.into(),
        size: before.len(),
        content,
    })
}

impl Workspace {
    pub async fn list_files(&self, path: &str) -> Result<Listing, Error> {
        let path = relative(path, true)?;
        let workspace = self.clone();
        job(move || listing(&workspace, &path)).await
    }

    pub async fn read_file(&self, path: &str) -> Result<Readback, Error> {
        let path = relative(path, false)?;
        let workspace = self.clone();
        job(move || readback(&workspace, &path)).await
    }

    /// Open an explicit local path without trying a directory read then a file read.
    pub async fn open_path(&self, path: &str) -> Result<Opened, Error> {
        let path = Path::new(path);
        let path = if path.is_absolute() {
            path.strip_prefix(&self.registration.directory.path)
                .map_err(|_| invalid("file path is outside the saved workspace"))?
        } else {
            path.strip_prefix(".").unwrap_or(path)
        };
        let path = relative(
            path.to_str().ok_or_else(|| invalid("path is not UTF-8"))?,
            true,
        )?;
        let workspace = self.clone();
        job(move || {
            let anchor = root(&workspace)?;
            if path.is_empty() { return listing(&workspace, &path).map(Opened::Directory); }
            let parent = Path::new(&path).parent().and_then(Path::to_str)
                .ok_or_else(|| invalid("invalid parent path"))?;
            let folder = directory(&anchor, parent)?;
            let name = Path::new(&path).file_name().ok_or_else(|| invalid("choose a file or directory"))?;
            let metadata = statat(&folder, name, AtFlags::SYMLINK_NOFOLLOW).map_err(invalid)?;
            match FileType::from_raw_mode(metadata.st_mode) {
                FileType::Directory => listing(&workspace, &path).map(Opened::Directory),
                FileType::RegularFile => readback(&workspace, &path).map(Opened::File),
                _ => Err(invalid("only regular files and directories can be opened; links and special files are not followed")),
            }
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hosts::Endpoint, storage::Store, workspaces::Kind as WorkspaceKind};
    use std::{
        os::unix::{ffi::OsStrExt, fs::symlink},
        path::PathBuf,
    };
    pub(super) static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[tokio::test]
    async fn command_read_path_uses_its_directory_and_keeps_literal_names() {
        let _guard = SERIAL.lock().await;
        let (base, workspace) = fixture().await;
        let root = base.join("work");
        let directory = root.join("子目录 ");
        std::fs::create_dir(&directory).unwrap();
        let name = "exact%20#tag:2.txt";
        std::fs::write(root.join(name), "wrong root file").unwrap();
        std::fs::write(directory.join(name), "exact 子目录\r\n  ").unwrap();
        let target = Target::in_directory(directory.to_str().unwrap(), name).unwrap();
        let Opened::File(file) = workspace.open_path(&target.path).await.unwrap() else {
            panic!("expected file");
        };
        assert!(matches!(file.content,Content::Text {text} if text == "exact 子目录\r\n  "));
        assert_eq!(target.line, None);
        let absolute = directory.join(name);
        assert_eq!(
            Target::in_directory("unused", absolute.to_str().unwrap()).unwrap(),
            target
        );
        for (cwd, path) in [
            ("relative", "file"),
            ("", "file"),
            ("/work", ""),
            ("/work", "bad\0path"),
        ] {
            assert!(Target::in_directory(cwd, path).is_err());
        }
        for (cwd, path) in [
            (directory.to_str().unwrap(), "../exact%20#tag:2.txt"),
            (base.to_str().unwrap(), "outside.txt"),
        ] {
            let target = Target::in_directory(cwd, path).unwrap();
            assert!(workspace.open_path(&target.path).await.is_err());
        }
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn explicit_links_decode_paths_and_lines_without_external_scheme_confusion() {
        assert_eq!(
            Link::parse("https://example.test/x#L2").unwrap(),
            Link::Web("https://example.test/x#L2".into())
        );
        for href in ["./目录/file%20name.txt#L2-L3", "./目录/file%20name.txt:2"] {
            assert_eq!(
                Link::parse(href).unwrap(),
                Link::Local(Target {
                    path: "./目录/file name.txt".into(),
                    line: NonZeroUsize::new(2)
                })
            );
        }
        for href in ["file:///tmp/name%23tag", "file://localhost/tmp/name%23tag"] {
            assert_eq!(
                Link::parse(href).unwrap(),
                Link::Local(Target {
                    path: "/tmp/name#tag".into(),
                    line: None
                })
            );
        }
        for href in [
            "javascript:alert(1)",
            "data:text/plain,hello",
            "mailto:x@example.test",
            "file://remote/tmp/x",
            "//remote/x",
            "a.rs#L0",
            "a.rs#L3-L1",
            "%FF",
            "%00",
            "a.rs#bad",
        ] {
            assert!(Link::parse(href).is_err(), "{href}");
        }
        assert_eq!(
            Link::parse("./literal%3A2").unwrap(),
            Link::Local(Target {
                path: "./literal:2".into(),
                line: None
            })
        );
    }

    #[tokio::test]
    async fn explicit_path_open_classifies_within_identity_and_preserves_large_folder_file_access()
    {
        let _guard = SERIAL.lock().await;
        let (base, workspace) = fixture().await;
        let root = base.join("work");
        let store = Store::open(base.join("state")).await.unwrap();
        assert_eq!(
            store
                .workspace_at(&workspace.registration.endpoint.socket, &root)
                .await
                .unwrap()
                .unwrap()
                .id,
            workspace.id
        );
        assert!(
            store
                .workspace_at(&base.join("other.sock"), &root)
                .await
                .unwrap()
                .is_none()
        );
        std::fs::create_dir(root.join("目录")).unwrap();
        std::fs::write(root.join("目录/line:2"), "exact\r\n中文  \n").unwrap();
        for path in [
            "目录/line:2".to_owned(),
            "./目录/line:2".into(),
            root.join("目录/line:2").to_str().unwrap().into(),
        ] {
            let Opened::File(file) = workspace.open_path(&path).await.unwrap() else {
                panic!("expected file");
            };
            assert_eq!(file.path, "目录/line:2");
            assert!(matches!(file.content,Content::Text {text} if text == "exact\r\n中文  \n"));
        }
        assert!(matches!(
            workspace.open_path("目录").await.unwrap(),
            Opened::Directory(_)
        ));
        assert!(matches!(
            workspace.open_path(root.to_str().unwrap()).await.unwrap(),
            Opened::Directory(_)
        ));
        symlink(&base, root.join("outside")).unwrap();
        std::fs::create_dir(root.join("many")).unwrap();
        for i in 0..501 {
            std::fs::write(root.join("many").join(i.to_string()), "exact").unwrap();
        }
        assert!(workspace.open_path("many").await.is_err());
        assert!(matches!(
            workspace.open_path("many/500").await.unwrap(),
            Opened::File(_)
        ));
        for path in [
            "../work/目录",
            "目录/../目录",
            "outside",
            "outside/state",
            "/etc/passwd",
            "missing",
        ] {
            assert!(workspace.open_path(path).await.is_err(), "{path}");
        }
        std::fs::rename(&root, base.join("old-root")).unwrap();
        std::fs::create_dir(&root).unwrap();
        assert!(workspace.open_path("").await.is_err());
        std::fs::remove_dir_all(base).unwrap();
    }

    pub(super) async fn fixture() -> (PathBuf, Workspace) {
        let base = std::env::temp_dir().join(format!("fp-files-{}", Uuid::new_v4()));
        std::fs::create_dir(&base).unwrap();
        let base = std::fs::canonicalize(base).unwrap();
        std::fs::create_dir(base.join("work")).unwrap();
        let store = Store::open(base.join("state")).await.unwrap();
        let workspace = store
            .register_workspace(
                Endpoint {
                    name: "Offline".into(),
                    socket: base.join("unused/control.sock"),
                },
                base.join("work"),
                WorkspaceKind::Folder,
            )
            .await
            .unwrap();
        (base, workspace)
    }

    #[tokio::test]
    async fn exact_text_nested_order_binary_empty_and_symlink_entries() {
        let _guard = SERIAL.lock().await;
        let (base, workspace) = fixture().await;
        let path = base.join("work");
        std::fs::create_dir(path.join("目录")).unwrap();
        let text = "中文\r\n  exact trailing space \n";
        std::fs::write(path.join("目录/line\nfile.txt"), text).unwrap();
        std::fs::write(path.join("empty"), "").unwrap();
        std::fs::write(path.join("bin"), [0, 1, 2]).unwrap();
        std::fs::write(path.join("invalid-utf8"), [255]).unwrap();
        symlink(path.join("目录"), path.join("link")).unwrap();
        symlink("missing", path.join("dangling")).unwrap();
        for _ in 0..2 {
            let listing = workspace.list_files(".").await.unwrap();
            assert_eq!(listing.entries[0].kind, Kind::Directory);
            assert_eq!(listing.entries[0].path, "目录");
            assert_eq!(
                listing
                    .entries
                    .iter()
                    .filter(|e| e.kind == Kind::Symlink)
                    .count(),
                2
            );
        }
        assert_eq!(
            workspace.list_files("目录").await.unwrap().entries[0].name,
            "line\nfile.txt"
        );
        assert!(
            matches!(workspace.read_file("目录/line\nfile.txt").await.unwrap().content,Content::Text {text: actual} if actual==text)
        );
        assert!(
            matches!(workspace.read_file("empty").await.unwrap().content,Content::Text {text} if text.is_empty())
        );
        for name in ["bin", "invalid-utf8"] {
            assert!(matches!(
                workspace.read_file(name).await.unwrap().content,
                Content::Binary
            ));
        }
        std::fs::remove_dir_all(base).unwrap();
    }

    fn encoded(format: image::ImageFormat, width: u32, height: u32) -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(width, height, image::Rgb([12, 180, 90]))
            .write_to(&mut bytes, format)
            .unwrap();
        bytes.into_inner()
    }

    #[tokio::test]
    async fn image_bytes_are_validated_preserved_and_reloaded_without_trusting_extensions() {
        let _guard = SERIAL.lock().await;
        let (base, workspace) = fixture().await;
        let root = base.join("work");
        for (name, format) in [
            ("图.png", image::ImageFormat::Png),
            ("image.jpg", image::ImageFormat::Jpeg),
            ("no-extension", image::ImageFormat::WebP),
        ] {
            let bytes = encoded(format, 37, 19);
            std::fs::write(root.join(name), &bytes).unwrap();
            let result = workspace.read_file(name).await.unwrap();
            let Content::Image { image, data_base64 } = result.content else {
                panic!("expected image for {name}")
            };
            assert_eq!((image.width, image.height), (37, 19));
            assert_eq!(STANDARD.decode(data_base64).unwrap(), bytes);
            assert_eq!(image.bytes, result.size);
            assert_eq!(image, Image::inspect(&bytes, name.into()).unwrap());
        }
        let bytes = encoded(image::ImageFormat::Png, 400, 300);
        std::fs::write(root.join("图.png"), &bytes).unwrap();
        let Opened::File(result) = workspace.open_path("图.png").await.unwrap() else {
            panic!("expected file")
        };
        assert!(matches!(result.content, Content::Image {image, ..} if image.width == 400));
        assert_eq!(std::fs::read(root.join("图.png")).unwrap(), bytes);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test]
    async fn image_corruption_format_and_dimensions_have_explicit_notices() {
        let _guard = SERIAL.lock().await;
        let (base, workspace) = fixture().await;
        let root = base.join("work");
        for (name, bytes) in [
            ("corrupt.png", b"broken".to_vec()),
            (
                "truncated",
                encoded(image::ImageFormat::Png, 1, 1)[..35].to_vec(),
            ),
            ("huge.png", encoded(image::ImageFormat::Png, 8193, 1)),
            ("unsupported", b"GIF89a".to_vec()),
        ] {
            std::fs::write(root.join(name), bytes).unwrap();
            assert!(
                matches!(
                    workspace.read_file(name).await.unwrap().content,
                    Content::Unavailable { .. }
                ),
                "{name}"
            );
        }
        let mut large = encoded(image::ImageFormat::Png, 1, 1);
        large.resize(MAX_IMAGE_BYTES as usize + 1, 0);
        std::fs::write(root.join("large.png"), large).unwrap();
        assert!(
            matches!(workspace.read_file("large.png").await.unwrap().content, Content::TooLarge {limit} if limit as u64 == MAX_IMAGE_BYTES)
        );
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test]
    async fn traversal_symlink_special_files_and_replaced_root_are_refused() {
        let _guard = SERIAL.lock().await;
        let (base, workspace) = fixture().await;
        let path = base.join("work");
        std::fs::write(base.join("outside"), "outside data").unwrap();
        symlink(&base, path.join("escape")).unwrap();
        symlink(base.join("outside"), path.join("outside-link")).unwrap();
        assert!(
            std::process::Command::new("mkfifo")
                .args(["-m", "600"])
                .arg(path.join("pipe"))
                .status()
                .unwrap()
                .success()
        );
        for name in [
            "",
            ".",
            "..",
            "../outside",
            "/etc/passwd",
            "a/../outside",
            "escape/outside",
            "outside-link",
            "pipe",
        ] {
            assert!(workspace.read_file(name).await.is_err(), "{name}");
        }
        assert!(workspace.list_files("escape").await.is_err());
        assert!(workspace.list_files("../").await.is_err());
        assert!(workspace.list_files(&"a/".repeat(65)).await.is_err());
        assert!(
            workspace
                .list_files("")
                .await
                .unwrap()
                .entries
                .iter()
                .any(|e| e.name == "pipe" && e.kind == Kind::Other)
        );
        std::fs::rename(&path, base.join("kept")).unwrap();
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("new"), "replacement").unwrap();
        assert!(workspace.list_files("").await.is_err());
        assert!(workspace.read_file("new").await.is_err());
        std::fs::remove_dir_all(&path).unwrap();
        symlink(base.join("kept"), &path).unwrap();
        assert!(workspace.list_files("").await.is_err());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test]
    async fn directory_file_and_utf8_name_limits_are_explicit() {
        let _guard = SERIAL.lock().await;
        let (base, workspace) = fixture().await;
        let path = base.join("work");
        std::fs::write(path.join("large"), vec![b'x'; MAX_TEXT_BYTES + 1]).unwrap();
        assert!(matches!(
            workspace.read_file("large").await.unwrap().content,
            Content::TooLarge {
                limit: MAX_TEXT_BYTES
            }
        ));
        std::fs::create_dir(path.join("many")).unwrap();
        for index in 0..=MAX_ENTRIES {
            std::fs::write(path.join("many").join(index.to_string()), "").unwrap();
        }
        assert!(workspace.list_files("many").await.is_err());
        std::fs::create_dir(path.join("non-utf8")).unwrap();
        let created = std::fs::write(
            path.join("non-utf8")
                .join(std::ffi::OsStr::from_bytes(&[255])),
            "data",
        );
        // APFS rejects this filename before the application can enumerate it.
        match created {
            Ok(()) => assert!(workspace.list_files("non-utf8").await.is_err()),
            #[cfg(target_os = "macos")]
            Err(error) => assert_eq!(
                error.raw_os_error(),
                Some(rustix::io::Errno::ILSEQ.raw_os_error())
            ),
            #[cfg(not(target_os = "macos"))]
            Err(error) => panic!("create non-UTF-8 fixture: {error}"),
        }
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test]
    async fn timed_out_workers_stay_bounded_across_runtime_replacement() {
        let _guard = SERIAL.lock().await;
        let mut releases = Vec::new();
        let mut tasks = Vec::new();
        for _ in 0..2 {
            let (release, blocked) = std::sync::mpsc::channel();
            releases.push(release);
            let (started, ready) = oneshot::channel();
            tasks.push(tokio::spawn(job(move || {
                started.send(()).unwrap();
                blocked.recv().unwrap();
                Ok(())
            })));
            ready.await.unwrap();
        }
        assert!(
            job(|| Ok(()))
                .await
                .unwrap_err()
                .to_string()
                .contains("two file operations")
        );
        for task in tasks {
            assert!(
                task.await
                    .unwrap()
                    .unwrap_err()
                    .to_string()
                    .contains("5 seconds")
            );
        }
        assert!(job(|| Ok(())).await.is_err());
        for release in releases {
            release.send(()).unwrap();
        }
        drop(FILE_JOBS.acquire_many(2).await.unwrap());
        // The native wrapper creates and drops a Tokio runtime per command.
        let (release, blocked) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(job(move || {
                    blocked.recv().unwrap();
                    Ok(())
                }))
        });
        let mut joined = tokio::task::spawn_blocking(move || worker.join().unwrap());
        let result = tokio::time::timeout(Duration::from_secs(8), &mut joined).await;
        release.send(()).unwrap();
        if result.is_err() {
            let _finished = joined.await.unwrap();
        }
        drop(FILE_JOBS.acquire_many(2).await.unwrap());
        assert!(
            result
                .expect("runtime drop must not wait on timed-out I/O")
                .unwrap()
                .is_err()
        );
        assert_eq!(job(|| Ok(7)).await.unwrap(), 7);
    }
}
