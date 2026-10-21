//! Immutable local image assets. Control messages and drafts carry small identities.
#[cfg(any(unix, test))]
use image::{ImageFormat, ImageReader, Limits};
use serde::{Deserialize, Serialize};
#[cfg(any(unix, test))]
use sha2::{Digest, Sha256};
#[cfg(any(unix, test))]
use std::io::Cursor;

pub const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 16 * 1024 * 1024;
pub const MAX_IMAGES: usize = 4;
pub const MAX_STORE_BYTES: u64 = 256 * 1024 * 1024;

/// Provider content and the small local-history projection are deliberately separate.
pub(crate) struct Prompt {
    pub wire: Vec<serde_json::Value>,
    pub display: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Image {
    pub sha256: String,
    pub mime_type: String,
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
    pub name: String,
}

impl Image {
    pub fn validate(&self) -> Result<(), String> {
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.bytes == 0
            || self.bytes > MAX_IMAGE_BYTES
            || self.width == 0
            || self.height == 0
            || self.width > 8192
            || self.height > 8192
            || u64::from(self.width) * u64::from(self.height) > MAX_PIXELS
            || self.name.trim().is_empty()
            || self.name.len() > 1024
            || self.name.contains(['\0', '/'])
        {
            return Err("Invalid image attachment identity, name or bounds".into());
        }
        self.extension()?;
        Ok(())
    }

    fn extension(&self) -> Result<&'static str, String> {
        match self.mime_type.as_str() {
            "image/png" => Ok("png"),
            "image/jpeg" => Ok("jpg"),
            "image/webp" => Ok("webp"),
            _ => Err("Image attachments support PNG, JPEG and WebP".into()),
        }
    }

    #[cfg(any(unix, test))]
    fn inspect(bytes: &[u8], name: String) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() as u64 > MAX_IMAGE_BYTES {
            return Err("Image must contain data and fit within 8 MiB".into());
        }
        let format = image::guess_format(bytes).map_err(|e| format!("Image format: {e}"))?;
        let mime_type = match format {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::WebP => "image/webp",
            _ => return Err("Image attachments support PNG, JPEG and WebP".into()),
        };
        let (width, height) = ImageReader::with_format(Cursor::new(bytes), format)
            .into_dimensions()
            .map_err(|e| format!("Image dimensions: {e}"))?;
        let value = Self {
            sha256: format!("{:x}", Sha256::digest(bytes)),
            mime_type: mime_type.into(),
            bytes: bytes.len() as u64,
            width,
            height,
            name,
        };
        value.validate()?;
        let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
        let mut limits = Limits::default();
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        reader.decode().map_err(|e| format!("Image data: {e}"))?;
        Ok(value)
    }
}

pub fn validate(images: &[Image]) -> Result<(), String> {
    if images.len() > MAX_IMAGES {
        return Err("A message can contain at most four images".into());
    }
    let mut identities = std::collections::HashSet::new();
    for image in images {
        image.validate()?;
        if !identities.insert(&image.sha256) {
            return Err("The same image is already attached".into());
        }
    }
    Ok(())
}

#[cfg(unix)]
mod store;
#[cfg(unix)]
pub use store::{Loaded, Store};

#[cfg(unix)]
mod prompt;
#[cfg(unix)]
pub(crate) use prompt::{Prepared, prepare};

#[cfg(test)]
mod tests;
