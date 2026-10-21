//! Endpoint diagnostics are independent of conversation initialization and control.
use super::*;
use fleetpane_session::hosts::diagnostics::{self, Report, ResultState};

pub(super) struct State {
    pub active: bool,
    reports: Vec<Option<Report>>,
    checking: Vec<usize>,
    error: Option<String>,
    focus: FocusHandle,
}

impl State {
    pub fn new(cx: &mut Context<Workbench>) -> Self {
        Self {
            active: false,
            reports: Vec::new(),
            checking: Vec::new(),
            error: None,
            focus: cx.focus_handle(),
        }
    }
}

impl Workbench {
    pub(super) fn open_diagnostics(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.export_pending(cx) {
            return;
        }
        self.leave_attention();
        self.sessions.active = false;
        self.choosing = false;
        self.browsing_workspaces = false;
        self.file_navigation = files::State::default();
        self.route_notice = None;
        self.diagnostics.active = true;
        // Retained composers must not receive typing while viewing diagnostics.
        self.diagnostics.focus.focus(window, cx);
        self.check_hosts(None, window, cx);
        cx.notify();
    }

    fn check_hosts(&mut self, index: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.diagnostics.checking.is_empty() {
            return;
        }
        let mut configuration = self.configuration.clone();
        let indices: Vec<usize> =
            index.map_or_else(|| (0..self.slots.len()).collect(), |index| vec![index]);
        configuration.hosts = indices
            .iter()
            .map(|index| self.slots[*index].endpoint.clone())
            .collect();
        configuration.routes.clear();
        self.diagnostics.checking = indices;
        self.diagnostics.error = None;
        let task = cx.background_executor().spawn(async move {
            background(async move {
                diagnostics::collect(configuration)
                    .await
                    .map_err(|error| Error::Input(error.to_string()))
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _closed = this.update_in(cx, |this, _, cx| {
                this.diagnostics.checking.clear();
                match result {
                    Ok(reports) => {
                        this.diagnostics
                            .reports
                            .resize_with(this.slots.len(), || None);
                        for report in reports {
                            let index = this
                                .slots
                                .iter()
                                .position(|slot| slot.endpoint.socket == report.endpoint.socket)
                                .expect("configured diagnostic endpoint");
                            this.diagnostics.reports[index] = Some(report);
                        }
                    }
                    Err(error) => this.diagnostics.error = Some(error.to_string()),
                }
                // A late observation must not navigate away from another page.
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn diagnostics_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = !self.diagnostics.checking.is_empty();
        div().id("host-diagnostics").track_focus(&self.diagnostics.focus)
            .flex_1().min_h_0().overflow_y_scroll().p_4().flex().flex_col().gap_3()
            .child(div().flex().flex_wrap().items_center().gap_2()
                .child(div().text_lg().child("Host diagnostics"))
                .child(Button::new("check-all-hosts").small().label(if busy { "Checking…" } else { "Check all" }).disabled(busy)
                    .on_click(cx.listener(|this, _, window, cx| this.check_hosts(None, window, cx)))))
            .child(div().text_sm().child("Results describe the last check. Recheck after a runtime change. Counts alone do not establish that a Host is safe to stop."))
            .when_some(self.diagnostics.error.as_ref(), |view, error| view.child(div().text_sm().text_color(cx.theme().danger).child(format!("Check failed; previous results retained: {error}"))))
            .children(self.slots.iter().enumerate().map(|(index, slot)| {
                let report = self.diagnostics.reports.get(index).and_then(Option::as_ref);
                div().id(("diagnostic-host", index)).p_3().flex().flex_col().gap_2().border_1().border_color(cx.theme().border).rounded_md()
                    .child(div().flex().flex_wrap().items_center().gap_2()
                        .child(div().text_sm().child(slot.endpoint.name.clone()))
                        .child(div().text_sm().child(if self.diagnostics.checking.contains(&index) { "Checking · previous result below" } else { report.map_or("Not checked", Report::label) }))
                        .child(Button::new(("check-host", index)).small().label("Check").disabled(busy).on_click(cx.listener(move |this, _, window, cx| this.check_hosts(Some(index), window, cx))))
                        .child(Button::new(("view-host", index)).small().label("View sessions").on_click(cx.listener(move |this, _, window, cx| this.select(index, window, cx))))
                        .when_some(report, |row, report| {
                            let text = serde_json::to_string_pretty(report).expect("typed diagnostic JSON serializes");
                            row.child(Button::new(("copy-host-diagnostic", index)).small().label("Copy diagnostics").disabled(busy).on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))))
                        }))
                    .child(div().text_xs().child(slot.endpoint.socket.display().to_string()))
                    .when_some(report, |view, report| {
                        let view = view.child(div().text_xs().child(format!("Check started {} ms since Unix epoch · {} ms elapsed", report.checked_at_unix_ms, report.elapsed_ms)));
                        match &report.result {
                            ResultState::Unavailable { error } => view.child(div().text_sm().text_color(cx.theme().danger).child(error.clone())),
                            ResultState::Reachable { observation: o } => view
                                .child(div().text_xs().child(format!("Protocol {} · expected {} · Host PID {} · provider PID {}", o.protocol, report.expected_protocol, o.host_pid, o.provider_pid)))
                                .child(div().text_xs().child(format!("Host {}", o.host_id)))
                                .child(div().text_xs().child(match &o.owner {
                                    Some(lease) => format!("Controller {} · generation {}", lease.controller_id, lease.generation),
                                    None => "No controller lease".into(),
                                }))
                                .child(div().text_xs().child(format!("Admission {:?} · generation {} · sequence {} · saved operations {}", o.admission.mode, o.admission.generation, o.sequence, o.journal_total)))
                                .child(div().text_xs().child(format!("Observed: {} active turns · {} pending requests · {} in-flight · {} unknown operations · {} terminals · {}", o.active_turns, o.pending_requests, o.in_flight_operations, o.unknown_operations, o.terminals, o.stopping.map_or_else(|| "Stop progress unreported".into(), |count| format!("{count} Stops")))))
                                .when_some(o.fault.as_ref(), |view, fault| view.child(div().text_sm().text_color(cx.theme().danger).child(fault.clone()))),
                        }
                    })
            }))
    }
}
