use super::*;

fn encoded(format: ImageFormat) -> Vec<u8> {
    let image = image::RgbImage::from_fn(7, 5, |x, y| image::Rgb([x as u8, y as u8, 100]));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, format).unwrap();
    bytes.into_inner()
}

#[test]
fn original_formats_dimensions_and_invalid_data() {
    for (format, mime) in [
        (ImageFormat::Png, "image/png"),
        (ImageFormat::Jpeg, "image/jpeg"),
        (ImageFormat::WebP, "image/webp"),
    ] {
        let bytes = encoded(format);
        let image = Image::inspect(&bytes, "截图.data".into()).unwrap();
        assert_eq!((image.width, image.height), (7, 5));
        assert_eq!(image.mime_type, mime);
        assert_eq!(image.bytes, bytes.len() as u64);
        let mut changed = bytes;
        changed.truncate(changed.len() / 2);
        assert!(Image::inspect(&changed, "broken".into()).is_err());
        let mut wrong = image.clone();
        wrong.width = 8193;
        assert!(wrong.validate().is_err());
        wrong = image.clone();
        wrong.width = 8192;
        wrong.height = 8192;
        assert!(wrong.validate().is_err());
        assert!(validate(&[image.clone(), image]).is_err());
    }
    assert!(Image::inspect(&[], "empty".into()).is_err());
    assert!(Image::inspect(b"not an image", "bad.png".into()).is_err());
    assert!(Image::inspect(&vec![0; MAX_IMAGE_BYTES as usize + 1], "large".into()).is_err());
    assert!(Image::inspect(&encoded(ImageFormat::Png), "../escape".into()).is_err());
    let mut wide = Cursor::new(Vec::new());
    image::RgbImage::new(8193, 1)
        .write_to(&mut wide, ImageFormat::Png)
        .unwrap();
    assert!(Image::inspect(wide.get_ref(), "wide.png".into()).is_err());
    let images: Vec<_> = (0..5)
        .map(|i| {
            let mut image = Image::inspect(&encoded(ImageFormat::Png), "image.png".into()).unwrap();
            image.sha256 = format!("{i:064x}");
            image
        })
        .collect();
    assert!(validate(&images[..4]).is_ok());
    assert!(validate(&images).is_err());
}

#[cfg(unix)]
mod local {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
        path::PathBuf,
    };
    struct Temp(PathBuf);
    #[test]
    fn different_socket_suffixes_never_share_an_asset_directory() {
        let temp = Temp::new();
        let image = Image::inspect(&encoded(ImageFormat::Png), "image.png".into()).unwrap();
        let first = Store::for_socket(&temp.0.join("host.sock")).unwrap();
        let second = Store::for_socket(&temp.0.join("host.other")).unwrap();
        assert_ne!(first.path(&image).unwrap(), second.path(&image).unwrap());
    }
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("fp-image-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn store(&self) -> Store {
            Store::for_socket(&self.0.join("host.sock")).unwrap()
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    async fn durable_original_survives_source_removal_and_reopen() {
        let temp = Temp::new();
        let store = temp.store();
        let bytes = encoded(ImageFormat::Png);
        let source = temp.0.join("截图.jpg");
        fs::write(&source, &bytes).unwrap();
        let saved = store.import(source.clone()).await.unwrap();
        assert_eq!(saved.mime_type, "image/png");
        fs::remove_file(source).unwrap();
        let loaded = temp.store().load(saved.clone()).await.unwrap();
        assert_eq!(loaded.bytes, bytes);
        assert_eq!(loaded.path.extension().unwrap(), "png");
        assert_eq!(
            fs::metadata(&loaded.path).unwrap().permissions().mode() & 0o777,
            0o400
        );
        let duplicate = temp.0.join("renamed.webp");
        fs::write(&duplicate, &bytes).unwrap();
        let second = store.import(duplicate).await.unwrap();
        assert_eq!(saved.sha256, second.sha256);
        assert_eq!(store.path(&saved).unwrap(), store.path(&second).unwrap());
        assert_eq!(store.load(second).await.unwrap().bytes, bytes);
        let mut wrong = saved.clone();
        wrong.height += 1;
        assert!(store.load(wrong).await.unwrap_err().contains("identity"));
        fs::set_permissions(&loaded.path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&loaded.path, encoded(ImageFormat::Jpeg)).unwrap();
        assert!(store.load(saved).await.unwrap_err().contains("identity"));
    }

    async fn special_files_symlink_and_nonprivate_assets_are_rejected() {
        let temp = Temp::new();
        let store = temp.store();
        let source = temp.0.join("source.png");
        fs::write(&source, encoded(ImageFormat::Png)).unwrap();
        let link = temp.0.join("link.png");
        symlink(&source, &link).unwrap();
        assert!(store.import(link).await.is_err());
        assert!(store.import(temp.0.clone()).await.is_err());
        assert!(store.import(PathBuf::from("relative.png")).await.is_err());
        let saved = store.import(source.clone()).await.unwrap();
        let path = store.path(&saved).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(store.load(saved.clone()).await.is_err());
        fs::remove_file(&path).unwrap();
        symlink(source, &path).unwrap();
        assert!(store.load(saved).await.is_err());
        let fifo = temp.0.join("fifo");
        assert!(
            std::process::Command::new("mkfifo")
                .args(["-m", "600"])
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        assert!(store.import(fifo).await.is_err());
    }

    async fn quota_and_lock_failures_preserve_existing_assets() {
        let temp = Temp::new();
        let store = temp.store();
        let source = temp.0.join("a.png");
        fs::write(&source, encoded(ImageFormat::Png)).unwrap();
        let saved = store.import(source.clone()).await.unwrap();
        let root = temp.0.join("host.sock.attachments");
        let quota = root.join("occupied");
        let file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&quota)
            .unwrap();
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .unwrap();
        file.set_len(MAX_STORE_BYTES).unwrap();
        // Dedupe requires no new allocation, even when the store is full.
        assert_eq!(store.import(source.clone()).await.unwrap(), saved);
        fs::write(&source, encoded(ImageFormat::Jpeg)).unwrap();
        assert!(
            store
                .import(source.clone())
                .await
                .unwrap_err()
                .contains("limit")
        );
        assert!(store.load(saved.clone()).await.is_ok());
        fs::remove_file(quota).unwrap();
        let lock = fs::File::open(root.join(".lock")).unwrap();
        rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive).unwrap();
        assert!(store.import(source).await.unwrap_err().contains("locked"));
        assert!(store.load(saved).await.is_ok());
        assert!(!fs::read_dir(root).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".import-")
        }));
    }

    #[tokio::test]
    async fn private_durable_assets_and_failure_boundaries() {
        durable_original_survives_source_removal_and_reopen().await;
        special_files_symlink_and_nonprivate_assets_are_rejected().await;
        quota_and_lock_failures_preserve_existing_assets().await;
        clipboard_bytes_share_file_validation_and_durable_publication().await;
    }

    async fn clipboard_bytes_share_file_validation_and_durable_publication() {
        let temp = Temp::new();
        let store = temp.store();
        for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::WebP] {
            let bytes = encoded(format);
            let saved = store
                .import_bytes(bytes.clone(), "Clipboard.image".into())
                .await
                .unwrap();
            assert_eq!(temp.store().load(saved.clone()).await.unwrap().bytes, bytes);
            let source = temp.0.join("source.image");
            fs::write(&source, &bytes).unwrap();
            let from_file = store.import(source).await.unwrap();
            assert_eq!(saved.sha256, from_file.sha256);
            assert_eq!(store.path(&saved).unwrap(), store.path(&from_file).unwrap());
            assert!(validate(&[saved.clone(), from_file]).is_err());
            let mut invalid = bytes.clone();
            invalid.truncate(invalid.len() / 2);
            assert!(
                store
                    .import_bytes(invalid, "broken.png".into())
                    .await
                    .is_err()
            );
            assert_eq!(store.load(saved).await.unwrap().bytes, bytes);
        }
        for bytes in [
            Vec::new(),
            b"not an image".to_vec(),
            vec![0; MAX_IMAGE_BYTES as usize + 1],
        ] {
            assert!(store.import_bytes(bytes, "bad.png".into()).await.is_err());
        }
        assert!(
            store
                .import_bytes(encoded(ImageFormat::Png), "../escape".into())
                .await
                .is_err()
        );
    }
}
