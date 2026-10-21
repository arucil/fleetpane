//! Saved receipts are observation only. Retrying always creates a new attempt.
use super::*;

impl PaseoImportView {
    pub(super) fn saved_batches(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div().flex().flex_col().gap_2()
            .child(div().flex().gap_2().items_center()
                .child(div().text_lg().child("Saved migration attempts"))
                .child(Button::new("reload-paseo-batches").small().label("Newest attempts").disabled(self.busy).on_click(cx.listener(|this,_,window,cx|this.run(Action::Batches(None),window,cx))))
                .child(Button::new("older-paseo-batches").small().label("Older attempts").disabled(self.busy||self.batches.len()<32).on_click(cx.listener(|this,_,window,cx|{let before=this.batches.last().map(|b|b.position);this.run(Action::Batches(before),window,cx)}))))
            .child("Progress is saved after each history. Opening a report never starts copying. A retry checks the original selection again and keeps the prior report.")
            .child(uniform_list("saved-paseo-batches",self.batches.len(),cx.processor(|this,range:std::ops::Range<usize>,_,cx|range.map(|i|{
                let b=&this.batches[i];let id=b.id;
                div().h(px(62.)).flex().items_center().gap_2()
                    .child(div().flex_1().min_w_0().flex().flex_col()
                        .child(div().truncate().child(format!("{} · {} · {} / {} workspaces",b.id,b.state.label(b.writer_active),b.progress.workspaces_done,b.progress.workspaces_total)))
                        .child(div().text_xs().truncate().child(format!("{} → {} · {} histories · {} failures",b.source.display(),b.target,b.progress.histories_available,b.progress.histories_failed))))
                    .child(Button::new(("open-paseo-batch",i)).small().label("Open report").disabled(this.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::BatchResults(id,None),window,cx))))
            }).collect())).h(px(124.)).flex_shrink_0())
            .when_some(self.batch_page.as_ref(),|pane,page|{
                let id=page.summary.id;let next=page.next;
                pane.child(format!("Attempt {} · {}",id,page.summary.state.label(page.summary.writer_active)))
                    .child(format!("Saved source: {} · Saved target Host: {}",page.summary.source.display(),page.summary.target))
                    .when_some(page.summary.progress.current_workspace.as_ref(),|pane,name|pane.child(format!("Last checkpoint: {} · {}",name,page.summary.progress.current_session.as_deref().unwrap_or("Reading workspace"))))
                    .child(div().flex().gap_2()
                        .child(Button::new("paseo-batch-first-page").small().label("Refresh report").disabled(self.busy).on_click(cx.listener(move|this,_,window,cx|this.run(Action::BatchResults(id,None),window,cx))))
                        .child(Button::new("paseo-batch-next-page").small().label("Next results").disabled(self.busy||next.is_none()).on_click(cx.listener(move|this,_,window,cx|this.run(Action::BatchResults(id,next),window,cx))))
                        .child(Button::new("retry-saved-paseo-batch").small().label("Retry saved selection").disabled(self.busy||page.summary.writer_active).on_click(cx.listener(move|this,_,window,cx|this.begin_batch(Action::RetryBatch(id,BatchControl::default()),window,cx)))))
                    .when_some(page.summary.error.clone(),|pane,error|pane.child(div().text_color(cx.theme().danger).child(error)))
                    .child(uniform_list("saved-paseo-batch-results",page.results.len(),cx.processor(|this,range:std::ops::Range<usize>,_,cx|{
                        let Some(page)=&this.batch_page else{return Vec::new();};
                        range.map(|i|{
                            let r=&page.results[i];
                            let details=batch_details(r);
                            div().h(px(62.)).flex().items_center().gap_2()
                                .child(div().flex_1().min_w_0().flex().flex_col()
                                    .child(div().truncate().child(format!("{} · {} histories · {} failures · {} warnings",r.name,r.histories_available,r.history_failures.len(),r.history_warnings.len())))
                                    .child(div().text_xs().truncate().child(r.error.clone().or_else(||r.registration_error.clone()).unwrap_or_else(||if r.histories_finished{"History traversal finished".into()}else{"History traversal incomplete".into()}))))
                                .child(Button::new(("saved-paseo-result-details",i)).small().label("Details").disabled(this.busy).on_click(cx.listener(move|this,_,_,cx|{this.notice=Some(details.clone());cx.notify();})))
                        }).collect()
                    })).h(px(124.)).flex_shrink_0())
            })
    }
}
