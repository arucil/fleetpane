use fleetpane_session::{
    Item,
    content::{Content, Image as Attachment, Part},
};
use gpui_kit::component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
    diff::{Diff, DiffFile, DiffState},
    text::{TextView, TextViewState},
};
use gpui_kit::{
    AnyElement, App, ClipboardItem, Context, Entity, Image, ImageFormat, ImageSource, ObjectFit,
    Render, ScrollHandle, SharedString, SharedUri, StyledImage, Subscription, Window, div, img,
    prelude::*, rems,
};
use serde_json::Value;
use std::{path::Path, sync::Arc};

const PREVIEW_BYTES: usize = 16 * 1024;

#[derive(Clone, PartialEq)]
pub(super) struct Row {
    pub id: String,
    pub label: String,
    pub content: Content,
    pub copy: String,
    pub error: Option<String>,
}

impl Row {
    pub fn item(id: String, item: &Item) -> Option<Self> {
        let content = item.content();
        if matches!(content, Ok(None)) {
            return None;
        }
        let copy = match item.text() {
            Ok(text) => text,
            Err(error) => format!("Invalid item: {error}\n{:?}", item.fields),
        };
        let (content, error) = match content {
            Ok(Some(content)) => (content, None),
            Ok(None) => return None,
            Err(error) => (
                Content::Message(vec![Part::Text(copy.clone())]),
                Some(error.to_string()),
            ),
        };
        let label = match &content {
            Content::Markdown(_) => "Assistant".into(),
            Content::Message(_) => "You".into(),
            Content::AgentParts(_) => "Assistant".into(),
            Content::Tool(call) => call.title.clone(),
            Content::Command(_) => "Command".into(),
            Content::Mcp(call) => format!("{} · {}", call.server, call.tool),
            Content::Files(_) => "Changed files".into(),
            Content::Image(_) => "Image".into(),
            Content::Reasoning(_) => "Reasoning".into(),
            Content::Unknown(_) => item.kind.clone(),
        };
        let label = if error.is_some() {
            format!("Invalid {}", item.kind)
        } else {
            label
        };
        Some(Self {
            id,
            label,
            content,
            copy,
            error,
        })
    }
}

enum Block {
    Markdown(Entity<TextViewState>),
    Text {
        text: SharedString,
        code: bool,
    },
    Image {
        source: Result<ImageSource, String>,
        label: SharedString,
    },
    Patch {
        state: Entity<DiffState>,
        _subscription: Subscription,
    },
    Error(SharedString),
}

pub(super) struct ItemView {
    row: Row,
    cwd: String,
    blocks: Vec<Block>,
    details: Vec<Block>,
    expanded: bool,
    details_scroll: ScrollHandle,
}

impl ItemView {
    pub fn new(row: Row, cwd: String, cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            row,
            cwd,
            blocks: Vec::new(),
            details: Vec::new(),
            expanded: false,
            details_scroll: ScrollHandle::new(),
        };
        view.prepare(cx);
        view
    }

    pub fn set_row(&mut self, row: Row, cx: &mut Context<Self>) {
        if self.row == row {
            return;
        }
        self.row = row;
        self.prepare(cx);
        cx.notify();
    }

    fn prepare(&mut self, cx: &mut Context<Self>) {
        let previous_markdown = self.blocks.first().and_then(|block| match block {
            Block::Markdown(state) => Some(state.clone()),
            _ => None,
        });
        self.blocks.clear();
        self.details.clear();
        match &self.row.content {
            Content::Markdown(text) => {
                let text = preview(text);
                let state = if let Some(state) = previous_markdown {
                    state.update(cx, |state, cx| state.set_text(&text, cx));
                    state
                } else {
                    cx.new(|cx| TextViewState::markdown(&text, cx))
                };
                self.blocks.push(Block::Markdown(state));
            }
            Content::Message(parts) => self.blocks.extend(parts.iter().map(|part| match part {
                Part::Text(value) => text(value),
                _ => part_block(part, &self.cwd),
            })),
            Content::AgentParts(parts) => {
                for part in parts {
                    if let Part::Text(value) = part {
                        self.blocks.push(Block::Markdown(
                            cx.new(|cx| TextViewState::markdown(value, cx)),
                        ));
                    } else {
                        self.blocks.push(part_block(part, &self.cwd));
                    }
                }
            }
            Content::Tool(call) => {
                if let Some(input) = &call.raw_input {
                    self.details.push(text("Input"));
                    self.details.push(code(&pretty(input)));
                }
                match call.parts() {
                    Ok(parts) => self
                        .details
                        .extend(parts.iter().map(|part| part_block(part, &self.cwd))),
                    Err(error) => self.details.push(Block::Error(error.to_string().into())),
                }
                if let Some(output) = &call.raw_output {
                    self.details.push(code(&pretty(output)));
                }
            }
            Content::Command(command) => {
                let summary = command_summary(&command.command);
                self.blocks.push(code(&summary));
                if summary != command.command {
                    self.details.push(text("Command"));
                    self.details.push(code(&command.command));
                }
                if let Some(output) = &command.aggregated_output
                    && !output.is_empty()
                {
                    self.details.push(text("Output"));
                    self.details.push(code(output));
                }
            }
            Content::Mcp(call) => {
                self.details.push(text("Arguments"));
                self.details.push(code(&pretty(&call.arguments)));
                if let Some(error) = &call.error {
                    self.blocks
                        .push(Block::Error(preview(&pretty(error)).into()));
                }
                if let Some(result) = &call.result {
                    match result.parts() {
                        Ok(parts) => self
                            .details
                            .extend(parts.iter().map(|part| part_block(part, &self.cwd))),
                        Err(error) => self.details.push(Block::Error(error.to_string().into())),
                    }
                    if let Some(structured) = &result.structured_content {
                        self.details.push(text("Structured result"));
                        self.details.push(code(&pretty(structured)));
                    }
                }
            }
            Content::Files(changes) => {
                let patch = changes
                    .changes
                    .iter()
                    .map(|change| change.unified_patch())
                    .collect::<Result<Vec<_>, _>>();
                let parsed = patch
                    .map_err(|error| error.to_string())
                    .and_then(|patches| {
                        DiffFile::parse(&patches.join("\n")).map_err(|error| error.to_string())
                    });
                match parsed {
                    Ok(files) => {
                        let state = cx.new(|cx| DiffState::new(files, cx));
                        // Diff measures its viewport after layout. Forward that
                        // notification so the owning row paints the measured width.
                        let subscription = cx.observe(&state, |_, _, cx| cx.notify());
                        self.blocks.push(Block::Patch {
                            state,
                            _subscription: subscription,
                        });
                    }
                    Err(error) => {
                        self.blocks.push(Block::Error(
                            format!("Cannot display patch: {error}").into(),
                        ));
                        self.blocks
                            .extend(changes.changes.iter().map(|change| code(&change.diff)));
                    }
                }
            }
            Content::Image(image) => self.blocks.push(image_block(image, &self.cwd)),
            Content::Reasoning(reasoning) => self.details.push(text(reasoning)),
            Content::Unknown(value) => self.details.push(code(&pretty(value))),
        }
    }

    fn status(&self) -> Option<String> {
        let (status, duration, exit) = match &self.row.content {
            Content::Command(command) => (
                command.status.as_str(),
                command.duration_ms,
                command.exit_code,
            ),
            Content::Mcp(call) => (call.status.as_str(), call.duration_ms, None),
            Content::Tool(call) => (call.status.as_str(), None, None),
            Content::Files(changes) => (changes.status.as_str(), None, None),
            _ => return None,
        };
        let mut label = status.to_owned();
        if let Content::Mcp(call) = &self.row.content
            && call
                .result
                .as_ref()
                .is_some_and(|result| result.is_error == Some(true))
        {
            label.push_str(" · tool error");
        }
        if let Some(exit) = exit {
            label.push_str(&format!(" · exit {exit}"));
        }
        if let Some(duration) = duration {
            label.push_str(&format!(" · {duration} ms"));
        }
        Some(label)
    }

    fn failed(&self) -> bool {
        self.row.error.is_some()
            || match &self.row.content {
                Content::Command(command) => {
                    command.status == "failed" || command.exit_code.is_some_and(|exit| exit != 0)
                }
                Content::Mcp(call) => {
                    call.status == "failed"
                        || call.error.is_some()
                        || call
                            .result
                            .as_ref()
                            .is_some_and(|result| result.is_error == Some(true))
                }
                Content::Files(changes) => changes.status == "failed",
                _ => false,
            }
    }
}

impl Render for ItemView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let copy = self.row.copy.clone();
        let status = self.status();
        let failed = self.failed();
        let details_scroll = self.details_scroll.clone();
        let color = if failed {
            cx.theme().danger
        } else {
            cx.theme().muted_foreground
        };
        div()
            .id(SharedString::from(self.row.id.clone()))
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .mb_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .text_color(color)
                            .child(self.row.label.clone()),
                    )
                    .children(status.map(|status| div().text_xs().text_color(color).child(status)))
                    .when(!self.details.is_empty(), |header| {
                        header.child(
                            Button::new("details")
                                .small()
                                .ghost()
                                .label(if self.expanded {
                                    "Hide details"
                                } else {
                                    "Show details"
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.expanded = !this.expanded;
                                    cx.notify();
                                })),
                        )
                    })
                    .child(Button::new("copy").small().ghost().label("Copy").on_click(
                        move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
                        },
                    )),
            )
            .children(self.row.error.as_ref().map(|error| {
                div()
                    .text_color(cx.theme().danger)
                    .mb_2()
                    .child(format!("Invalid item: {error}"))
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(self.blocks.iter().map(|block| render_block(block, cx))),
            )
            .when(self.expanded, |row| {
                row.child(
                    div()
                        .id("details-content")
                        .mt_2()
                        .max_h(rems(24.))
                        .overflow_y_scroll()
                        .track_scroll(&self.details_scroll)
                        .on_scroll_wheel(move |_, _, cx| {
                            if details_scroll.max_offset().y > gpui_kit::Pixels::ZERO {
                                cx.stop_propagation();
                            }
                        })
                        .flex()
                        .flex_col()
                        .gap_2()
                        .children(self.details.iter().map(|block| render_block(block, cx))),
                )
            })
    }
}

fn render_block(block: &Block, cx: &App) -> AnyElement {
    match block {
        Block::Markdown(state) => TextView::new(state).w_full().into_any_element(),
        Block::Text { text, code: true } => div()
            .p_3()
            .bg(cx.theme().muted)
            .font_family(cx.theme().mono_font_family.clone())
            .text_sm()
            .child(text.clone())
            .into_any_element(),
        Block::Text { text, code: false } => div().child(text.clone()).into_any_element(),
        Block::Error(error) => div()
            .text_color(cx.theme().danger)
            .child(error.clone())
            .into_any_element(),
        Block::Patch { state, .. } => div()
            .id("patch-viewport")
            .occlude()
            .child(Diff::new(state).w_full().h(rems(18.)).soft_wrap(true))
            .into_any_element(),
        Block::Image { source, label } => {
            let image = match source {
                Ok(source) => img(source.clone())
                    .w_full()
                    .h(rems(16.))
                    .object_fit(ObjectFit::Contain)
                    .with_loading(|| div().child("Loading image…").into_any_element())
                    .with_fallback(|| {
                        div()
                            .child("Image unavailable — file missing, inaccessible or unsupported")
                            .into_any_element()
                    })
                    .into_any_element(),
                Err(error) => div()
                    .text_color(cx.theme().danger)
                    .child(format!("Image unavailable: {error}"))
                    .into_any_element(),
            };
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(image)
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(label.clone()),
                )
                .into_any_element()
        }
    }
}

fn text(value: &str) -> Block {
    Block::Text {
        text: preview(value).into(),
        code: false,
    }
}
fn code(value: &str) -> Block {
    Block::Text {
        text: preview(value).into(),
        code: true,
    }
}

fn pretty(value: &Value) -> String {
    // JSON values cannot fail serialization; this is not user-provided text parsing.
    serde_json::to_string_pretty(value).expect("JSON value serializes")
}

fn part_block(part: &Part, cwd: &str) -> Block {
    match part {
        Part::Text(value) => code(value),
        Part::Image(image) => image_block(image, cwd),
        Part::Unknown(value) => code(&pretty(value)),
    }
}

fn image_block(image: &Attachment, cwd: &str) -> Block {
    let (source, label) = match image {
        Attachment::Local(path) => {
            let path = Path::new(cwd).join(path);
            (
                Ok(ImageSource::from(Arc::<Path>::from(path))),
                "Local image".into(),
            )
        }
        Attachment::Url(url) => (image_url(url), "Image attachment".into()),
        Attachment::Encoded { mime_type, data } => (
            image_url(&format!("data:{mime_type};base64,{data}")),
            "Tool image".into(),
        ),
    };
    Block::Image { source, label }
}

fn image_url(url: &str) -> Result<ImageSource, String> {
    if url.starts_with("data:") {
        let data = data_url::DataUrl::process(url)
            .map_err(|error| format!("invalid data URL: {error:?}"))?;
        let mime = data.mime_type();
        let format = ImageFormat::from_mime_type(&format!("{}/{}", mime.type_, mime.subtype))
            .ok_or_else(|| "unsupported image format".to_owned())?;
        let (bytes, _) = data
            .decode_to_vec()
            .map_err(|error| format!("invalid image data: {error:?}"))?;
        Ok(ImageSource::from(Arc::new(Image::from_bytes(
            format, bytes,
        ))))
    } else {
        Ok(ImageSource::from(SharedUri::from(url.to_owned())))
    }
}

fn preview(value: &str) -> String {
    if value.len() <= PREVIEW_BYTES {
        return value.to_owned();
    }
    let mut end = PREVIEW_BYTES;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n[Preview limited to 16 KiB; Copy retains the full item.]",
        &value[..end]
    )
}

fn command_summary(command: &str) -> String {
    let summary: String = command
        .chars()
        .take_while(|ch| *ch != '\n')
        .take(160)
        .collect();
    if summary == command {
        summary
    } else {
        format!("{summary} …")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleetpane_session::content::FileChange;
    use serde_json::json;

    #[test]
    fn codex_patch_variants_parse_as_diff_files() {
        for change in [
            json!({"path":"/example/new file.rs","kind":{"type":"add"},"diff":"fn main() {}\n"}),
            json!({"path":"/example/gone","kind":{"type":"delete"},"diff":"old"}),
            json!({"path":"/example/old","kind":{"type":"update","move_path":"/example/new"},"diff":"@@ -1 +1 @@\n-old\n+new\n"}),
            json!({"path":"/example/empty","kind":{"type":"add"},"diff":""}),
        ] {
            let change: FileChange = serde_json::from_value(change).unwrap();
            let files = DiffFile::parse(&change.unified_patch().unwrap()).unwrap();
            assert_eq!(files.len(), 1);
        }
    }

    #[test]
    fn previews_keep_utf8_and_copy_source_is_complete() {
        let value = "字".repeat(PREVIEW_BYTES);
        let shown = preview(&value);
        assert!(shown.len() < value.len());
        assert!(shown.ends_with("Copy retains the full item.]"));
        let item: Item =
            serde_json::from_value(json!({"id":"a","type":"agentMessage","text":value})).unwrap();
        let row = Row::item("thread:turn:a".into(), &item).unwrap();
        assert_eq!(row.copy.len(), PREVIEW_BYTES * 3);
    }

    #[test]
    fn invalid_image_data_is_visible_failure() {
        assert!(image_url("data:image/png;base64,invalid*").is_err());
        assert!(image_url("data:text/plain;base64,aGk=").is_err());
        assert!(image_url("data:image/png;base64,iVBORw0KGgo=").is_ok());
    }

    #[test]
    fn long_commands_use_a_summary_without_losing_the_script() {
        let script = "python3 - <<'PY'\nprint('你好')\nPY";
        assert_eq!(command_summary(script), "python3 - <<'PY' …");
        assert_eq!(command_summary("cargo test"), "cargo test");
        assert_eq!(command_summary(&"字".repeat(200)).chars().count(), 162);
    }
}
