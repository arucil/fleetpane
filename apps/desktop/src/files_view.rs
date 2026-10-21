//! Read-only file navigation; filesystem work stays in the shared session engine.
use crate::{file_locations::Location, host_view::background};
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
use std::num::NonZeroUsize;

enum Action {
    List(String, Option<Target>),
    Read(String, Option<NonZeroUsize>),
    Open(Target),
}
enum Reply {
    Listed(Listing, Option<Target>),
    Read(Readback, Option<NonZeroUsize>),
    Opened(Opened, Option<NonZeroUsize>),
}
pub struct FilesView {
    workspace: Workspace,
    location: Location,
    back_label: &'static str,
    line: Option<NonZeroUsize>,
    path: Entity<InputState>,
    _path_subscription: Subscription,
    listing: Option<Listing>,
    file: Option<Readback>,
    preview: Option<Entity<TextareaState>>,
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

impl FilesView {
    pub fn new(
        workspace: Workspace,
        location: Location,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let action = Action::List(
            location.directory.clone(),
            location
                .file
                .clone()
                .map(|path| Target { path, line: None }),
        );
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
        let mut value = Self {
            workspace,
            location: location.clone(),
            back_label,
            line: None,
            path,
            _path_subscription: subscription,
            listing: None,
            file: None,
            preview: None,
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

    fn show_file(
        &mut self,
        file: Readback,
        line: Option<NonZeroUsize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.location.file = Some(file.path.clone());
        self.line = line;
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
    fn run(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        let workspace = self.workspace.clone();
        let task = cx.background_executor().spawn(async move {
            background(async move {
                match action {
                    Action::List(path, selected) => workspace
                        .list_files(&path)
                        .await
                        .map(|listing| Reply::Listed(listing, selected)),
                    Action::Read(path, line) => workspace
                        .read_file(&path)
                        .await
                        .map(|file| Reply::Read(file, line)),
                    Action::Open(target) => workspace
                        .open_path(&target.path)
                        .await
                        .map(|opened| Reply::Opened(opened, target.line)),
                }
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let reply = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                let mut restore = None;
                match reply {
                    Ok(Reply::Listed(listing, selected)) => {
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
                        this.line = None;
                        this.scroll.scroll_to_item(0, ScrollStrategy::Top);
                        restore = selected;
                    }
                    Ok(Reply::Read(file, line)) => this.show_file(file, line, window, cx),
                    Ok(Reply::Opened(Opened::Directory(listing), _)) => {
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
                        this.line = None;
                        this.scroll.scroll_to_item(0, ScrollStrategy::Top);
                    }
                    Ok(Reply::Opened(Opened::File(file), line)) => {
                        this.location.directory = file
                            .path
                            .rsplit_once('/')
                            .map_or("", |(parent, _)| parent)
                            .to_owned();
                        this.path.update(cx, |input, cx| {
                            input.set_value(file.path.clone(), window, cx)
                        });
                        this.listing = None;
                        this.show_file(file, line, window, cx);
                    }
                    Err(error) => this.error = Some(error.to_string()),
                }
                if let Some(target) = restore {
                    this.run(Action::Read(target.path, target.line), window, cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for FilesView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let folder = self.folder();
        div().size_full().flex().flex_col().gap_3().p_4()
            .child(div().flex().items_center().gap_3()
                .child(Button::new("back-workspaces").small().label(self.back_label).on_click(cx.listener(|this,_,_,cx|cx.emit(this.location.clone()))))
                .child(div().text_lg().child("Workspace files · Read only")))
            .child(div().text_sm().truncate().child(format!("{} · {}",self.workspace.registration.endpoint.name,self.workspace.registration.directory.path.display())))
            .child(div().flex().items_center().gap_2()
                .child(Button::new("files-root").small().label("Root").disabled(self.busy).on_click(cx.listener(|this,_,window,cx|this.run(Action::List(String::new(), None),window,cx))))
                .child(Button::new("files-up").small().label("Up").disabled(self.busy || folder.is_empty()).on_click(cx.listener(|this,_,window,cx| {
                    let folder=this.folder();let parent=folder.rsplit_once('/').map_or("",|(parent,_)|parent).to_owned();this.run(Action::List(parent, None),window,cx);
                })))
                .child(Button::new("files-refresh").small().label("Refresh folder").disabled(self.busy).on_click(cx.listener(|this,_,window,cx|this.run(Action::List(this.folder(), this.location.file.clone().map(|path|Target {path,line:this.line})),window,cx))))
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
                                .on_click(cx.listener(move|this,_,window,cx|this.run(if kind==Kind::Directory {Action::List(path.clone(), None)} else {Action::Read(path.clone(), None)},window,cx))))
                        }).collect()
                    })).flex_1().min_h_0().track_scroll(&self.scroll))
                    .child(div().text_xs().child("Links and special files are listed without following them.")))
                .child(div().flex_1().min_w_0().flex().flex_col().gap_2()
                    .when(self.file.is_none(),|view|view.child("Choose a file to preview. UTF-8 text up to 256 KiB; larger and binary files show an explicit notice."))
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
                            Content::Binary=>view.child("Binary or non-UTF-8 file; text preview is unavailable."),
                            Content::TooLarge {limit}=>view.child(format!("File exceeds the {limit}-byte text limit; no partial content was loaded.")),
                        }
                    })
                    .when_some(self.line,|view,line|view.child(div().text_sm().child(format!("From source line {line}"))))
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
