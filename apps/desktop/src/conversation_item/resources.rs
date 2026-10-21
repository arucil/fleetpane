//! Render supplied protocol resources without resolving or fetching their URIs.
use super::*;
use fleetpane_session::content::{Resource, ResourceLink};

pub(super) struct ResourceBlock {
    uri: SharedString,
    label: SharedString,
    metadata: Vec<SharedString>,
    text: Option<SharedString>,
    blob: Option<SharedString>,
    body: Option<Box<Block>>,
}

impl ResourceBlock {
    pub fn embedded(resource: &Resource, cx: &mut Context<ItemView>) -> Self {
        let body = resource.text.as_ref().map(|text| {
            let excerpt = excerpt(text);
            let block = if text.is_empty() {
                super::text("Empty text resource")
            } else if is_markdown(resource.mime_type.as_deref()) {
                Block::Markdown(cx.new(|cx| TextViewState::markdown(&excerpt, cx)))
            } else {
                code(&excerpt)
            };
            Box::new(block)
        });
        Self {
            uri: resource.uri.clone().into(),
            label: "Embedded resource".into(),
            metadata: resource
                .mime_type
                .iter()
                .map(|mime| format!("MIME: {}", metadata_excerpt(mime)).into())
                .collect(),
            text: resource.text.clone().map(Into::into),
            blob: resource.blob.clone().map(Into::into),
            body,
        }
    }

    pub fn link(link: &ResourceLink) -> Self {
        let mut metadata = vec![format!("Name: {}", metadata_excerpt(&link.name)).into()];
        if let Some(mime) = &link.mime_type {
            metadata.push(format!("MIME: {}", metadata_excerpt(mime)).into());
        }
        if let Some(size) = link.size {
            metadata.push(format!("{size} bytes").into());
        }
        if let Some(description) = &link.description {
            metadata.push(metadata_excerpt(description).into());
        }
        Self {
            uri: link.uri.clone().into(),
            label: format!(
                "Resource link · {}",
                metadata_excerpt(link.title.as_deref().unwrap_or(&link.name))
            )
            .into(),
            metadata,
            text: None,
            blob: None,
            body: None,
        }
    }

    pub fn render(&self, index: usize, cx: &mut Context<ItemView>) -> AnyElement {
        let uri = self.uri.clone();
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_md()
            .child(self.label.clone())
            .child(div().text_sm().child(metadata_excerpt(&self.uri)))
            .children(self.metadata.iter().map(|line| {
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(line.clone())
            }))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new(("resource-copy-uri", index))
                            .small()
                            .ghost()
                            .label("Copy URI")
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(uri.to_string()))
                            }),
                    )
                    .when_some(self.text.clone(), |row, text| {
                        row.child(
                            Button::new(("resource-copy-text", index))
                                .small()
                                .ghost()
                                .label("Copy text")
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        text.to_string(),
                                    ))
                                }),
                        )
                    })
                    .when_some(self.blob.clone(), |row, blob| {
                        row.child(
                            Button::new(("resource-copy-blob", index))
                                .small()
                                .ghost()
                                .label("Copy base64")
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        blob.to_string(),
                                    ))
                                }),
                        )
                    }),
            )
            .when_some(self.body.as_ref(), |card, body| {
                card.child(render_block(body, index, cx))
            })
            .when_some(self.blob.as_ref(), |card, blob| {
                card.child(
                    div()
                        .text_sm()
                        .child(format!("Binary content · {} base64 characters", blob.len())),
                )
            })
            .into_any_element()
    }
}

fn is_markdown(mime: Option<&str>) -> bool {
    mime.is_some_and(|mime| {
        mime.split(';')
            .next()
            .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("text/markdown"))
    })
}

/// Bound both bytes and lines: thousands of empty lines must not expand layout.
fn excerpt(text: &str) -> String {
    let line_end = text
        .match_indices('\n')
        .nth(64)
        .map_or(text.len(), |(index, _)| index);
    let mut end = text.len().min(8 * 1024).min(line_end);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    if end < text.len() {
        format!(
            "{}\n… Preview truncated · Copy text for full contents",
            &text[..end]
        )
    } else {
        text.to_owned()
    }
}

fn metadata_excerpt(value: &str) -> String {
    let mut end = value.len().min(512);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    let text = value[..end].replace(['\r', '\n'], " ");
    if end < value.len() {
        format!("{text}…")
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_preview_is_bounded_without_trimming_the_copy_payload() {
        let short = "  原文\r\n\n";
        assert_eq!(excerpt(short), short);
        assert_eq!(excerpt(""), "");
        let large = "字".repeat(16_000);
        let preview = excerpt(&large);
        assert!(preview.len() < 8 * 1024 + 100);
        assert!(preview.ends_with("Copy text for full contents"));
        assert!(excerpt(&"\n".repeat(50_000)).len() < 200);
        assert!(metadata_excerpt(&large).len() < 520);
        assert!(is_markdown(Some("Text/Markdown; charset=utf-8")));
        assert!(!is_markdown(Some("text/plain")));
        assert!(!is_markdown(None));
    }
}
