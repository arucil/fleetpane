use fleetpane_session::{
    Item, Turn,
    content::{Content, Image as Attachment, Part},
    files::{Link, Target},
};
use gpui_kit::component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
    diff::{Diff, DiffFile, DiffState},
    text::{TextView, TextViewState},
};
use gpui_kit::{
    AnyElement, ClipboardItem, Context, Entity, Image, ImageFormat, ImageSource, ObjectFit, Render,
    ScrollHandle, SharedString, StyledImage, Subscription, Window, div, img, prelude::*, rems,
};
use serde_json::Value;
use std::{path::Path, sync::Arc};

const PREVIEW_BYTES: usize = 16 * 1024;

mod image_viewer;
mod resources;

#[derive(Clone, PartialEq)]
pub(super) struct Row {
    pub id: String,
    pub label: String,
    pub content: Content,
    pub copy: String,
    pub error: Option<String>,
    pub answer_turn: Option<String>,
}

impl Row {
    fn failed(&self) -> bool {
        self.error.is_some()
            || match &self.content {
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
                Content::Tool(call) => call.status == "failed",
                Content::Files(changes) => changes.status == "failed",
                _ => false,
            }
    }

    pub fn conversation(thread: &fleetpane_session::Thread) -> Vec<Self> {
        let mut rows = Vec::new();
        for turn in &thread.turns {
            rows.push(Self::turn(&thread.id, turn));
            for item in &turn.items {
                if let Some(row) = Self::item(&thread.id, &turn.id, item) {
                    rows.push(row);
                }
            }
        }
        rows
    }

    pub fn permission(
        id: String,
        pending: &Value,
        thread: Option<&fleetpane_session::Thread>,
    ) -> Self {
        let review = fleetpane_session::acp::Permission::decode(pending)
            .and_then(|request| request.ok_or_else(|| "Not an ACP permission".into()))
            .and_then(|request| request.review(thread));
        match review {
            Ok(tool) => Self {
                id,
                label: format!("Tool permission · {}", tool.display_title()),
                content: Content::Tool(tool),
                copy: pending.to_string(),
                error: None,
                answer_turn: None,
            },
            Err(error) => Self {
                id,
                label: "Invalid tool permission".into(),
                content: Content::Unknown(pending.clone()),
                copy: pending.to_string(),
                error: Some(error),
                answer_turn: None,
            },
        }
    }

    pub fn turn(thread_id: &str, turn: &Turn) -> Self {
        let mut label = format!("Turn · {}", turn.status);
        if let Some(reason) = turn.stop_reason {
            label.push_str(" · ");
            label.push_str(reason.description());
        }
        Self {
            id: Self::turn_key(thread_id, &turn.id),
            label,
            content: Content::Message(
                turn.error
                    .as_ref()
                    .map(|error| Part::Text(error.to_string()))
                    .into_iter()
                    .collect(),
            ),
            copy: serde_json::json!({"id":turn.id,"status":turn.status,"stopReason":turn.stop_reason,"error":turn.error}).to_string(),
            error: None,
            answer_turn: Some(turn.id.clone()),
        }
    }

    pub fn item_key(thread_id: &str, turn_id: &str, item_id: &str) -> String {
        row_key(&["item", thread_id, turn_id, item_id])
    }

    pub fn turn_key(thread_id: &str, turn_id: &str) -> String {
        row_key(&["turn", thread_id, turn_id])
    }

    pub fn item(thread_id: &str, turn_id: &str, item: &Item) -> Option<Self> {
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
            Content::Tool(call) => call.display_title().into(),
            Content::Command(command) => match command.command_actions.as_deref() {
                Some([fleetpane_session::content::CommandAction::Read { .. }]) => {
                    "Read file".into()
                }
                Some([fleetpane_session::content::CommandAction::ListFiles { .. }]) => {
                    "List files".into()
                }
                Some([fleetpane_session::content::CommandAction::Search { .. }]) => {
                    "Search files".into()
                }
                Some(actions) if actions.len() > 1 => {
                    format!("Command · {} reported actions", actions.len())
                }
                _ => "Command".into(),
            },
            Content::Mcp(call) => format!("{} · {}", call.server, call.tool),
            Content::WebSearch(_) => "Web search".into(),
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
            id: Self::item_key(thread_id, turn_id, &item.id),
            label,
            content,
            copy,
            error,
            answer_turn: None,
        })
    }
}

/// Provider IDs are opaque strings. A tagged tuple preserves boundaries,
/// empty IDs, case, quotes and Unicode without delimiter or hash collisions.
fn row_key(parts: &[&str]) -> String {
    serde_json::to_string(parts).expect("string identity tuples serialize")
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
        identity: SharedString,
    },
    Patch {
        state: Entity<DiffState>,
        _subscription: Subscription,
    },
    Error(SharedString),
    File {
        target: Target,
        label: SharedString,
    },
    Resource(Box<resources::ResourceBlock>),
}

#[derive(Clone)]
pub(super) struct OpenFile {
    pub cwd: String,
    pub target: Target,
}

pub(super) struct CopyAnswer {
    pub turn_id: String,
}

pub(super) struct ItemView {
    row: Row,
    cwd: String,
    blocks: Vec<Block>,
    details: Vec<Block>,
    expanded: bool,
    permission: bool,
    details_scroll: ScrollHandle,
    link_error: Option<String>,
}

impl gpui_kit::EventEmitter<OpenFile> for ItemView {}
impl gpui_kit::EventEmitter<CopyAnswer> for ItemView {}

impl ItemView {
    pub fn permission(row: Row, cwd: String, cx: &mut Context<Self>) -> Self {
        let mut view = Self::new(row, cwd, cx);
        view.expanded = true;
        view.permission = true;
        view
    }

    pub fn new(row: Row, cwd: String, cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            row,
            cwd,
            blocks: Vec::new(),
            details: Vec::new(),
            expanded: false,
            permission: false,
            details_scroll: ScrollHandle::new(),
            link_error: None,
        };
        view.prepare(cx);
        view
    }

    pub fn set_row(&mut self, row: Row, cx: &mut Context<Self>) {
        if self.row == row {
            return;
        }
        self.row = row;
        self.link_error = None;
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
                _ => part_block(part, &self.cwd, cx),
            })),
            Content::AgentParts(parts) => {
                for part in parts {
                    if let Part::Text(value) = part {
                        self.blocks.push(Block::Markdown(
                            cx.new(|cx| TextViewState::markdown(value, cx)),
                        ));
                    } else {
                        self.blocks.push(part_block(part, &self.cwd, cx));
                    }
                }
            }
            Content::Tool(call) => {
                if let Some(name) = &call.name {
                    self.details.push(text(&format!("Tool: {name}")));
                }
                if let Some(kind) = &call.kind {
                    self.details.push(text(&format!("Kind: {kind}")));
                }
                for location in call.locations.iter().flatten() {
                    let label = location.line.map_or_else(
                        || location.path.clone(),
                        |line| format!("{}:{line}", location.path),
                    );
                    self.details.push(Block::File {
                        target: Target {
                            path: location.path.clone(),
                            line: None,
                        },
                        label: label.into(),
                    });
                }
                if let Some(input) = &call.raw_input {
                    self.details.push(text("Input"));
                    self.details.push(code(&pretty(input)));
                }
                match call.parts() {
                    Ok(parts) => self
                        .details
                        .extend(parts.iter().map(|part| part_block(part, &self.cwd, cx))),
                    Err(error) => self.details.push(Block::Error(error.to_string().into())),
                }
                if let Some(output) = &call.raw_output {
                    self.details.push(code(&pretty(output)));
                }
            }
            Content::Command(command) => {
                use fleetpane_session::content::CommandAction;
                let summary = command_summary(&command.command);
                self.blocks.push(code(&summary));
                match &command.command_actions {
                    Some(actions) => {
                        self.details.push(text(&format!(
                            "{} provider-reported actions",
                            actions.len()
                        )));
                        for action in actions {
                            match action {
                                CommandAction::Read {
                                    command: action_command,
                                    name,
                                    path,
                                } => {
                                    self.blocks.push(
                                        match Target::in_directory(&command.cwd, path) {
                                            Ok(target) => Block::File {
                                                target,
                                                label: format!("Read · {name} · {path}").into(),
                                            },
                                            Err(error) => Block::Error(error.to_string().into()),
                                        },
                                    );
                                    self.details.push(code(action_command));
                                }
                                CommandAction::ListFiles { command, path } => {
                                    self.details.push(text("List files"));
                                    self.details.push(code(command));
                                    if let Some(path) = path {
                                        self.details.push(text(path));
                                    }
                                }
                                CommandAction::Search {
                                    command,
                                    path,
                                    query,
                                } => {
                                    self.details.push(text("Search files"));
                                    self.details.push(code(command));
                                    if let Some(path) = path {
                                        self.details.push(text(path));
                                    }
                                    if let Some(query) = query {
                                        self.details.push(text(query));
                                    }
                                }
                                CommandAction::Unknown { command } => {
                                    self.details.push(text("Unclassified action"));
                                    self.details.push(code(command));
                                }
                                CommandAction::Unsupported(raw) => {
                                    self.details.push(text("Unsupported command action"));
                                    self.details.push(code(&pretty(raw)));
                                }
                            }
                        }
                    }
                    None => self.details.push(text("Command actions unavailable")),
                }
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
                            .extend(parts.iter().map(|part| part_block(part, &self.cwd, cx))),
                        Err(error) => self.details.push(Block::Error(error.to_string().into())),
                    }
                    if let Some(structured) = &result.structured_content {
                        self.details.push(text("Structured result"));
                        self.details.push(code(&pretty(structured)));
                    }
                }
            }
            Content::WebSearch(search) => {
                use fleetpane_session::content::WebSearchAction;
                self.blocks.push(text(&search.query));
                match &search.action {
                    Some(WebSearchAction::Search { query, queries }) => {
                        self.details.push(text("Search"));
                        if let Some(query) = query {
                            self.details.push(text(query));
                        }
                        for query in queries.iter().flatten() {
                            self.details.push(text(query));
                        }
                    }
                    Some(WebSearchAction::OpenPage { url }) => {
                        self.details.push(text("Open page"));
                        if let Some(url) = url {
                            self.details.push(text(url));
                        }
                    }
                    Some(WebSearchAction::FindInPage { url, pattern }) => {
                        self.details.push(text("Find in page"));
                        if let Some(url) = url {
                            self.details.push(text(url));
                        }
                        if let Some(pattern) = pattern {
                            self.details.push(text(pattern));
                        }
                    }
                    Some(WebSearchAction::Other) => self.details.push(text("Other search action")),
                    Some(WebSearchAction::Unknown(raw)) => {
                        self.details.push(text("Unsupported search action"));
                        self.details.push(code(&pretty(raw)));
                    }
                    None => self.details.push(text("Search action unavailable")),
                }
                if let Some(results) = &search.results {
                    self.details
                        .push(text(&format!("{} supplied results", results.len())));
                    self.details
                        .push(code(&pretty(&serde_json::json!(results))));
                } else {
                    self.details.push(text("Search results unavailable"));
                }
            }
            Content::Files(changes) => {
                for change in &changes.changes {
                    self.blocks.push(Block::File {
                        target: Target {
                            path: change.path.clone(),
                            line: None,
                        },
                        label: change.path.clone().into(),
                    });
                }
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
}

impl Render for ItemView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let copy = self.row.copy.clone();
        let status = self.status();
        let failed = self.row.failed();
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
                    .when_some(self.row.answer_turn.as_ref(), |header, turn| {
                        let turn_id = turn.clone();
                        header.child(Button::new("copy-loaded-answer").small().ghost()
                            .label("Copy loaded answer")
                            .tooltip("Copy assistant text loaded for this turn. Earlier pages and non-text content are excluded.")
                            .on_click(cx.listener(move |_, _, _, cx| cx.emit(CopyAnswer {turn_id:turn_id.clone()}))))
                    })
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
                div().flex().flex_col().gap_2().children(
                    self.blocks
                        .iter()
                        .enumerate()
                        .map(|(index, block)| render_block(block, index, cx)),
                ),
            )
            .children(self.link_error.iter().map(|error| {
                div()
                    .text_color(cx.theme().danger)
                    .mb_2()
                    .child(error.clone())
            }))
            .when(self.expanded, |row| {
                row.child(
                    div()
                        .id("details-content")
                        .mt_2()
                        .when(self.permission, |details| {
                            details.max_h(window.viewport_size().height * 0.2)
                        })
                        .when(!self.permission, |details| details.max_h(rems(24.)))
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
                        .children(
                            self.details
                                .iter()
                                .enumerate()
                                .map(|(index, block)| render_block(block, index, cx)),
                        ),
                )
            })
    }
}

fn render_block(block: &Block, index: usize, cx: &mut Context<ItemView>) -> AnyElement {
    match block {
        Block::Resource(resource) => resource.render(index, cx),
        Block::Markdown(state) => {
            let owner = cx.entity().downgrade();
            TextView::new(state)
                .w_full()
                .on_link_click(move |href, _, _, cx| {
                    if let Some(view) = owner.upgrade() {
                        view.update(cx, |this, cx| match Link::parse(href) {
                            Ok(Link::Web(url)) => {
                                this.link_error = None;
                                cx.open_url(&url);
                                cx.notify();
                            }
                            Ok(Link::Local(target)) => {
                                this.link_error = None;
                                cx.emit(OpenFile {
                                    cwd: this.cwd.clone(),
                                    target,
                                });
                                cx.notify();
                            }
                            Err(error) => {
                                this.link_error = Some(error.to_string());
                                cx.notify();
                            }
                        });
                    }
                })
                .into_any_element()
        }
        Block::File { target, label } => {
            let target = target.clone();
            Button::new(("open-file", index))
                .small()
                .ghost()
                .label(format!("Open {}", preview(label)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.emit(OpenFile {
                        cwd: this.cwd.clone(),
                        target: target.clone(),
                    })
                }))
                .into_any_element()
        }
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
        Block::Image {
            source,
            label,
            identity,
        } => {
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
                .when_some(source.as_ref().ok(), |view, source| {
                    let source = source.clone();
                    let label = label.clone();
                    let identity = identity.clone();
                    view.child(
                        Button::new(("view-image", index))
                            .small()
                            .label("View image")
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                image_viewer::open(
                                    source.clone(),
                                    label.clone(),
                                    identity.clone(),
                                    window,
                                    cx,
                                );
                            }),
                    )
                })
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

fn part_block(part: &Part, cwd: &str, cx: &mut Context<ItemView>) -> Block {
    match part {
        Part::Text(value) => code(value),
        Part::Image(image) => image_block(image, cwd),
        Part::Resource(resource) => {
            Block::Resource(Box::new(resources::ResourceBlock::embedded(resource, cx)))
        }
        Part::ResourceLink(link) => Block::Resource(Box::new(resources::ResourceBlock::link(link))),
        Part::Unknown(value) => code(&pretty(value)),
    }
}

fn image_block(image: &Attachment, cwd: &str) -> Block {
    let (source, label, identity) = match image {
        Attachment::Local(path) => {
            let path = Path::new(cwd).join(path);
            let identity: SharedString = path.to_string_lossy().into_owned().into();
            (
                Ok(ImageSource::from(Arc::<Path>::from(path))),
                "Local image".into(),
                identity,
            )
        }
        Attachment::Url(url) => (
            image_url(url),
            "Image attachment".into(),
            url.clone().into(),
        ),
        Attachment::Encoded { mime_type, data } => {
            let identity = format!("data:{mime_type};base64,{data}");
            (
                image_url(&identity),
                format!("Tool image · {mime_type}").into(),
                identity.into(),
            )
        }
    };
    Block::Image {
        source,
        label,
        identity,
    }
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
        Err("Only supplied data URLs and local image paths can be displayed; other URLs are not fetched".into())
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
    fn tool_failures_use_the_same_error_presentation_as_codex() {
        for (status, output, expected) in [
            ("failed", json!(false), true),
            ("failed", json!({"message":"Original error 中文"}), true),
            ("completed", json!(false), false),
            ("pending", Value::Null, false),
            ("in_progress", json!(0), false),
            ("future_status", Value::Null, false),
        ] {
            let source = json!({"id":"tool","type":"acpToolCall","call":{
                "title":"Read settings","status":status,"rawOutput":output
            }});
            let item: Item = serde_json::from_value(source.clone()).unwrap();
            let row = Row::item("thread", "turn", &item).unwrap();
            assert_eq!(row.failed(), expected, "{status}: {output}");
            let copied: Value = serde_json::from_str(&row.copy).unwrap();
            assert_eq!(copied["call"], source["call"]);
        }
    }

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
        let row = Row::item("thread", "turn", &item).unwrap();
        assert_eq!(row.copy.len(), PREVIEW_BYTES * 3);
    }

    #[test]
    fn command_cards_classify_only_supplied_actions_and_preserve_whole_output() {
        for (action, label) in [
            (
                json!({"type":"read","command":"cat one","name":"one","path":"one"}),
                "Read file",
            ),
            (json!({"type":"listFiles","command":"ls"}), "List files"),
            (
                json!({"type":"search","command":"rg exact","query":" exact "}),
                "Search files",
            ),
        ] {
            let item:Item=serde_json::from_value(json!({"id":"c","type":"commandExecution","command":"opaque original command","cwd":"/work","status":"completed","commandActions":[action.clone()],"aggregatedOutput":" one\n二  ","exitCode":null,"future":42})).unwrap();
            let row = Row::item("thread", "turn", &item).unwrap();
            assert_eq!(row.label, label);
            assert!(!row.failed());
            let fields: Value = serde_json::from_str(&row.copy).unwrap();
            assert_eq!(fields["commandActions"], json!([action]));
            assert_eq!(fields["aggregatedOutput"], " one\n二  ");
            assert_eq!(fields["future"], 42);
            assert!(fields["exitCode"].is_null());
        }
        let item:Item=serde_json::from_value(json!({"id":"c","type":"commandExecution","command":"cat one","cwd":"/work","status":"completed"})).unwrap();
        assert_eq!(Row::item("thread", "turn", &item).unwrap().label, "Command");
        let item:Item=serde_json::from_value(json!({"id":"c","type":"commandExecution","command":"cat one","cwd":"/work","status":"completed","commandActions":[{"type":"read","command":"cat","name":"x"}]})).unwrap();
        let row = Row::item("thread", "turn", &item).unwrap();
        assert!(row.failed());
        assert_eq!(row.label, "Invalid commandExecution");
    }

    #[test]
    fn search_rows_keep_source_fields_without_inventing_completion() {
        let source = json!({"id":"search","type":"webSearch","query":" 中文 ","action":{"type":"future","extra":42},"results":[{"payload":"literal"}],"futureField":true});
        let item: Item = serde_json::from_value(source.clone()).unwrap();
        let row = Row::item("thread", "turn", &item).unwrap();
        assert_eq!(row.label, "Web search");
        assert!(!row.failed());
        assert!(matches!(row.content, Content::WebSearch(_)));
        let fields: Value = serde_json::from_str(&row.copy).unwrap();
        assert_eq!(fields["futureField"], true);
        assert_eq!(fields["action"], source["action"]);
        assert!(fields.get("status").is_none());
        let invalid:Item=serde_json::from_value(json!({"id":"bad","type":"webSearch","query":"x","action":{"type":"search","queries":[4]}})).unwrap();
        let row = Row::item("thread", "turn", &invalid).unwrap();
        assert!(row.failed());
        assert_eq!(row.label, "Invalid webSearch");
        assert_eq!(
            serde_json::from_str::<Value>(&row.copy).unwrap()["action"]["queries"],
            json!([4])
        );
    }

    #[test]
    fn turn_rows_distinguish_stop_reasons_and_copy_original_metadata() {
        for (reason, label) in [
            ("end_turn", "End of turn"),
            ("max_tokens", "Token limit reached"),
            ("max_turn_requests", "Turn request limit reached"),
            ("refusal", "Refused by provider"),
            ("cancelled", "Cancelled"),
        ] {
            let turn: Turn = serde_json::from_value(json!({"id":"turn","status":"completed","items":[],"error":null,"stopReason":reason})).unwrap();
            let row = Row::turn("thread", &turn);
            assert_eq!(row.label, format!("Turn · completed · {label}"));
            let copied: Value = serde_json::from_str(&row.copy).unwrap();
            assert_eq!(copied["stopReason"], reason);
            assert_eq!(copied["id"], "turn");
            assert!(matches!(row.content, Content::Message(parts) if parts.is_empty()));
        }
        let turn: Turn = serde_json::from_value(json!({"id":"failed","status":"failed","items":[],"error":{"message":"Original failure"}})).unwrap();
        let row = Row::turn("thread", &turn);
        assert_eq!(row.label, "Turn · failed");
        assert_eq!(
            serde_json::from_str::<Value>(&row.copy).unwrap()["error"],
            turn.error.unwrap()
        );
        assert!(
            matches!(row.content, Content::Message(parts) if matches!(&parts[..], [Part::Text(error)] if error.contains("Original failure")))
        );
    }

    #[test]
    fn invalid_image_data_is_visible_failure() {
        assert!(image_url("data:image/png;base64,invalid*").is_err());
        assert!(image_url("data:text/plain;base64,aGk=").is_err());
        assert!(image_url("data:image/png;base64,iVBORw0KGgo=").is_ok());
        for url in [
            "https://example.com/image.png",
            "file:///example/image.png",
            "",
        ] {
            assert!(matches!(image_url(url), Err(error) if error.contains("not fetched")));
        }
    }

    #[test]
    fn long_commands_use_a_summary_without_losing_the_script() {
        let script = "python3 - <<'PY'\nprint('你好')\nPY";
        assert_eq!(command_summary(script), "python3 - <<'PY' …");
        assert_eq!(command_summary("cargo test"), "cargo test");
        assert_eq!(command_summary(&"字".repeat(200)).chars().count(), 162);
    }
}
