//! Read-only file navigation; filesystem work stays in the shared session engine.
use crate::{
    file_locations::{History, Location, Visit},
    host_view::background,
    image_navigation,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use fleetpane_session::{
    files::{Content, Kind, Link, Listing, Opened, Readback, Target},
    workspaces::Workspace,
};
use gpui_kit::component::{
    ActiveTheme, Disableable, Selectable, Sizable,
    button::{Button, ButtonVariants},
    input::{Editor, EditorState, Input, InputEvent, InputState, Position},
};
// The component facade does not expose Base's authoritative image-source override.
use gpui_kit::base::text::{TextView, TextViewState};
use gpui_kit::{prelude::*, *};
use std::{num::NonZeroUsize, path::Path, sync::Arc};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum TextMode {
    #[default]
    Source,
    Preview,
}

struct MarkdownPreview {
    state: Entity<TextViewState>,
    _observer: Subscription,
    image_placeholder: Arc<Image>,
}

enum Action {
    List(String),
    Restore(Visit),
    Read(String, Option<NonZeroUsize>),
    Open(Target),
    ImageStep { path: String, forward: bool },
}
struct PreparedFile {
    file: Readback,
    image: Option<Arc<Image>>,
}

enum Reply {
    Listed(Listing),
    Restored(
        Listing,
        Result<Option<PreparedFile>, fleetpane_session::client::Error>,
        Visit,
    ),
    Read(Readback, Option<NonZeroUsize>, Option<Arc<Image>>),
    Opened(Opened, Option<NonZeroUsize>, Option<Arc<Image>>),
    ImageStep(
        Listing,
        Result<PreparedFile, fleetpane_session::client::Error>,
    ),
}
pub struct FilesView {
    workspace: Workspace,
    location: Location,
    back_label: &'static str,
    line: Option<NonZeroUsize>,
    history: History,
    path: Entity<InputState>,
    _path_subscription: Subscription,
    listing: Option<Listing>,
    file: Option<Readback>,
    preview: Option<Entity<EditorState>>,
    text_mode: TextMode,
    markdown: Option<MarkdownPreview>,
    wrap: bool,
    image: Option<Arc<Image>>,
    busy: bool,
    error: Option<String>,
    export_open: bool,
    exporting: bool,
    export_path: Entity<InputState>,
    _export_subscription: Subscription,
    export_result: Option<String>,
    scroll: UniformListScrollHandle,
}
impl EventEmitter<Location> for FilesView {}

fn source_position(text: &str, line: Option<NonZeroUsize>) -> Result<Position, String> {
    let line = line.unwrap_or(NonZeroUsize::MIN).get();
    if line > 1 && text.match_indices('\n').nth(line - 2).is_none() {
        return Err(format!(
            "Requested line {line} does not exist; the file is loaded and can be copied."
        ));
    }
    Ok(Position::new(
        u32::try_from(line - 1).map_err(|_| "Source line exceeds the supported range")?,
        0,
    ))
}

fn language(path: &str) -> &'static str {
    if markdown_file(path) {
        return "markdown";
    }
    match std::path::Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
    {
        Some("rs") => "rust",
        Some("py" | "pyi") => "python",
        Some("js" | "jsx" | "mjs" | "cjs") => "javascript",
        Some("tsx") => "tsx",
        Some("ts" | "mts" | "cts") => "typescript",
        Some("json" | "jsonc") => "json",
        Some("go") => "go",
        Some("toml") => "toml",
        Some("yaml" | "yml") => "yaml",
        Some("sh" | "bash" | "zsh") => "bash",
        Some("html" | "htm") => "html",
        Some("css") => "css",
        Some("c" | "h") => "c",
        Some("cpp" | "cc" | "cxx" | "hpp") => "cpp",
        _ => "text",
    }
}

fn markdown_file(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
        })
}

// Link decoding stays shared with conversations. The displayed file, rather
// than the directory listing or editable path input, supplies the relative base.
// open_path remains responsible for traversal, symlink and saved-root checks.
fn markdown_link(
    root: &Path,
    displayed_file: &str,
    href: &str,
) -> Result<Link, fleetpane_session::client::Error> {
    match Link::parse(href)? {
        Link::Web(url) => Ok(Link::Web(url)),
        Link::Local(target) => {
            let file = root.join(displayed_file);
            let directory = file.parent().and_then(Path::to_str).ok_or_else(|| {
                fleetpane_session::client::Error::Input(
                    "Markdown file directory is not a local UTF-8 path".into(),
                )
            })?;
            let mut resolved = Target::in_directory(directory, &target.path)?;
            resolved.line = target.line;
            Ok(Link::Local(resolved))
        }
    }
}

// Called in the existing bounded command worker, never in render or the UI callback.
fn prepare_image(file: &Readback) -> Result<Option<Arc<Image>>, fleetpane_session::client::Error> {
    let Content::Image { image, data_base64 } = &file.content else {
        return Ok(None);
    };
    let format = ImageFormat::from_mime_type(&image.mime_type).ok_or_else(|| {
        fleetpane_session::client::Error::Input("Unsupported workspace image format".into())
    })?;
    let bytes = STANDARD.decode(data_base64).map_err(|error| {
        fleetpane_session::client::Error::Input(format!("Invalid workspace image bytes: {error}"))
    })?;
    Ok(Some(Arc::new(Image::from_bytes(format, bytes))))
}

impl FilesView {
    pub fn new(
        workspace: Workspace,
        location: Location,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let action = Action::Restore(Visit {
            location: location.clone(),
            ..Visit::default()
        });
        Self::build(
            workspace,
            location,
            action,
            "Back to workspaces",
            window,
            cx,
        )
    }

    pub fn target(
        workspace: Workspace,
        target: Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::build(
            workspace,
            Location::default(),
            Action::Open(target),
            "Back to conversation",
            window,
            cx,
        )
    }

    fn build(
        workspace: Workspace,
        location: Location,
        action: Action,
        back_label: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let initial_path = match &action {
            Action::Open(target) => &target.path,
            _ => &location.directory,
        };
        let path = cx.new(|cx| {
            let mut input = InputState::new(window, cx)
                .placeholder("Workspace file or directory (empty = root)");
            input.set_value(initial_path.clone(), window, cx);
            input
        });
        let subscription = cx.subscribe_in(&path, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.go(window, cx);
            }
        });
        let export_path =
            cx.new(|cx| InputState::new(window, cx).placeholder("Absolute path for a new copy"));
        let export_subscription = cx.subscribe_in(
            &export_path,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::PressEnter { .. } => this.export_file(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        );
        cx.on_release(|this, cx| {
            this.clear_image(cx);
            this.clear_markdown(cx);
        })
        .detach();
        let mut value = Self {
            workspace,
            location: location.clone(),
            back_label,
            line: None,
            history: History::default(),
            path,
            _path_subscription: subscription,
            listing: None,
            file: None,
            preview: None,
            text_mode: TextMode::Source,
            markdown: None,
            wrap: true,
            image: None,
            busy: false,
            error: None,
            export_open: false,
            exporting: false,
            export_path,
            _export_subscription: export_subscription,
            export_result: None,
            scroll: UniformListScrollHandle::new(),
        };
        value.focus(window, cx);
        value.run(action, window, cx);
        value
    }
    pub(crate) fn saved_location(&self) -> crate::file_locations::Location {
        self.location.clone()
    }
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.path.update(cx, |path, cx| path.focus(window, cx));
    }
    pub(crate) fn export_pending(&self) -> bool {
        self.exporting
    }

    fn export_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.export_open {
            return;
        }
        let Some(file) = &self.file else {
            return;
        };
        if file.size > fleetpane_session::files::MAX_EXPORT_BYTES {
            return;
        }
        let path = file.path.clone();
        let destination = std::path::PathBuf::from(self.export_path.read(cx).value().to_string());
        let workspace = self.workspace.clone();
        self.busy = true;
        self.exporting = true;
        self.error = None;
        self.export_result = None;
        let task = cx
            .background_executor()
            .spawn(async move { background(workspace.export_file(&path, destination)) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                this.exporting = false;
                match result {
                    Ok(exported) => {
                        this.export_result = Some(format!(
                            "Saved {} bytes to {} · SHA-256 {}",
                            exported.size,
                            exported.destination.display(),
                            exported.sha256
                        ));
                        this.export_open = false;
                    }
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn folder(&self) -> String {
        self.location.directory.clone()
    }

    fn go(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.run(
            Action::Open(Target {
                path: self.path.read(cx).value().to_string(),
                line: None,
            }),
            window,
            cx,
        );
    }

    fn clear_image(&mut self, cx: &mut App) {
        if let Some(image) = self.image.take() {
            ImageSource::from(image).remove_asset(cx);
        }
    }

    fn clear_markdown(&mut self, cx: &mut App) {
        if let Some(markdown) = self.markdown.take() {
            ImageSource::from(markdown.image_placeholder).remove_asset(cx);
        }
        self.text_mode = TextMode::Source;
    }

    fn set_text_mode(&mut self, mode: TextMode, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(file) = &self.file else {
            return;
        };
        let Content::Text { text } = &file.content else {
            return;
        };
        if !markdown_file(&file.path) {
            return;
        }
        if mode == TextMode::Preview && self.markdown.is_none() {
            let state = cx.new(|cx| TextViewState::markdown(text, cx));
            // Larger documents parse asynchronously. Observe completion even
            // when the parent has no other reason to repaint.
            let observer = cx.observe(&state, |_, _, cx| cx.notify());
            self.markdown = Some(MarkdownPreview {
                state,
                _observer: observer,
                image_placeholder: Arc::new(Image::from_bytes(
                    ImageFormat::Svg,
                    br##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="48" viewBox="0 0 64 48"><rect x="1" y="1" width="62" height="46" rx="4" fill="#dddddd" stroke="#666666"/><path d="M12 36 26 20 36 30 44 22 54 36M10 8 54 40" fill="none" stroke="#666666" stroke-width="3"/></svg>"##.to_vec(),
                )),
            });
        }
        self.text_mode = mode;
        cx.notify();
    }

    fn render_text(&self, cx: &mut Context<Self>) -> AnyElement {
        match self.text_mode {
            TextMode::Source => {
                let Some(state) = &self.preview else {
                    return div().into_any_element();
                };
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        Editor::new(state)
                            .readonly(true)
                            .aria_label("Read-only workspace source")
                            .h_full(),
                    )
                    .into_any_element()
            }
            TextMode::Preview => {
                let markdown = self
                    .markdown
                    .as_ref()
                    .expect("Preview mode retains its Markdown state");
                let file = self.file.as_ref().expect("Preview mode has a loaded file");
                let displayed_file = file.path.clone();
                let image_placeholder = markdown.image_placeholder.clone();
                let owner = cx.entity().downgrade();
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        TextView::new(&markdown.state)
                            .w_full()
                            .h_full()
                            .selectable(true)
                            .scrollable(true)
                            // Covers intrinsic measurement as well as rendering,
                            // including HTML and data images. Never return a URI.
                            .image_source(move |_| ImageSource::from(image_placeholder.clone()))
                            .on_link_click(move |href, _, window, cx| {
                                if let Some(view) = owner.upgrade() {
                                    view.update(cx, |this, cx| {
                                        if this.busy
                                            || this.text_mode != TextMode::Preview
                                            || !this
                                                .file
                                                .as_ref()
                                                .is_some_and(|file| file.path == displayed_file)
                                        {
                                            return;
                                        }
                                        match markdown_link(
                                            &this.workspace.registration.directory.path,
                                            &displayed_file,
                                            href,
                                        ) {
                                            Ok(Link::Web(url)) => {
                                                this.error = None;
                                                cx.open_url(&url);
                                                cx.notify();
                                            }
                                            Ok(Link::Local(target)) => {
                                                this.run(Action::Open(target), window, cx);
                                            }
                                            Err(error) => {
                                                this.error = Some(error.to_string());
                                                cx.notify();
                                            }
                                        }
                                    });
                                }
                            }),
                    )
                    .into_any_element()
            }
        }
    }

    fn show_file(
        &mut self,
        file: Readback,
        image: Option<Arc<Image>>,
        line: Option<NonZeroUsize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_open = false;
        self.export_result = None;
        self.location.file = Some(file.path.clone());
        self.line = line;
        self.clear_image(cx);
        self.image = image;
        if self.image.is_some() && line.is_some() {
            self.error = Some("Source line anchors apply to text files; showing the image.".into());
        }
        self.preview = match &file.content {
            Content::Text { text } => match source_position(text, line) {
                Ok(position) => Some(cx.new(|cx| {
                    let mut state = EditorState::new(window, cx)
                        .language(language(&file.path))
                        .soft_wrap(self.wrap)
                        .folding(false);
                    state.set_value(text, window, cx);
                    if line.is_some() {
                        state.set_cursor_position(position, window, cx);
                    }
                    state
                })),
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            },
            _ => None,
        };
        self.file = Some(file);
    }
    fn top_entry(&self) -> Option<String> {
        let listing = self.listing.as_ref()?;
        if listing.entries.is_empty() {
            return None;
        }
        let state = self.scroll.0.borrow();
        let index = match &state.deferred_scroll_to_item {
            Some(pending) => pending.item_index,
            None => {
                let size = state.last_item_size?;
                // UniformList virtualizes its children; ScrollHandle's child index
                // is not the index in our directory. Use the uniform row geometry.
                let row = f32::from(size.contents.height) / listing.entries.len() as f32;
                (-f32::from(state.base_handle.offset().y) / row)
                    .max(0.0)
                    .floor() as usize
            }
        };
        listing.entries.get(index).map(|entry| entry.path.clone())
    }

    fn navigate(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((index, visit)) = self.history.destination(forward) {
            self.run_navigation(Action::Restore(visit), Some(index), window, cx);
        }
    }

    fn run(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        self.run_navigation(action, None, window, cx);
    }

    fn run_navigation(
        &mut self,
        action: Action,
        destination: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.history
            .remember_scroll(&self.location, self.line, self.top_entry());
        if let Action::Restore(visit) = &action {
            let attempted = visit
                .location
                .file
                .as_ref()
                .unwrap_or(&visit.location.directory);
            self.path.update(cx, |input, cx| {
                input.set_value(attempted.clone(), window, cx)
            });
        }
        self.busy = true;
        self.error = None;
        self.clear_markdown(cx);
        let workspace = self.workspace.clone();
        let task = cx.background_executor().spawn(async move {
            background(async move {
                match action {
                    Action::ImageStep { path, forward } => {
                        let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
                        let listing = workspace.list_files(parent).await?;
                        let file = match image_navigation::destination(&listing, &path, forward) {
                            Ok(destination) => {
                                workspace.read_file(&destination).await.and_then(|file| {
                                    let image = prepare_image(&file)?;
                                    Ok(PreparedFile { file, image })
                                })
                            }
                            Err(error) => Err(fleetpane_session::client::Error::Input(error)),
                        };
                        Ok(Reply::ImageStep(listing, file))
                    }
                    Action::List(path) => workspace.list_files(&path).await.map(Reply::Listed),
                    Action::Restore(visit) => {
                        let listing = workspace.list_files(&visit.location.directory).await?;
                        let file = match &visit.location.file {
                            Some(path) => workspace.read_file(path).await.and_then(|file| {
                                let image = prepare_image(&file)?;
                                Ok(Some(PreparedFile { file, image }))
                            }),
                            None => Ok(None),
                        };
                        Ok(Reply::Restored(listing, file, visit))
                    }
                    Action::Read(path, line) => workspace.read_file(&path).await.and_then(|file| {
                        let image = prepare_image(&file)?;
                        Ok(Reply::Read(file, line, image))
                    }),
                    Action::Open(target) => {
                        workspace.open_path(&target.path).await.and_then(|opened| {
                            let image = match &opened {
                                Opened::File(file) => prepare_image(file)?,
                                Opened::Directory(_) => None,
                            };
                            Ok(Reply::Opened(opened, target.line, image))
                        })
                    }
                }
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let reply = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                let mut success = reply.is_ok();
                match reply {
                    Ok(Reply::ImageStep(listing, file)) => {
                        this.listing = Some(listing);
                        match file {
                            Ok(prepared) => {
                                this.show_file(prepared.file, prepared.image, None, window, cx)
                            }
                            Err(error) => {
                                this.error = Some(format!(
                                    "Image navigation failed; previous preview retained: {error}"
                                ));
                                success = false;
                            }
                        }
                    }
                    Ok(Reply::Restored(listing, file, visit)) => {
                        this.location = Location {
                            directory: listing.path.clone(),
                            file: None,
                        };
                        let top = visit
                            .top
                            .as_ref()
                            .and_then(|path| {
                                listing.entries.iter().position(|entry| &entry.path == path)
                            })
                            .unwrap_or(0);
                        this.listing = Some(listing);
                        this.file = None;
                        this.preview = None;
                        this.clear_image(cx);
                        this.line = None;
                        this.scroll.scroll_to_item_strict(top, ScrollStrategy::Top);
                        match file {
                            Ok(Some(prepared)) => this.show_file(
                                prepared.file,
                                prepared.image,
                                visit.line,
                                window,
                                cx,
                            ),
                            Ok(None) => {}
                            Err(error) => {
                                this.error = Some(error.to_string());
                                success = false;
                            }
                        }
                    }
                    Ok(Reply::Listed(listing)) => {
                        this.location = Location {
                            directory: listing.path.clone(),
                            file: None,
                        };
                        this.path.update(cx, |path, cx| {
                            path.set_value(listing.path.clone(), window, cx)
                        });
                        this.listing = Some(listing);
                        this.file = None;
                        this.preview = None;
                        this.clear_image(cx);
                        this.line = None;
                        this.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
                    }
                    Ok(Reply::Read(file, line, image)) => {
                        this.show_file(file, image, line, window, cx)
                    }
                    Ok(Reply::Opened(Opened::Directory(listing), _, _)) => {
                        this.location = Location {
                            directory: listing.path.clone(),
                            file: None,
                        };
                        this.path.update(cx, |input, cx| {
                            input.set_value(listing.path.clone(), window, cx)
                        });
                        this.listing = Some(listing);
                        this.file = None;
                        this.preview = None;
                        this.clear_image(cx);
                        this.line = None;
                        this.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
                    }
                    Ok(Reply::Opened(Opened::File(file), line, image)) => {
                        this.location.directory = file
                            .path
                            .rsplit_once('/')
                            .map_or("", |(parent, _)| parent)
                            .to_owned();
                        this.path.update(cx, |input, cx| {
                            input.set_value(file.path.clone(), window, cx)
                        });
                        this.listing = None;
                        this.show_file(file, image, line, window, cx);
                    }
                    Err(error) => {
                        this.error = Some(error.to_string());
                        this.file = None;
                        this.preview = None;
                        this.clear_image(cx);
                        this.line = None;
                        this.location.file = None;
                    }
                }
                if success {
                    let selected = this
                        .location
                        .file
                        .as_ref()
                        .unwrap_or(&this.location.directory);
                    this.path.update(cx, |input, cx| {
                        input.set_value(selected.clone(), window, cx)
                    });
                    this.history.accept(
                        Visit {
                            location: this.location.clone(),
                            line: this.line,
                            top: this.top_entry(),
                        },
                        destination,
                    );
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn render_image_navigation(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(file) = &self.file else {
            return div().into_any_element();
        };
        if !image_navigation::named_image(&file.path) {
            return div().into_any_element();
        }
        let Some(listing) = &self.listing else {
            return Button::new("load-image-list")
                .small()
                .label("Load image list")
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.run(
                        Action::Restore(Visit {
                            location: this.location.clone(),
                            line: this.line,
                            top: None,
                        }),
                        window,
                        cx,
                    )
                }))
                .into_any_element();
        };
        let Some(position) = image_navigation::position(listing, &file.path) else {
            return div()
                .text_sm()
                .child("Current image is absent from the image list. Refresh folder to update it.")
                .into_any_element();
        };
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .children([false, true].map(|forward| {
                let path = file.path.clone();
                Button::new(if forward {
                    "next-workspace-image"
                } else {
                    "previous-workspace-image"
                })
                .small()
                .label(if forward {
                    "Next image"
                } else {
                    "Previous image"
                })
                .disabled(
                    self.busy
                        || if forward {
                            position.next.is_none()
                        } else {
                            position.previous.is_none()
                        },
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.run(
                        Action::ImageStep {
                            path: path.clone(),
                            forward,
                        },
                        window,
                        cx,
                    )
                }))
            }))
            .child(div().text_sm().child(format!(
                "Image {} of {} · PNG/JPEG/WebP names",
                position.index + 1,
                position.total
            )))
            .into_any_element()
    }
}

impl Render for FilesView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let folder = self.folder();
        div().size_full().flex().flex_col().gap_3().p_4()
            .child(div().flex().items_center().gap_3()
                .child(Button::new("back-workspaces").small().label(self.back_label).disabled(self.exporting).on_click(cx.listener(|this,_,_,cx|cx.emit(this.location.clone()))))
                .child(div().text_lg().child("Workspace files"))
                .child(div().flex_1())
                .child(Button::new("files-back").small().label("Back").disabled(self.busy || self.history.destination(false).is_none()).on_click(cx.listener(|this,_,window,cx|this.navigate(false,window,cx))))
                .child(Button::new("files-forward").small().label("Forward").disabled(self.busy || self.history.destination(true).is_none()).on_click(cx.listener(|this,_,window,cx|this.navigate(true,window,cx)))))
            .child(div().text_sm().truncate().child(format!("{} · {}",self.workspace.registration.endpoint.name,self.workspace.registration.directory.path.display())))
            .child(div().flex().items_center().gap_2()
                .child(Button::new("files-root").small().label("Root").disabled(self.busy).on_click(cx.listener(|this,_,window,cx|this.run(Action::List(String::new()),window,cx))))
                .child(Button::new("files-up").small().label("Up").disabled(self.busy || folder.is_empty()).on_click(cx.listener(|this,_,window,cx| {
                    let folder=this.folder();let parent=folder.rsplit_once('/').map_or("",|(parent,_)|parent).to_owned();this.run(Action::List(parent),window,cx);
                })))
                .child(Button::new("files-refresh").small().label("Refresh folder").disabled(self.busy).on_click(cx.listener(|this,_,window,cx|this.run(Action::Restore(Visit {location:this.location.clone(),line:this.line,top:this.top_entry()}),window,cx))))
                .child(div().flex_1().min_w_0().child(Input::new(&self.path).disabled(self.busy)))
                .child(Button::new("files-go").small().label("Go").disabled(self.busy).on_click(cx.listener(|this,_,window,cx|this.go(window,cx)))))
            .when(self.busy,|view|view.child(div().text_sm().child(if self.exporting {"Saving copy… wait for the result before leaving."} else {"Reading…"})))
            .children(self.error.iter().map(|e|div().text_sm().text_color(cx.theme().danger).child(e.clone())))
            .when_some(self.export_result.as_ref(),|view,result| {
                let copy=result.clone();
                view.child(div().flex().items_center().gap_2()
                    .child(div().flex_1().min_w_0().text_sm().truncate().child(result.clone()))
                    .child(Button::new("copy-export-result").small().label("Copy result").on_click(move|_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(copy.clone())))))
            })
            .when(self.export_open, |view| view.child(div().flex().flex_col().gap_2()
                .child("Save current file bytes as a new copy · up to 64 MiB · existing files are kept")
                .child(div().flex().gap_2()
                    .child(div().flex_1().min_w_0().child(Input::new(&self.export_path).disabled(self.busy)))
                    .child(Button::new("export-save").small().label("Save copy").disabled(self.busy || self.export_path.read(cx).value().is_empty()).on_click(cx.listener(|this,_,window,cx|this.export_file(window,cx))))
                    .child(Button::new("export-cancel").small().label("Cancel").disabled(self.busy).on_click(cx.listener(|this,_,_,cx| {this.export_open=false;cx.notify();}))))))
            .child(div().flex_1().min_h_0().flex().gap_4()
                .child(div().w_72().flex_shrink_0().flex().flex_col().gap_2()
                    .child(div().text_sm().truncate().child(format!("Directory: {}",if folder.is_empty() {"."} else {&folder})))
                    .when(self.listing.as_ref().is_some_and(|l|l.entries.is_empty()),|view|view.child("Empty directory"))
                    .when(self.listing.is_none() && self.file.is_some(), |view|view.child("Refresh folder to load its entries."))
                    .child(uniform_list("workspace-files",self.listing.as_ref().map_or(0,|l|l.entries.len()),cx.processor(|this,range:std::ops::Range<usize>,_,cx| {
                        range.map(|index| {
                            let entry=&this.listing.as_ref().expect("rendered listing").entries[index];
                            let path=entry.path.clone();let kind=entry.kind;
                            let label=format!("{} {:?}",match kind {Kind::Directory=>"Folder",Kind::File=>"File",Kind::Symlink=>"Link",Kind::Other=>"Special"},entry.name);
                            div().h_9().min_w_0().child(Button::new(("workspace-file",index)).w_full().small().ghost().label(label)
                                .selected(this.location.file.as_ref() == Some(&path))
                                .disabled(this.busy || !matches!(kind,Kind::Directory|Kind::File))
                                .on_click(cx.listener(move|this,_,window,cx|this.run(if kind==Kind::Directory {Action::List(path.clone())} else {Action::Read(path.clone(), None)},window,cx))))
                        }).collect()
                    })).flex_1().min_h_0().track_scroll(&self.scroll))
                    .child(div().text_xs().child("Links and special files are listed without following them.")))
                .child(div().flex_1().min_w_0().flex().flex_col().gap_2()
                    .when(self.file.is_none(),|view|view.child("Choose a file to preview. UTF-8 text up to 256 KiB; PNG, JPEG and WebP up to 8 MiB / 16 megapixels."))
                    .when_some(self.file.as_ref(),|view,file| {
                        let reload=file.path.clone();
                        let copy_path=file.path.clone();
                        let view=view.child(div().text_sm().truncate().child(format!("{:?} · {} bytes",file.path,file.size)))
                            .child(div().flex().flex_wrap().items_center().gap_2()
                            .child(Button::new("export-file-copy").small().label("Save a copy…").disabled(self.busy || file.size > fleetpane_session::files::MAX_EXPORT_BYTES).on_click(cx.listener(|this,_,window,cx| {
                                this.export_open=true;this.export_result=None;
                                this.export_path.update(cx,|input,cx|input.focus(window,cx));cx.notify();
                            })))
                            .child(Button::new("reload-workspace-file").small().label("Reload file").disabled(self.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Read(reload.clone(), this.line),window,cx))))
                            .child(Button::new("copy-workspace-path").small().label("Copy path").on_click(move|_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(copy_path.clone())))));
                        match &file.content {
                            Content::Text {text}=> {
                                let full=text.clone();
                                view.child(div().flex().flex_wrap().items_center().gap_2()
                                    .when(markdown_file(&file.path),|view|view.children([TextMode::Source,TextMode::Preview].map(|mode| {
                                        Button::new(if mode == TextMode::Source {"workspace-file-source"} else {"workspace-file-preview"})
                                            .small().label(if mode == TextMode::Source {"Source"} else {"Preview"})
                                            .selected(self.text_mode == mode).disabled(self.busy)
                                            .on_click(cx.listener(move|this,_,_,cx|this.set_text_mode(mode,cx)))
                                    })))
                                    .child(Button::new("find-workspace-file").small().label("Find")
                                        .tooltip("Find in source · Ctrl/Cmd+F in the editor")
                                        .disabled(self.busy || self.preview.is_none())
                                        .on_click(cx.listener(|this,_,window,cx| {
                                            this.set_text_mode(TextMode::Source,cx);
                                            if let Some(preview) = &this.preview {
                                                preview.update(cx,|state,cx| {
                                                    state.focus(window,cx);
                                                    state.open_search(false,cx);
                                                });
                                            }
                                        })))
                                    .child(Button::new("copy-workspace-file").small().label("Copy full text").on_click(move|_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(full.clone()))))
                                    .child(Button::new("wrap-workspace-file").small().label(if self.wrap {"Wrap: on"} else {"Wrap: off"}).selected(self.wrap).disabled(self.busy || self.preview.is_none() || self.text_mode != TextMode::Source).on_click(cx.listener(|this,_,window,cx| {
                                        this.wrap = !this.wrap;
                                        if let Some(preview) = &this.preview {
                                            preview.update(cx,|state,cx|state.set_soft_wrap(this.wrap,window,cx));
                                        }
                                        cx.notify();
                                    }))))
                                    .when(text.is_empty(),|view|view.child("Empty file"))
                            }
                            Content::Image {image, ..}=>view.child(format!("{} · {} × {} pixels", image.mime_type, image.width, image.height)),
                            Content::Unavailable {reason}=>view.child(div().text_color(cx.theme().danger).child(format!("Image preview unavailable: {reason}"))),
                            Content::Binary=>view.child("Binary or non-UTF-8 file; text preview is unavailable."),
                            Content::TooLarge {limit}=>view.child(format!("File exceeds the {limit}-byte preview limit; no partial content was loaded.")),
                        }
                    })
                    .child(self.render_image_navigation(cx))
                    .when_some(self.image.as_ref(), |view,source|view.child(
                        div().relative().flex_1().min_h_0().overflow_hidden().child(img(ImageSource::from(source.clone())).absolute().top_0().left_0().size_full().object_fit(ObjectFit::Contain)
                            .with_loading(||div().child("Loading image…").into_any_element())
                            .with_fallback(||div().child("Image renderer could not display the validated file.").into_any_element()))))
                    .when(self.text_mode == TextMode::Preview,|view|view.child(div().text_sm().child("Markdown preview · images disabled (placeholders) · source line anchors require Source. Heading anchors are unsupported.")))
                    .when_some(self.line.filter(|_|self.preview.is_some() && self.text_mode == TextMode::Source),|view,line|view.child(div().text_sm().child(format!("Requested source line {line} · complete loaded file"))))
                    .child(self.render_text(cx))))
    }
}

#[cfg(test)]
mod tests {
    use super::{Link, Target, markdown_file, markdown_link};
    use std::{num::NonZeroUsize, path::Path};

    #[test]
    fn markdown_extension_is_explicit_and_case_insensitive() {
        for path in [
            "README.md",
            "doc.MD",
            "nested/中文.MarkDown",
            "empty.markdown",
        ] {
            assert!(markdown_file(path), "{path}");
            assert_eq!(super::language(path), "markdown");
        }
        for path in ["README", ".md", "doc.mdx", "doc.md.rs", "md/doc.txt"] {
            assert!(!markdown_file(path), "{path}");
        }
    }

    #[test]
    fn markdown_links_use_displayed_file_directory_and_shared_decoding() {
        let root = Path::new("/workspace 中文");
        assert_eq!(
            markdown_link(root, "docs/README.MD", "sibling%20中文.rs#L2").unwrap(),
            Link::Local(Target {
                path: "/workspace 中文/docs/sibling 中文.rs".into(),
                line: NonZeroUsize::new(2),
            })
        );
        assert_eq!(
            markdown_link(root, "docs/guide.MD", "子目录/file%20name.rs#L2-L4").unwrap(),
            Link::Local(Target {
                path: "/workspace 中文/docs/子目录/file name.rs".into(),
                line: NonZeroUsize::new(2),
            })
        );
        assert_eq!(
            markdown_link(root, "README.md", "exact%25%23%3A2.txt").unwrap(),
            Link::Local(Target {
                path: "/workspace 中文/exact%#:2.txt".into(),
                line: None,
            })
        );
        assert_eq!(
            markdown_link(
                root,
                "docs/guide.md",
                "file:///workspace%20中文/src/main.rs:7"
            )
            .unwrap(),
            Link::Local(Target {
                path: "/workspace 中文/src/main.rs".into(),
                line: NonZeroUsize::new(7),
            })
        );
        assert_eq!(
            markdown_link(root, "docs/guide.md", "https://example.test/path#section").unwrap(),
            Link::Web("https://example.test/path#section".into())
        );
    }

    #[test]
    fn markdown_links_reject_bad_schemes_and_anchors_and_keep_engine_confinement() {
        let root = Path::new("/workspace");
        for href in [
            "javascript:alert(1)",
            "data:text/html,bad",
            "mailto:user@example.test",
            "file://remote/path.md",
            "//remote/path.md",
            "file.md#heading",
            "file.md#L0",
            "file.md#L4-L2",
            "#heading",
            "#L2",
            "bad%00.md",
            "bad%FF.md",
        ] {
            assert!(
                markdown_link(root, "docs/guide.md", href).is_err(),
                "{href}"
            );
        }
        // Never erase parent components or rebase absolute escapes: the shared
        // open_path engine must receive these unmodified and refuse them.
        for href in ["../outside.md", "%2e%2e/outside.md"] {
            assert_eq!(
                markdown_link(root, "docs/guide.md", href).unwrap(),
                Link::Local(Target {
                    path: "/workspace/docs/../outside.md".into(),
                    line: None,
                })
            );
        }
        assert_eq!(
            markdown_link(root, "docs/guide.md", "/outside.md").unwrap(),
            Link::Local(Target {
                path: "/outside.md".into(),
                line: None,
            })
        );
    }

    #[test]
    fn source_position_keeps_original_lines_and_rejects_absent_targets() {
        assert_eq!(
            super::source_position("first\r\n中文  \n", NonZeroUsize::new(2)).unwrap(),
            super::Position::new(1, 0)
        );
        assert_eq!(
            super::source_position("first\n", NonZeroUsize::new(2)).unwrap(),
            super::Position::new(1, 0)
        );
        assert!(super::source_position("first\n", NonZeroUsize::new(3)).is_err());
        assert!(super::source_position("x", NonZeroUsize::new(usize::MAX)).is_err());
        assert_eq!(
            super::source_position("", None).unwrap(),
            super::Position::new(0, 0)
        );
    }
}
