use super::*;

impl PaseoImportView {
    pub(super) fn scratch_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div().flex().flex_col().gap_2()
            .child(div().flex().gap_2().items_center()
                .child(div().text_lg().child("Temporary migration files"))
                .child(Button::new("paseo-scratch-status").small().label("Check temporary files").disabled(self.busy).on_click(cx.listener(|this,_,window,cx|this.run(Action::Scratch(false),window,cx))))
                .child(Button::new("paseo-clean-scratch").small().label("Clear unused temporary files").disabled(self.busy||!self.scratch.as_ref().is_some_and(|r|!r.removed&&r.files>0)).on_click(cx.listener(|this,_,window,cx|this.run(Action::Scratch(true),window,cx)))))
            .child("Copies and context exports must finish before checking or clearing. Published histories, source logs and migration reports are kept.")
            .when_some(self.scratch.as_ref(),|pane,report|pane
                .child(format!("{}: {} files · {} bytes · {} older files retained",if report.removed{"Cleared unused temporary files"}else{"Unused temporary files"},report.files,report.bytes,report.legacy_files_retained)))
            .child("Older temporary files cannot be checked against active copies and are retained.")
    }
}
