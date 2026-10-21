//! Read-only file navigation; filesystem work stays in the shared session engine.
use crate::{
    file_locations::{History, Location, Visit},
    host_view::background,
    image_navigation,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use fleetpane_session::{
    files::{Content, Kind, Listing, Opened, Readback, Target},
    workspaces::Workspace,
};
use gpui_kit::component::{
    ActiveTheme, Disableable, Selectable, Sizable,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
};
use gpui_kit::{prelude::*, *};
use std::{num::NonZeroUsize, sync::Arc};

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
    preview: Option<Entity<TextareaState>>,
    image: Option<Arc<Image>>,
    busy: bool,
    error: Option<String>,
    scroll: UniformListScrollHandle,
}
impl EventEmitter<Location> for FilesView {}

const PREVIEW_BYTES: usize = 16 * 1024;
fn preview(text: &str) -> &str {
    let mut end = text.len().min(PREVIEW_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn source_preview(text: &str, line: Option<NonZeroUsize>) -> Result<&str, String> {
    let line = line.unwrap_or(NonZeroUsize::MIN).get();
    let start = if line == 1 {
        0
    } else {
        text.match_indices('\n')
            .nth(line - 2)
            .map(|(index, _)| index + 1)
            .ok_or_else(|| {
                format!(
                    "Requested line {line} does not exist; the file is loaded and can be copied."
                )
            })?
    };
    Ok(preview(&text[start..]))
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
        cx.on_release(|this, cx| this.clear_image(cx)).detach();
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
            image: None,
            busy: false,
            error: None,
            scroll: UniformListScrollHandle::new(),
        };
        value.focus(window, cx);
        value.run(action, window, cx);
        value
    }
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.path.update(cx, |path, cx| path.focus(window, cx));
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

    fn show_file(
        &mut self,
        file: Readback,
        image: Option<Arc<Image>>,
        line: Option<NonZeroUsize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.location.file = Some(file.path.clone());
        self.line = line;
        self.clear_image(cx);
        self.image = image;
        if self.image.is_some() && line.is_some() {
            self.error = Some("Source line anchors apply to text files; showing the image.".into());
        }
        self.preview = match &file.content {
            Content::Text { text } => match source_preview(text, line) {
                Ok(text) => Some(cx.new(|cx| {
                    let mut state = TextareaState::new(window, cx).auto_grow(8, 20);
                    state.set_value(text, window, cx);
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
                .child(Button::new("back-workspaces").small().label(self.back_label).on_click(cx.listener(|this,_,_,cx|cx.emit(this.location.clone()))))
                .child(div().text_lg().child("Workspace files · Read only"))
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
            .when(self.busy,|view|view.child(div().text_sm().child("Reading…")))
            .children(self.error.iter().map(|e|div().text_sm().text_color(cx.theme().danger).child(e.clone())))
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
                            .child(Button::new("reload-workspace-file").small().label("Reload file").disabled(self.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Read(reload.clone(), this.line),window,cx))))
                            .child(Button::new("copy-workspace-path").small().label("Copy path").on_click(move|_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(copy_path.clone()))));
                        match &file.content {
                            Content::Text {text}=> {
                                let full=text.clone();
                                view.child(Button::new("copy-workspace-file").small().label("Copy full text").on_click(move|_,_,cx|cx.write_to_clipboard(ClipboardItem::new_string(full.clone()))))
                                    .when(text.len()>PREVIEW_BYTES,|view|view.child(div().text_sm().child("Preview shows up to 16 KiB from the selected line. Copy full text includes the complete loaded file.")))
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
                    .when_some(self.line.filter(|_|self.preview.is_some()),|view,line|view.child(div().text_sm().child(format!("From source line {line}"))))
                    .when_some(self.preview.as_ref(),|view,state|view.child(div().flex_1().min_h_0().overflow_hidden().child(Textarea::new(state).readonly(true).h_full())))))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn source_line_preview_preserves_newlines_bounds_and_explicit_missing_line() {
        use std::num::NonZeroUsize;
        assert_eq!(
            super::source_preview("first\r\n中文  \n", NonZeroUsize::new(2)).unwrap(),
            "中文  \n"
        );
        assert_eq!(
            super::source_preview("first\n", NonZeroUsize::new(2)).unwrap(),
            ""
        );
        assert!(super::source_preview("first\n", NonZeroUsize::new(3)).is_err());
        let text = format!("skip\n{}", "中".repeat(20_000));
        let shown = super::source_preview(&text, NonZeroUsize::new(2)).unwrap();
        assert!(
            shown.len() <= super::PREVIEW_BYTES && shown.starts_with('中') && shown.ends_with('中')
        );
    }
    #[test]
    fn preview_preserves_utf8_and_caps_display_only() {
        let value = "中".repeat(20_000);
        let shown = super::preview(&value);
        assert!(shown.len() <= super::PREVIEW_BYTES && shown.ends_with('中'));
        assert_eq!(value.len(), 60_000);
        assert_eq!(super::preview("\r\n exact \n"), "\r\n exact \n");
    }
}
