use super::*;

impl WorkspaceView {
    pub(super) fn name_editor(&self, cx: &Context<Self>) -> Div {
        div().when_some(self.name_target.as_ref(), |view, workspace| {
            let id = workspace.id;
            let revision = workspace.name.revision;
            view.p_2().border_1().border_color(cx.theme().border)
                .child(div().text_sm().truncate().child(format!("Rename · {} · {}", workspace.registration.endpoint.name, workspace.display_name())))
                .child(div().text_xs().truncate().child(workspace.registration.directory.path.to_string_lossy().into_owned()))
                .child(Input::new(&self.name_input).disabled(self.busy))
                .child(div().flex().flex_wrap().gap_2().mt_1()
                    .child(Button::new("save-workspace-name").small().label("Save name").disabled(self.busy || self.name_error.is_some()).on_click(cx.listener(move |this,_,window,cx| {
                        let value=this.name_input.read(cx).value().to_string();
                        this.run(Action::Rename(id,revision,Some(value)),window,cx);
                    })))
                    .child(Button::new("reset-workspace-name").small().label("Use default name").disabled(self.busy || self.name_error.is_some()).on_click(cx.listener(move |this,_,window,cx| {
                        this.run(Action::Rename(id,revision,None),window,cx);
                    })))
                    .child(Button::new("reload-workspace-name").small().label("Reload name").disabled(self.busy).on_click(cx.listener(move |this,_,window,cx|this.run(Action::ReloadName(id),window,cx))))
                    .child(Button::new("cancel-workspace-name").small().label("Close name editor").disabled(self.busy).on_click(cx.listener(|this,_,_,cx| {
                        this.name_error=None;
                        this.name_target=None;
                        cx.emit(NamesUpdated(vec![]));
                        cx.notify();
                    })))
                )
                .child(div().text_xs().child("Local display name · 1–256 UTF-8 bytes · Reload name reads other windows and discards this edit."))
                .when_some(self.name_error.as_ref(),|view,error|view.child(div().text_sm().text_color(cx.theme().danger).child(error.clone())))
        })
    }
}
