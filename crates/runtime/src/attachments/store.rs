use super::{Image, MAX_IMAGE_BYTES, MAX_STORE_BYTES};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

type Error = Box<dyn std::error::Error + Send + Sync>;
static IO_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
const MAX_FILES: usize = 4096;

#[derive(Clone, Debug)]
pub struct Store(PathBuf);

#[derive(Debug)]
pub struct Loaded {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

fn regular(path: &Path) -> Result<(File, std::fs::Metadata), Error> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(
            (rustix::fs::OFlags::NOFOLLOW.bits() | rustix::fs::OFlags::NONBLOCK.bits())
                .try_into()?,
        )
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_IMAGE_BYTES {
        return Err("Image source must be a nonempty regular file up to 8 MiB".into());
    }
    Ok((file, meta))
}

fn read(path: &Path) -> Result<Vec<u8>, Error> {
    let (mut file, before) = regular(path)?;
    let mut bytes = Vec::with_capacity(before.len() as usize);
    (&mut file)
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    if bytes.len() as u64 != before.len()
        || before.len() != after.len()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
    {
        return Err("Image changed while it was being imported or read".into());
    }
    Ok(bytes)
}

impl Store {
    pub fn for_socket(socket: &Path) -> Result<Self, String> {
        if !socket.is_absolute() || socket.as_os_str().len() > 4096 {
            return Err(
                "Image store requires an absolute local Host socket path up to 4 KiB".into(),
            );
        }
        let mut path = socket.as_os_str().to_os_string();
        path.push(".attachments");
        Ok(Self(PathBuf::from(path)))
    }

    pub fn path(&self, image: &Image) -> Result<PathBuf, String> {
        image.validate()?;
        Ok(self
            .0
            .join(format!("{}.{}", image.sha256, image.extension()?)))
    }

    /// File reads, decoding and fsync never execute on the UI or Host reactor.
    pub async fn import(&self, source: PathBuf) -> Result<Image, String> {
        let permit = IO_SLOTS
            .try_acquire()
            .map_err(|_| "Image I/O is busy; wait for current images to finish")?;
        let store = self.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            store
                .import_sync(&source)
                .map_err(|e| format!("Import image: {e}"))
        })
        .await
        .map_err(|e| format!("Image import worker failed: {e}"))?
    }

    /// Import encoded clipboard bytes through the same validation and publication as files.
    pub async fn import_bytes(&self, bytes: Vec<u8>, name: String) -> Result<Image, String> {
        if bytes.is_empty() || bytes.len() as u64 > MAX_IMAGE_BYTES {
            return Err("Image must contain data and fit within 8 MiB".into());
        }
        let permit = IO_SLOTS
            .try_acquire()
            .map_err(|_| "Image I/O is busy; wait for current images to finish")?;
        let store = self.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            store
                .publish_sync(&bytes, name)
                .map_err(|e| format!("Import clipboard image: {e}"))
        })
        .await
        .map_err(|e| format!("Image import worker failed: {e}"))?
    }

    pub async fn load(&self, image: Image) -> Result<Loaded, String> {
        image.validate()?;
        let permit = IO_SLOTS
            .try_acquire()
            .map_err(|_| "Image I/O is busy; wait for current images to finish")?;
        let store = self.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            store
                .load_sync(&image)
                .map_err(|e| format!("Load image: {e}"))
        })
        .await
        .map_err(|e| format!("Image load worker failed: {e}"))?
    }

    fn load_sync(&self, image: &Image) -> Result<Loaded, Error> {
        // Existing assets are never silently recreated if their directory is missing.
        let directory = std::fs::symlink_metadata(&self.0)?;
        if !directory.is_dir() || directory.permissions().mode() & 0o077 != 0 {
            return Err("Image store must be a private non-symlink directory (0700)".into());
        }
        let path = self.path(image)?;
        let meta = std::fs::symlink_metadata(&path)?;
        if !meta.is_file() || meta.permissions().mode() & 0o077 != 0 {
            return Err("Saved image must be a private non-symlink regular file".into());
        }
        let bytes = read(&path)?;
        if Image::inspect(&bytes, image.name.clone())? != *image {
            return Err("Saved image no longer matches its recorded identity".into());
        }
        Ok(Loaded { path, bytes })
    }

    fn import_sync(&self, source: &Path) -> Result<Image, Error> {
        if !source.is_absolute() {
            return Err("Choose an absolute image file path".into());
        }
        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("Image filename must be Unicode")?
            .to_owned();
        let bytes = read(source)?;
        self.publish_sync(&bytes, name)
    }

    fn publish_sync(&self, bytes: &[u8], name: String) -> Result<Image, Error> {
        let image = Image::inspect(bytes, name)?;
        crate::unix::prepare_directory(&self.0)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(
                (rustix::fs::OFlags::NOFOLLOW.bits() | rustix::fs::OFlags::NONBLOCK.bits())
                    .try_into()?,
            )
            .open(self.0.join(".lock"))?;
        if !lock.metadata()?.is_file() || lock.metadata()?.permissions().mode() & 0o077 != 0 {
            return Err("Image store lock must be a private regular file".into());
        }
        rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)
            .map_err(|e| format!("Image store is locked by another import: {e}"))?;
        let target = self.path(&image)?;
        match std::fs::symlink_metadata(&target) {
            Ok(_) => {
                self.load_sync(&image)?;
                // An earlier publication may have failed after creating its link.
                File::open(&self.0)?.sync_all()?;
                return Ok(image);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let mut count = 0;
        let mut total = 0_u64;
        for entry in std::fs::read_dir(&self.0)? {
            let entry = entry?;
            if entry.file_name() == ".lock" {
                continue;
            }
            count += 1;
            let metadata = std::fs::symlink_metadata(entry.path())?;
            if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
                return Err("Unexpected non-private or non-regular image store entry".into());
            }
            total = total
                .checked_add(metadata.len())
                .ok_or("Image store size overflow")?;
            if count >= MAX_FILES || total > MAX_STORE_BYTES {
                return Err("Image store reached its 256 MiB / 4096 file limit; saved assets were preserved".into());
            }
        }
        if total + image.bytes > MAX_STORE_BYTES {
            return Err(
                "Image store reached its 256 MiB limit; saved assets were preserved".into(),
            );
        }
        let temporary_path = self.0.join(format!(".import-{}", uuid::Uuid::new_v4()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary_path)?;
        let temporary = Temporary(temporary_path);
        file.write_all(bytes)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o400))?;
        file.sync_all()?;
        // A hard link publishes complete bytes without replacing any existing asset.
        std::fs::hard_link(&temporary.0, &target)?;
        std::fs::remove_file(&temporary.0)?;
        File::open(&self.0)?.sync_all()?;
        Ok(image)
    }
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        match std::fs::remove_file(&self.0) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => eprintln!("image temporary file cleanup: {error}"),
        }
    }
}
