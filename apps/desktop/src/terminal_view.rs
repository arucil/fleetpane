//! Native PTY console. Shared commands own I/O; closing the view only detaches.
mod keyboard;
gpui_kit::actions!(fleetpane_terminal, [ForwardTab, ReverseTab]);

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", ForwardTab, Some("FleetpaneTerminal")),
        KeyBinding::new("shift-tab", ReverseTab, Some("FleetpaneTerminal")),
    ]);
}
use crate::host_view::background;
use fleetpane_runtime::Snapshot;
use fleetpane_session::{
    client::{Connection, Error, completed, snapshot},
    terminals::{
        Command, Info, Mutation, Output, Read,
        screen::{Color, Grid, Screen},
    },
    workspaces::Workspace,
};
use gpui_kit::component::{
    ActiveTheme, Disableable, Selectable, Sizable,
    button::{Button, ButtonVariants},
    input::{Input, InputState, Textarea, TextareaState},
};
use gpui_kit::{prelude::*, *};
use std::{collections::BTreeMap, time::Duration};
use uuid::Uuid;

#[derive(Clone)]
struct Pending {
    id: Uuid,
    host: Uuid,
    target: Option<Uuid>,
    draft: Option<String>,
    keyboard: bool,
}

enum Action {
    Refresh,
    Claim,
    Create(String),
    Mutate(Mutation),
    Send(Mutation, String),
    Keyboard(Mutation),
    Check(Pending),
}
#[derive(Default)]
struct Actions {
    reading: bool,
    busy: bool,
    queued: Option<Action>,
}
impl Actions {
    fn submit(&mut self, action: Action) -> Option<Action> {
        let read = matches!(action, Action::Refresh);
        if self.busy || (read && self.reading) {
            return None;
        }
        if read {
            self.reading = true;
        } else {
            self.busy = true;
            if self.reading {
                self.queued = Some(action);
                return None;
            }
        }
        Some(action)
    }
    fn complete(&mut self, read: bool) -> Option<Action> {
        if read {
            self.reading = false;
        } else {
            self.busy = false;
        }
        let queued = self.queued.take();
        if queued.is_some() {
            self.busy = false;
        }
        queued
    }
}

enum Reply {
    Refreshed(Box<Snapshot>, Vec<Info>, Option<Output>),
    Claimed(Connection),
    Changed(Info),
}

pub struct TerminalView {
    workspace: Workspace,
    connection: Option<Connection>,
    shared_control: Option<crate::host_view::workspace::Control>,
    host: Option<Snapshot>,
    terminals: Vec<Info>,
    selected: Option<Uuid>,
    screen: Option<Screen>,
    label: Entity<InputState>,
    input: Entity<TextareaState>,
    actions: Actions,
    drafts: BTreeMap<Uuid, String>,
    pending: Option<Pending>,
    error: Option<String>,
    polling: bool,
    view_visible: bool,
    restore_selected: Option<(Uuid, Uuid)>,
    keyboard: keyboard::Keyboard,
}
impl EventEmitter<()> for TerminalView {}

impl TerminalView {
    pub(crate) fn tab_shortcut_blocked(&self, window: &mut Window, cx: &mut App) -> bool {
        crate::input_composition::active(&self.input, window, cx)
            || crate::input_composition::active(&self.label, window, cx)
    }

    pub(crate) fn focus_input(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    pub(crate) fn socket(&self) -> &std::path::Path {
        &self.workspace.registration.endpoint.socket
    }
    pub(crate) fn share_control(&mut self, control: crate::host_view::workspace::Control) {
        self.connection = control.borrow().clone();
        self.shared_control = Some(control);
    }

    pub(crate) fn tab_close_status(&self) -> Result<bool, String> {
        Ok(self.close_status()? && !self.actions.busy && self.pending.is_none())
    }
    pub(crate) fn saved_selection(&self) -> Option<(Uuid, Uuid)> {
        self.host
            .as_ref()
            .and_then(|host| self.selected.map(|id| (host.host_id, id)))
            .or(self.restore_selected)
    }
    pub(crate) fn restore_selection(&mut self, selected: Option<(Uuid, Uuid)>) {
        self.restore_selected = selected;
    }
    pub(crate) fn set_visible(&mut self, visible: bool) {
        if let Some(control) = &self.shared_control {
            self.connection = control.borrow().clone();
        }
        self.view_visible = visible;
    }

    pub fn new(workspace: Workspace, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let label = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Terminal name");
            input.set_value("shell", window, cx);
            input
        });
        let input = cx.new(|cx| {
            TextareaState::new(window, cx).placeholder(
                "Exact text · Enter here adds a line · Send text does not append Enter",
            )
        });
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                if this
                    .update_in(cx, |this, window, cx| {
                        if this.polling && this.view_visible {
                            this.run(Action::Refresh, window, cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let mut view = Self {
            workspace,
            connection: None,
            shared_control: None,
            host: None,
            terminals: vec![],
            selected: None,
            screen: None,
            label,
            input,
            actions: Actions::default(),
            drafts: BTreeMap::new(),
            pending: None,
            error: None,
            polling: true,
            view_visible: true,
            restore_selected: None,
            keyboard: keyboard::Keyboard::new(cx),
        };
        view.run(Action::Refresh, window, cx);
        view
    }

    fn run(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(control) = &self.shared_control {
            self.connection = control.borrow().clone();
        }
        let mutation = matches!(
            action,
            Action::Create(_) | Action::Mutate(_) | Action::Send(_, _) | Action::Keyboard(_)
        );
        if mutation && (self.connection.is_none() || self.pending.is_some()) {
            return;
        }
        let Some(action) = self.actions.submit(action) else {
            cx.notify();
            return;
        };
        let read = matches!(action, Action::Refresh);
        if !read {
            self.error = None;
            self.polling = true;
        }
        let workspace = self.workspace.clone();
        let connection = self.connection.clone();
        let selected = self.selected;
        let after = self.screen.as_ref().map_or(0, |screen| screen.cursor());
        let operation = Uuid::new_v4();
        let pending = match &action {
            Action::Create(_) => connection.as_ref().map(|c| Pending {
                id: operation,
                host: c.lease.host_id,
                target: None,
                draft: None,
                keyboard: false,
            }),
            Action::Mutate(_) | Action::Send(_, _) | Action::Keyboard(_) => {
                connection.as_ref().map(|c| Pending {
                    id: operation,
                    host: c.lease.host_id,
                    target: selected,
                    draft: match &action {
                        Action::Send(_, draft) => Some(draft.clone()),
                        _ => None,
                    },
                    keyboard: matches!(action, Action::Keyboard(_)),
                })
            }
            Action::Check(pending) => Some(pending.clone()),
            _ => None,
        };
        let expected = pending.clone();
        let task = cx.background_executor().spawn(async move {
            background(async move {
                match action {
                    Action::Refresh => {
                        let state = snapshot(&workspace.registration.endpoint.socket).await?;
                        if state
                            .provider_info
                            .get("fleetpaneTerminalVersion")
                            .and_then(serde_json::Value::as_u64)
                            != Some(1)
                        {
                            return Err(Error::Input(
                                "This Host does not advertise terminal support.".into(),
                            ));
                        }
                        if let Some(connection) = &connection
                            && connection.lease.host_id != state.host_id
                        {
                            return Ok(Reply::Refreshed(Box::new(state), vec![], None));
                        }
                        let Some(lease) = state.owner.clone() else {
                            return Ok(Reply::Refreshed(Box::new(state), vec![], None));
                        };
                        // Observational reads use the current lease without a claim.
                        let read = Connection {
                            socket: workspace.registration.endpoint.socket.clone(),
                            lease,
                        };
                        let result = read
                            .execute_terminal(
                                &Command::Read(Read::List {
                                    workspace_id: workspace.id,
                                }),
                                operation,
                            )
                            .await?;
                        let infos: Vec<Info> = serde_json::from_value(result.result)
                            .map_err(|e| Error::Protocol(e.to_string()))?;
                        let output = if let Some(id) =
                            selected.filter(|id| infos.iter().any(|info| info.id == *id))
                        {
                            let result = read
                                .execute_terminal(
                                    &Command::Read(Read::Output {
                                        workspace_id: workspace.id,
                                        id,
                                        after,
                                    }),
                                    operation,
                                )
                                .await?;
                            Some(
                                serde_json::from_value(result.result)
                                    .map_err(|e| Error::Protocol(e.to_string()))?,
                            )
                        } else {
                            None
                        };
                        Ok(Reply::Refreshed(Box::new(state), infos, output))
                    }
                    Action::Claim => {
                        let state = snapshot(&workspace.registration.endpoint.socket).await?;
                        Ok(Reply::Claimed(
                            Connection::claim(
                                workspace.registration.endpoint.socket.clone(),
                                &state,
                            )
                            .await?,
                        ))
                    }
                    Action::Create(label) => {
                        let c = connection
                            .as_ref()
                            .expect("mutation requires explicit ownership");
                        let result = workspace.create_terminal(c, operation, label).await?;
                        decode_info(
                            result.result,
                            &workspace,
                            expected.as_ref().expect("mutation has receipt"),
                        )
                    }
                    Action::Mutate(command)
                    | Action::Send(command, _)
                    | Action::Keyboard(command) => {
                        let c = connection
                            .as_ref()
                            .expect("mutation requires explicit ownership");
                        let result = c
                            .execute_terminal(&Command::Mutate(command), operation)
                            .await?;
                        decode_info(
                            result.result,
                            &workspace,
                            expected.as_ref().expect("mutation has receipt"),
                        )
                    }
                    Action::Check(pending) => {
                        let state = snapshot(&workspace.registration.endpoint.socket).await?;
                        if state.host_id != pending.host {
                            return Err(Error::Uncertain {
                                id: pending.id,
                                reason:
                                    "Host changed; receipt is historical. No command was resent."
                                        .into(),
                            });
                        }
                        let lease = state.owner.ok_or_else(|| {
                            Error::Input(
                                "No Host controller; take control to inspect the receipt.".into(),
                            )
                        })?;
                        let c = Connection {
                            socket: workspace.registration.endpoint.socket.clone(),
                            lease,
                        };
                        let result = completed(c.operation(pending.id).await?)?;
                        decode_info(result.result, &workspace, &pending)
                    }
                }
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let reply = task.await;
            let _closed = this.update_in(cx, |this, window, cx| {
                let queued = this.actions.complete(read);
                match reply {
                    Ok(Reply::Refreshed(state, infos, output)) => {
                        if this
                            .connection
                            .as_ref()
                            .is_some_and(|c| state.owner.as_ref() != Some(&c.lease))
                        {
                            this.connection = None;
                            if !this.keyboard.queue.empty() { this.keyboard.queue.pause(); }
                        }
                        let replaced = this.host.as_ref().is_some_and(|old| old.host_id != state.host_id);
                        if replaced {
                            this.keyboard.enabled = false;
                            this.connection = None;
                            this.selected = None;
                            this.screen = None;
                            this.drafts.clear();
                            this.input.update(cx, |input, cx| input.set_value("", window, cx));
                            this.error = Some("Host incarnation changed. Previous terminal receipts remain historical; no command was replayed.".into());
                            this.polling = false;
                        }
                        this.host = Some(*state);
                        this.terminals = infos;
                        if let Some((host,id))=this.restore_selected.take() {
                            if this.host.as_ref().is_some_and(|state|state.host_id==host) && this.terminals.iter().any(|info|info.id==id) {
                                this.select(id,window,cx);
                            } else {
                                this.error=Some("Saved terminal is absent or its Host was replaced. Choose an existing terminal explicitly.".into());
                            }
                        }
                        if this.selected == selected && let Some(output) = output {
                            let result = if let Some(screen) = &mut this.screen {
                                screen.apply(output)
                            } else {
                                Screen::new(output.terminal.clone()).and_then(|mut screen| {
                                    screen.apply(output)?;
                                    this.screen = Some(screen);
                                    Ok(())
                                })
                            };
                            if let Err(error) = result {
                                this.error = Some(error);
                                this.polling = false;
                            }
                        } else if this.selected == selected && this.selected.is_some() {
                            this.selected = None;
                            this.screen = None;
                        }
                    }
                    Ok(Reply::Claimed(connection)) => {
                        if let Some(control)=&this.shared_control {*control.borrow_mut()=Some(connection.clone());}
                        this.connection=Some(connection);
                    },
                    Ok(Reply::Changed(info)) => {
                        this.pending = None;
                        if pending.as_ref().is_some_and(|p|p.keyboard) {
                            this.keyboard.queue.acknowledge();
                        }
                        if pending.as_ref().and_then(|p| p.draft.as_deref()).is_some_and(|draft| this.input.read(cx).value().as_ref() == draft) {
                            this.input
                                .update(cx, |input, cx| input.set_value("", window, cx));
                        }
                        this.select(info.id, window, cx);
                        if this.screen.as_ref().is_none_or(|s| s.info.id != info.id) {
                            match Screen::new(info) {
                                Ok(screen) => this.screen = Some(screen),
                                Err(error) => this.error = Some(error),
                            }
                        } else if let Some(screen) = &mut this.screen {
                            screen.info = info;
                        }
                    }
                    Err(error) => {
                        if pending.as_ref().is_some_and(|p|p.keyboard) || !this.keyboard.queue.empty() {
                            this.keyboard.queue.pause();
                        }
                        if matches!(
                            error,
                            Error::Pending(_) | Error::Uncertain { .. } | Error::Historical(_)
                        ) {
                            this.pending = pending;
                        } else if pending.is_some() {
                            this.pending = None;
                        }
                        this.error = Some(error.to_string());
                        this.polling = false;
                    }
                }
                if let Some(action) = queued { this.run(action, window, cx); }
                this.pump_keyboard(window,cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn select(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected == Some(id) {
            return;
        }
        if let Some(old) = self.selected {
            self.drafts
                .insert(old, self.input.read(cx).value().to_string());
        }
        self.selected = Some(id);
        self.screen = None;
        let draft = self.drafts.get(&id).cloned().unwrap_or_default();
        self.input
            .update(cx, |input, cx| input.set_value(draft, window, cx));
    }

    fn send(
        &mut self,
        mut bytes: Vec<u8>,
        enter: bool,
        clear_draft: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(screen) = &self.screen else {
            return;
        };
        if enter {
            bytes.push(13);
        }
        if bytes.is_empty() {
            return;
        }
        let command = Mutation::Input {
            workspace_id: self.workspace.id,
            id: screen.info.id,
            expected_input: screen.info.next_input,
            bytes,
        };
        let action = if clear_draft {
            Action::Send(command, self.input.read(cx).value().to_string())
        } else {
            Action::Mutate(command)
        };
        self.run(action, window, cx);
    }

    fn can_write(&self) -> bool {
        self.connection.is_some()
            && !self.actions.busy
            && self.pending.is_none()
            && self.keyboard.queue.empty()
    }
}

fn decode_info(
    value: serde_json::Value,
    workspace: &Workspace,
    pending: &Pending,
) -> Result<Reply, Error> {
    let info: Info = serde_json::from_value(value).map_err(|e| Error::Uncertain {
        id: pending.id,
        reason: format!("invalid terminal acknowledgement: {e}"),
    })?;
    if info.workspace_id != workspace.id || pending.target.is_some_and(|id| id != info.id) {
        return Err(Error::Uncertain {
            id: pending.id,
            reason: "terminal acknowledgement has a different target".into(),
        });
    }
    Ok(Reply::Changed(info))
}

fn status(info: &Info) -> String {
    let mut label = if info.closed {
        "closed"
    } else if info.exit.is_some() {
        "exited"
    } else {
        "running"
    }
    .to_string();
    if let Some(exit) = &info.exit {
        if let Some(code) = exit.code {
            label.push_str(&format!(" · code {code}"));
        } else if let Some(signal) = exit.signal {
            label.push_str(&format!(" · signal {signal}"));
        } else {
            label.push_str(" · status unavailable");
        }
        if !info.output_eof {
            label.push_str(" · output still open");
        }
    }
    label
}

fn color(color: Color, default: u32) -> Hsla {
    let value = match color {
        Color::Default => default,
        Color::Rgb(r, g, b) => (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
        Color::Idx(i) => {
            const ANSI: [u32; 16] = [
                0x171a20, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xabb2bf,
                0x5c6370, 0xff7a85, 0xb5e890, 0xffd68a, 0x82c8ff, 0xe3a0ff, 0x7ce3eb, 0xffffff,
            ];
            match i {
                0..=15 => ANSI[usize::from(i)],
                16..=231 => {
                    let i = u32::from(i - 16);
                    let component = |n| if n == 0 { 0 } else { 55 + 40 * n };
                    (component(i / 36) << 16) | (component((i / 6) % 6) << 8) | component(i % 6)
                }
                _ => {
                    let level = 8 + 10 * u32::from(i - 232);
                    (level << 16) | (level << 8) | level
                }
            }
        }
    };
    rgb(value).into()
}

fn rows(grid: &Grid, family: SharedString) -> Vec<Div> {
    let (height, width) = grid.size();
    let cursor = grid.cursor_position();
    (0..height.get())
        .map(|row| {
            let mut text = String::new();
            let mut runs: Vec<TextRun> = vec![];
            for col in 0..width.get() {
                let cell = grid
                    .cell(row, col)
                    .expect("cell within validated dimensions");
                if cell.is_wide_continuation() {
                    continue;
                }
                let contents = if cell.has_contents() {
                    cell.contents()
                } else {
                    " "
                };
                text.push_str(contents);
                let mut fg = color(cell.fgcolor(), 0xdce2ec);
                let mut bg = color(cell.bgcolor(), 0x171a20);
                if cell.inverse()
                    ^ (!grid.hide_cursor() && grid.scrollback() == 0 && cursor == (row, col))
                {
                    std::mem::swap(&mut fg, &mut bg);
                }
                if cell.dim() {
                    fg.a *= 0.65;
                }
                let mut font = font(family.clone());
                if cell.bold() {
                    font.weight = FontWeight::BOLD;
                }
                if cell.italic() {
                    font.style = FontStyle::Italic;
                }
                let run = TextRun {
                    len: contents.len(),
                    font,
                    color: fg,
                    background_color: Some(bg),
                    underline: cell.underline().then_some(UnderlineStyle {
                        thickness: px(1.),
                        color: Some(fg),
                        wavy: false,
                    }),
                    strikethrough: None,
                };
                if let Some(last) = runs.last_mut().filter(|last| {
                    last.font == run.font
                        && last.color == run.color
                        && last.background_color == run.background_color
                        && last.underline == run.underline
                }) {
                    last.len += run.len;
                } else {
                    runs.push(run);
                }
            }
            div()
                .h(px(18.))
                .whitespace_nowrap()
                .child(StyledText::new(text).with_runs(runs))
        })
        .collect()
}

impl Render for TerminalView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let writable = self.can_write();
        let info = self.screen.as_ref().map(|s| &s.info);
        let input_ready = writable
            && info.is_some_and(|i| !i.closed && i.exit.is_none() && i.input_fault.is_none());
        let id = self.selected;
        let wid = self.workspace.id;
        div().size_full().flex().flex_col().gap_2().p_4()
            .child(div().flex().items_center().gap_2()
                .child(Button::new("terminal-back").small().label("Back to workspaces").disabled(!self.can_detach()).on_click(cx.listener(|_,_,_,cx|cx.emit(()))))
                .child(div().text_lg().child("Workspace terminals"))
                .child(Button::new("terminal-claim").small().label(if self.connection.is_some(){"Control held"}else{"Take control"}).disabled(self.actions.busy || self.connection.is_some()).on_click(cx.listener(|this,_,window,cx|this.run(Action::Claim,window,cx))))
                .child(Button::new("terminal-refresh").small().label("Refresh").disabled(self.actions.busy).on_click(cx.listener(|this,_,window,cx|{this.error=None;this.polling=true;this.run(Action::Refresh,window,cx);}))))
            .child(div().text_sm().child(format!("{} · {}",self.workspace.registration.endpoint.name,self.workspace.registration.directory.path.display())))
            .child(div().text_sm().child("Take control claims this entire Host; other windows become read-only until they reclaim control."))
            .children(self.host.as_ref().and_then(|h|h.fault.as_ref()).map(|fault|div().text_sm().text_color(cx.theme().danger).child(format!("Host reports: {fault}"))))
            .children(self.error.iter().map(|e|div().text_sm().text_color(cx.theme().danger).child(e.clone())))
            .when_some(self.pending.as_ref(),|view,p| {
                let pending=p.clone();
                view.child(div().text_sm().child(format!("Operation {} · do not resend",p.id)))
                    .child(Button::new("terminal-check-receipt").small().label("Check operation").disabled(self.actions.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Check(pending.clone()),window,cx))))
            })
            .child(div().flex_1().min_h_0().flex().gap_3()
                .child(div().w(px(180.)).flex_shrink_0().flex().flex_col().gap_2()
                    .child(Input::new(&self.label).disabled(self.actions.busy))
                    .child(Button::new("terminal-create").small().label("New terminal").disabled(!writable).on_click(cx.listener(|this,_,window,cx|{let label=this.label.read(cx).value().to_string();this.run(Action::Create(label),window,cx);})))
                    .child(div().id("terminal-list").flex_1().min_h_0().overflow_y_scroll().children(self.terminals.iter().enumerate().map(|(index,info)| {
                        let terminal=info.clone();let tid=info.id;
                        Button::new(("terminal-choice",index)).small().ghost().w_full().label(format!("{} · {}",info.label,status(info))).selected(id==Some(tid)).disabled(self.actions.busy || self.pending.is_some() || !self.keyboard.queue.empty()).on_click(cx.listener(move|this,_,window,cx|{this.keyboard.enabled=false;this.select(tid,window,cx);this.label.update(cx,|label,cx|label.set_value(terminal.label.clone(),window,cx));this.run(Action::Refresh,window,cx);}))
                    }))))
                .child(div().flex_1().min_w_0().flex().flex_col().gap_2()
                    .when_some(info,|view,info| {
                        let id=info.id;
                        view.child(div().text_sm().child(format!("{} · PID {} · {}×{} · input {} · {}",info.label,info.pid,info.cols,info.rows,info.next_input,status(info))))
                            .child(div().flex().gap_2()
                                .child(Button::new("terminal-rename").small().label("Rename").disabled(!writable).on_click(cx.listener(move|this,_,window,cx|{let label=this.label.read(cx).value().to_string();this.run(Action::Mutate(Mutation::Rename{workspace_id:wid,id,label}),window,cx);})))
                                .child(Button::new("terminal-size-80").small().label("80×24").disabled(!writable).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Mutate(Mutation::Resize{workspace_id:wid,id,rows:24,cols:80}),window,cx))))
                                .child(Button::new("terminal-size-100").small().label("100×30").disabled(!writable).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Mutate(Mutation::Resize{workspace_id:wid,id,rows:30,cols:100}),window,cx))))
                                .child(Button::new("terminal-close").small().label("Close terminal").disabled(!writable).on_click(cx.listener(move|this,_,window,cx|this.run(Action::Mutate(Mutation::Close{workspace_id:wid,id}),window,cx)))))
                            .children(info.input_fault.iter().chain(info.output_fault.iter()).map(|e|div().text_sm().text_color(cx.theme().danger).child(e.clone())))
                    })
                    .when_some(self.screen.as_ref(),|view,screen| {
                        view.when(screen.lost()>0,|view|view.child(div().text_sm().text_color(cx.theme().danger).child(format!("{} output bytes lost; this reconstructed screen is incomplete.",screen.lost()))))
                            .child(div().id("terminal-screen").track_focus(&self.keyboard.focus).when(self.keyboard.enabled,|view|view.key_context("FleetpaneTerminal"))
                                .on_action(cx.listener(|this,_:&ForwardTab,window,cx|this.keyboard_tab(false,window,cx)))
                                .on_action(cx.listener(|this,_:&ReverseTab,window,cx|this.keyboard_tab(true,window,cx)))
                                .on_key_down(cx.listener(Self::keyboard_key)).on_click(cx.listener(|this,_,window,cx|{if this.keyboard.enabled {window.focus(&this.keyboard.focus,cx);}})).flex_1().min_h_0().overflow_scroll().p_2().bg(rgb(0x171a20)).text_size(px(14.)).font_family(cx.theme().mono_font_family.clone())
                                .child(div().w(px(f32::from(screen.info.cols)*9.)).flex().flex_col().children(rows(screen.grid(),cx.theme().mono_font_family.clone()))))
                            .child(div().flex().gap_2()
                                .child(Button::new("terminal-scroll-up").small().label("Page up").on_click(cx.listener(|this,_,_,cx|{if let Some(s)=&mut this.screen{s.scroll(20);}cx.notify();})))
                                .child(Button::new("terminal-scroll-down").small().label("Page down").on_click(cx.listener(|this,_,_,cx|{if let Some(s)=&mut this.screen{s.scroll(-20);}cx.notify();})))
                                .child(Button::new("terminal-live").small().label("Live output").on_click(cx.listener(|this,_,_,cx|{if let Some(s)=&mut this.screen{s.live();}cx.notify();})))
                                .child(Button::new("terminal-copy-screen").small().label("Copy screen").on_click(cx.listener(|this,_,_,cx|{if let Some(s)=&this.screen{cx.write_to_clipboard(ClipboardItem::new_string(s.grid().contents()));}}))))
                    })
                    .when(self.screen.is_none(),|view|view.child(div().flex_1().child("Select a terminal, or take control and create one. Closing this panel keeps shells running.")))
                    .child(self.render_keyboard(cx))
                    .child(Textarea::new(&self.input).h(px(64.)).disabled(!input_ready))
                    .child(div().flex().gap_2()
                        .child(Button::new("terminal-send-text").small().label("Send text").disabled(!input_ready).on_click(cx.listener(|this,_,window,cx|{let text=this.input.read(cx).value().as_bytes().to_vec();this.send(text,false,true,window,cx);})))
                        .child(Button::new("terminal-run").small().label("Send + Enter").disabled(!input_ready).on_click(cx.listener(|this,_,window,cx|{let text=this.input.read(cx).value().as_bytes().to_vec();this.send(text,true,true,window,cx);})))
                        .child(Button::new("terminal-enter").small().label("Enter").disabled(!input_ready).on_click(cx.listener(|this,_,window,cx|this.send(vec![13],false,false,window,cx))))
                        .child(Button::new("terminal-interrupt").small().label("Ctrl-C").disabled(!input_ready).on_click(cx.listener(|this,_,window,cx|this.send(vec![3],false,false,window,cx))))
                        .child(Button::new("terminal-eof").small().label("Ctrl-D").disabled(!input_ready).on_click(cx.listener(|this,_,window,cx|this.send(vec![4],false,false,window,cx))))
                        .child(Button::new("terminal-tab").small().label("Tab").disabled(!input_ready).on_click(cx.listener(|this,_,window,cx|this.send(vec![9],false,false,window,cx)))))
                )
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, Actions};

    #[test]
    fn polling_preserves_one_explicit_action_and_never_repeats_it() {
        let mut actions = Actions::default();
        assert!(matches!(
            actions.submit(Action::Refresh),
            Some(Action::Refresh)
        ));
        assert!(!actions.busy, "reading must not disable user controls");
        assert!(actions.submit(Action::Create("first".into())).is_none());
        assert!(actions.busy);
        assert!(actions.submit(Action::Create("duplicate".into())).is_none());
        assert!(actions.submit(Action::Refresh).is_none());
        let queued = actions.complete(true).expect("the click survives the read");
        assert!(matches!(&queued, Action::Create(label) if label == "first"));
        assert!(matches!(actions.submit(queued), Some(Action::Create(_))));
        assert!(actions.submit(Action::Refresh).is_none());
        assert!(actions.complete(false).is_none());
        assert!(!actions.busy);
        assert!(
            actions.complete(true).is_none(),
            "completed commands are never retried"
        );
    }
}
