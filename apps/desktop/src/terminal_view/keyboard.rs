use super::*;
use fleetpane_session::terminals::input::{self, Key, Queue};

pub(super) struct Keyboard {
    pub focus: FocusHandle,
    pub enabled: bool,
    pub queue: Queue,
    target: Option<(Uuid, Uuid)>,
}
impl Keyboard {
    pub fn new(cx: &App) -> Self {
        Self {
            focus: cx.focus_handle(),
            enabled: false,
            queue: Queue::default(),
            target: None,
        }
    }
}
impl TerminalView {
    pub(super) fn keyboard_tab(
        &mut self,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selection.selected.is_some() {
            return;
        }
        self.queue_keyboard(
            if reverse { b"\x1b[Z".to_vec() } else { vec![9] },
            window,
            cx,
        );
    }
    pub(crate) fn close_status(&self) -> Result<bool, String> {
        if self.keyboard.queue.waiting() > 0
            && !self.keyboard.queue.paused()
            && self.keyboard_target_matches()
        {
            return Ok(false);
        }
        if self.keyboard.queue.waiting() > 0
            || (self.keyboard.queue.paused()
                && self.pending.is_none()
                && self.keyboard.queue.in_flight())
        {
            return Err(
                "Terminal has queued keyboard input. Copy or discard queued keys before closing"
                    .into(),
            );
        }
        Ok(!self.keyboard.queue.in_flight() || self.pending.is_some())
    }
    pub(super) fn can_detach(&self) -> bool {
        self.close_status() == Ok(true)
    }
    fn keyboard_target_matches(&self) -> bool {
        self.connection
            .as_ref()
            .is_some_and(|c| self.keyboard.target == self.selected.map(|id| (c.lease.host_id, id)))
    }
    pub(super) fn keyboard_toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.keyboard.enabled {
            self.keyboard.enabled = false;
        } else if self.can_write()
            && self.keyboard.queue.empty()
            && let (Some(c), Some(s)) = (&self.connection, &self.screen)
        {
            self.keyboard.target = Some((c.lease.host_id, s.info.id));
            self.keyboard.enabled = true;
            window.focus(&self.keyboard.focus, cx);
        }
        cx.notify();
    }
    fn queue_keyboard(&mut self, bytes: Vec<u8>, window: &mut Window, cx: &mut Context<Self>) {
        if bytes.is_empty() {
            return;
        }
        let ready = self.keyboard.enabled
            && self.selection.selected.is_none()
            && self.keyboard_target_matches()
            && self.pending.is_none()
            && (!self.actions.busy || self.keyboard.queue.in_flight())
            && self.screen.as_ref().is_some_and(|s| {
                !s.info.closed && s.info.exit.is_none() && s.info.input_fault.is_none()
            });
        if !ready {
            self.error =
                Some("Terminal is not ready for keyboard input; this input was not queued".into());
            cx.notify();
            return;
        }
        match self.keyboard.queue.push(bytes) {
            Ok(()) => self.pump_keyboard(window, cx),
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }
    pub(super) fn pump_keyboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.actions.busy
            || self.pending.is_some()
            || self.keyboard.queue.empty()
            || self.keyboard.queue.paused()
        {
            return;
        }
        if !self.keyboard_target_matches() {
            self.keyboard.queue.pause();
            self.error=Some("Terminal ownership or target changed. Queued keys were preserved and were not sent to another shell".into());
            return;
        }
        let Some(screen) = &mut self.screen else {
            return;
        };
        if screen.info.closed || screen.info.exit.is_some() || screen.info.input_fault.is_some() {
            self.keyboard.queue.pause();
            self.error = Some("Terminal input is closed; queued keys were preserved".into());
            return;
        }
        if let Some(bytes) = self.keyboard.queue.start() {
            screen.live();
            let command = Mutation::Input {
                workspace_id: self.workspace.id,
                id: screen.info.id,
                expected_input: screen.info.next_input,
                bytes,
            };
            self.run(Action::Keyboard(command), window, cx);
        }
    }
    pub(super) fn keyboard_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.keyboard.enabled {
            return;
        }
        let key = &event.keystroke;
        let mods = key.modifiers;
        if mods.control
            && mods.shift
            && !mods.alt
            && !mods.platform
            && key.key.eq_ignore_ascii_case("v")
        {
            cx.stop_propagation();
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                let bracketed = self
                    .screen
                    .as_ref()
                    .is_some_and(|s| s.grid().bracketed_paste());
                match input::paste(&text, bracketed) {
                    Ok(bytes) => self.queue_keyboard(bytes, window, cx),
                    Err(error) => {
                        self.error = Some(error);
                        cx.notify();
                    }
                }
            }
            return;
        }
        if mods.control
            && mods.shift
            && !mods.alt
            && !mods.platform
            && key.key.eq_ignore_ascii_case("c")
        {
            cx.stop_propagation();
            if let Some(screen) = &self.screen {
                cx.write_to_clipboard(ClipboardItem::new_string(screen.grid().contents()));
            }
            return;
        }
        let character = event.prefer_character_input && key.key_char.is_some();
        let bytes = input::encode(
            Key {
                name: &key.key,
                text: key.key_char.as_deref(),
                control: mods.control && !character,
                alt: mods.alt && !character,
                shift: mods.shift,
                platform: mods.platform,
            },
            self.screen
                .as_ref()
                .is_some_and(|s| s.grid().application_cursor()),
        );
        if let Some(bytes) = bytes {
            cx.stop_propagation();
            self.queue_keyboard(bytes, window, cx);
        }
    }
    pub(super) fn render_keyboard(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let enabled = self.keyboard.enabled;
        let paused = self.keyboard.queue.paused();
        div().flex().flex_col().gap_1().text_sm()
            .child(div().flex().gap_2().items_center()
                .child(Button::new("terminal-keyboard").small().label(if enabled{"Stop keyboard input"}else{"Type in terminal"}).disabled(!enabled && (!self.can_write() || !self.keyboard.queue.empty() || self.screen.is_none())).on_click(cx.listener(|this,_,window,cx|this.keyboard_toggle(window,cx))))
                .child(if enabled {"Drag to select · Double-click word · Triple-click line · Ctrl+Shift+V paste"}else{"Drag to select · Double-click word · Triple-click line · Keyboard input is off"}))
            .when(!self.keyboard.queue.empty() || paused,|view|view.child(div().flex().gap_2().items_center()
                .child(format!("{} queued bytes{}{}",self.keyboard.queue.waiting(),if self.keyboard.queue.in_flight(){" · packet awaiting result"}else{""},if paused{" · paused"}else{""}))
                .child(Button::new("terminal-copy-keys").small().label("Copy queued keys").on_click(cx.listener(|this,_,_,cx|{
                    let (flight,waiting)=this.keyboard.queue.bytes();
                    cx.write_to_clipboard(ClipboardItem::new_string(serde_json::json!({"target":this.keyboard.target,"operation":this.pending.as_ref().map(|p|p.id),"inFlightDoNotResend":flight,"waiting":waiting}).to_string()));
                })))
                .child(Button::new("terminal-discard-keys").small().label("Discard queued keys").disabled(self.actions.busy).on_click(cx.listener(|this,_,_,cx|{
                    this.keyboard.queue.discard(this.pending.is_some());this.keyboard.enabled=false;cx.notify();
                })))
                .child(Button::new("terminal-resume-keys").small().label("Resume queued keys").disabled(!paused || self.actions.busy || self.pending.is_some() || self.keyboard.queue.in_flight() || !self.keyboard_target_matches()).on_click(cx.listener(|this,_,window,cx|{
                    match this.keyboard.queue.resume() {Ok(())=>{this.error=None;this.pump_keyboard(window,cx);},Err(error)=>this.error=Some(error)}cx.notify();
                })))
            ))
    }
}
