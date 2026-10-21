use super::*;
use fleetpane_runtime::attachments::{self, Store};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug)]
struct PastedImage {
    name: String,
    bytes: Vec<u8>,
}

fn clipboard_images(
    clipboard: &ClipboardItem,
    current: usize,
) -> Result<Option<Vec<PastedImage>>, String> {
    let images: Vec<_> = clipboard
        .entries
        .iter()
        .filter_map(|entry| match entry {
            ClipboardEntry::Image(image) => Some(image),
            _ => None,
        })
        .collect();
    if images.is_empty() {
        return Ok(None);
    }
    if images.len() + current > attachments::MAX_IMAGES {
        return Err(
            "A message can contain at most four images; draft was preserved without changes".into(),
        );
    }
    // Check the entire payload before allocating copies or publishing any assets.
    for image in &images {
        if !matches!(
            image.format,
            ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Webp
        ) {
            return Err("Image attachments support PNG, JPEG and WebP".into());
        }
        if image.bytes.is_empty() || image.bytes.len() as u64 > attachments::MAX_IMAGE_BYTES {
            return Err("Image must contain data and fit within 8 MiB".into());
        }
    }
    Ok(Some(
        images
            .into_iter()
            .map(|image| PastedImage {
                name: format!("Clipboard.{}", image.format.extension()),
                bytes: image.bytes.clone(),
            })
            .collect(),
    ))
}

impl HostView {
    pub(super) fn drop_images(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.supports_images() || !self.draft_sendable() {
            self.draft.images_error = Some(if !self.supports_images() {
                "Images unavailable for this Host/provider".into()
            } else {
                "Wait for the current draft operation before dropping images".into()
            });
            cx.notify();
            return;
        }
        self.import_image_paths(paths.paths().to_vec(), window, cx);
    }

    fn import_image_paths(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.draft.importing = true;
        self.draft.images_error = None;
        let socket = self.socket.clone();
        let current = self.draft_images().to_vec();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    background(import_paths(&socket, &current, paths))
                        .map_err(|error| error.to_string())
                })
                .await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.draft.importing = false;
                match result {
                    Ok(images) => this.edit_images(images, window, cx),
                    Err(error) => this.draft.images_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn supports_images(&self) -> bool {
        self.snapshot.as_ref().is_some_and(|s| {
            s.provider_info["fleetpaneAttachmentVersion"] == 1
                && (!self.is_acp()
                    || s.provider_info
                        .pointer("/agentCapabilities/promptCapabilities/image")
                        == Some(&Value::Bool(true)))
        })
    }

    pub(super) fn paste_images(
        &mut self,
        clipboard: &ClipboardItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let incoming = match clipboard_images(clipboard, self.draft_images().len()) {
            Ok(None) => return false,
            Ok(Some(images)) => images,
            Err(error) => {
                self.draft.images_error = Some(error);
                cx.notify();
                return true;
            }
        };
        if !self.supports_images() || !self.draft_sendable() {
            self.draft.images_error = Some(if !self.supports_images() {
                "Images unavailable for this Host/provider".into()
            } else {
                "Wait for the current draft operation before pasting images".into()
            });
            cx.notify();
            return true;
        }
        self.draft.images_error = None;
        self.draft.importing = true;
        let socket = self.socket.clone();
        let mut images = self.draft_images().to_vec();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    background(async move {
                        let store = Store::for_socket(&socket).map_err(Error::Input)?;
                        for image in incoming {
                            images.push(
                                store
                                    .import_bytes(image.bytes, image.name)
                                    .await
                                    .map_err(Error::Input)?,
                            );
                        }
                        attachments::validate(&images).map_err(Error::Input)?;
                        Ok(images)
                    })
                    .map_err(|e| e.to_string())
                })
                .await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.draft.importing = false;
                match result {
                    Ok(images) => this.edit_images(images, window, cx),
                    Err(error) => this.draft.images_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
        true
    }

    fn pick_images(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.draft_sendable() || !self.supports_images() {
            return;
        }
        self.draft.picking = true;
        self.draft.images_error = None;
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach images".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let paths = match selection.await {
                Ok(Ok(paths)) => Ok(paths),
                Ok(Err(error)) => Err(format!("Image picker: {error}")),
                Err(error) => Err(format!("Image picker closed unexpectedly: {error}")),
            };
            let _closed = this.update_in(cx, |this, window, cx| {
                this.draft.picking = false;
                match paths {
                    Ok(Some(paths)) => this.import_image_paths(paths, window, cx),
                    Ok(None) => {}
                    Err(error) => this.draft.images_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn render_images(&self, blocked: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let images = self.draft_images();
        let disabled = blocked || !self.draft_sendable();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .text_xs()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("attach-images")
                            .small()
                            .label(if self.draft.importing {
                                "Importing images…"
                            } else {
                                "Attach images"
                            })
                            .disabled(
                                disabled
                                    || !self.supports_images()
                                    || images.len() >= attachments::MAX_IMAGES,
                            )
                            .on_click(
                                cx.listener(|this, _, window, cx| this.pick_images(window, cx)),
                            ),
                    )
                    .child(if self.supports_images() {
                        "Attach, paste or drop PNG · JPEG · WebP · up to 4 × 8 MiB"
                    } else {
                        "Images unavailable for this Host/provider"
                    }),
            )
            .when_some(self.draft.images_error.as_ref(), |view, error| {
                view.child(div().text_color(cx.theme().danger).child(error.clone()))
            })
            .when(!images.is_empty(), |view| {
                view.child(
                    div()
                        .id("draft-images")
                        .flex()
                        .gap_2()
                        .overflow_x_scroll()
                        .children(images.iter().cloned().enumerate().map(|(index, image)| {
                            let preview = Store::for_socket(&self.socket)
                                .and_then(|store| store.path(&image));
                            let image_view = match preview {
                                Ok(path) => img(ImageSource::from(Arc::<Path>::from(path)))
                                    .w(px(56.))
                                    .h(px(48.))
                                    .object_fit(ObjectFit::Contain)
                                    .with_fallback(|| {
                                        div()
                                            .text_xs()
                                            .child("Image unavailable")
                                            .into_any_element()
                                    })
                                    .into_any_element(),
                                Err(error) => div().child(error).into_any_element(),
                            };
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .flex_shrink_0()
                                .child(image_view)
                                .child(
                                    div()
                                        .w(px(120.))
                                        .h(px(48.))
                                        .overflow_hidden()
                                        .child(image.name)
                                        .child(format!(
                                            "{} × {} · {} bytes",
                                            image.width, image.height, image.bytes
                                        )),
                                )
                                .child(
                                    Button::new(("remove-image", index))
                                        .small()
                                        .label("Remove")
                                        .disabled(disabled)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            let mut images = this.draft_images().to_vec();
                                            images.remove(index);
                                            this.edit_images(images, window, cx);
                                        })),
                                )
                        })),
                )
            })
    }
}

/// Picker and native drops have the same validation and all-or-nothing draft edit.
/// Valid assets imported before a later failure may remain in the bounded store.
async fn import_paths(
    socket: &Path,
    current: &[attachments::Image],
    paths: Vec<PathBuf>,
) -> Result<Vec<attachments::Image>, Error> {
    if paths.is_empty() || paths.len() + current.len() > attachments::MAX_IMAGES {
        return Err(Error::Input(
            "Choose one to four images in total; draft was preserved without changes".into(),
        ));
    }
    let store = Store::for_socket(socket).map_err(Error::Input)?;
    let mut images = current.to_vec();
    for path in paths {
        images.push(store.import(path).await.map_err(Error::Input)?);
    }
    attachments::validate(&images).map_err(Error::Input)?;
    Ok(images)
}

#[cfg(test)]
mod tests {
    use super::{attachments, clipboard_images, import_paths};
    use base64::{Engine, engine::general_purpose::STANDARD};
    use gpui_kit::{ClipboardEntry, ClipboardItem, ClipboardString, ImageFormat};

    #[tokio::test]
    async fn picked_and_dropped_paths_keep_order_and_refuse_entire_invalid_batch() {
        let base = std::env::temp_dir().join(format!("fp-dropped-images-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&base).unwrap();
        let socket = base.join("host.sock");
        let first = base.join("中文 %20#图.png");
        let second = base.join("second.png");
        std::fs::write(&first, STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAcAAAAFCAIAAAAG+GGPAAAAEUlEQVR4nGPYohGAiRhoJAoA3m4pBZ0fAUsAAAAASUVORK5CYII=").unwrap()).unwrap();
        std::fs::write(&second, STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAgAAAAFCAIAAAD38zoCAAAAEUlEQVR4nGPYohGAFTHQQQIAgnIu4STVK0EAAAAASUVORK5CYII=").unwrap()).unwrap();
        let current = import_paths(&socket, &[], vec![first.clone()])
            .await
            .unwrap();
        let updated = import_paths(&socket, &current, vec![second.clone()])
            .await
            .unwrap();
        assert_eq!(updated[0], current[0]);
        assert_eq!(updated[1].name, "second.png");
        assert_eq!((updated[0].width, updated[1].width), (7, 8));
        let invalid = base.join("bad.png");
        std::fs::write(&invalid, b"not an image").unwrap();
        let link = base.join("link.png");
        std::os::unix::fs::symlink(&first, &link).unwrap();
        for paths in [
            vec![],
            vec![first.clone()],
            vec![second.clone(), invalid],
            vec![link],
            vec![base.clone()],
            vec![second; 4],
        ] {
            assert!(import_paths(&socket, &current, paths).await.is_err());
            assert_eq!(current.len(), 1);
            assert_eq!(current[0].name, "中文 %20#图.png");
        }
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn clipboard_text_falls_through_and_image_batches_are_bounded() {
        assert!(
            clipboard_images(&ClipboardItem::new_string("中文\n  text".into()), 4)
                .unwrap()
                .is_none()
        );
        let image = gpui_kit::Image::from_bytes(ImageFormat::Png, vec![1, 2, 3]);
        let item = ClipboardItem::new_image(&image);
        let result = clipboard_images(&item, 3).unwrap().unwrap();
        assert_eq!(result[0].bytes, image.bytes);
        assert_eq!(result[0].name, "Clipboard.png");
        assert!(
            clipboard_images(&item, 4)
                .unwrap_err()
                .contains("four images")
        );
        let mixed = ClipboardItem {
            entries: vec![
                ClipboardEntry::Image(image.clone()),
                ClipboardEntry::String(ClipboardString::new("label".into())),
                ClipboardEntry::Image(image),
            ],
        };
        assert_eq!(clipboard_images(&mixed, 2).unwrap().unwrap().len(), 2);
        assert!(clipboard_images(&mixed, 3).is_err());
        for (format, bytes) in [
            (ImageFormat::Gif, vec![1]),
            (ImageFormat::Png, vec![]),
            (
                ImageFormat::Png,
                vec![0; attachments::MAX_IMAGE_BYTES as usize + 1],
            ),
        ] {
            let invalid = ClipboardItem::new_image(&gpui_kit::Image::from_bytes(format, bytes));
            assert!(clipboard_images(&invalid, 0).is_err());
        }
    }
}
