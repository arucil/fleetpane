use fleetpane_session::questions::{Request, Response};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    radio::{Radio, RadioGroup},
};
use gpui_kit::{
    App, Context, Entity, EventEmitter, IntoElement, Render, Subscription, Window, div, prelude::*,
    px, rems,
};

pub struct AnswerEvent {
    pub request: Request,
    pub response: Response,
}

enum Editor {
    Text(Entity<TextareaState>),
    Secret(Entity<InputState>),
}

impl Editor {
    fn value(&self, cx: &App) -> String {
        match self {
            Self::Text(input) => input.read(cx).value().to_string(),
            Self::Secret(input) => input.read(cx).value().to_string(),
        }
    }
}

pub struct QuestionView {
    pub request: Request,
    page: usize,
    selected: Vec<Option<usize>>,
    editors: Vec<Editor>,
    enabled: bool,
    submitted: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<AnswerEvent> for QuestionView {}

impl QuestionView {
    pub fn new(request: Request, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut subscriptions = Vec::new();
        let editors = request
            .params
            .questions
            .iter()
            .map(|question| {
                if question.is_secret {
                    let input = cx.new(|cx| {
                        InputState::new(window, cx)
                            .masked(true)
                            .placeholder("Sensitive answer")
                    });
                    subscriptions.push(cx.subscribe(&input, |_, _, event, cx| {
                        if matches!(event, InputEvent::Change) {
                            cx.notify();
                        }
                    }));
                    Editor::Secret(input)
                } else {
                    let input = cx.new(|cx| {
                        TextareaState::new(window, cx).placeholder(
                            if question.labels().is_empty() {
                                "Your answer · Enter adds a line"
                            } else {
                                "Optional details · Enter adds a line"
                            },
                        )
                    });
                    subscriptions.push(cx.subscribe(&input, |_, _, event, cx| {
                        if matches!(event, InputEvent::Change) {
                            cx.notify();
                        }
                    }));
                    Editor::Text(input)
                }
            })
            .collect();
        Self {
            selected: vec![None; request.params.questions.len()],
            request,
            page: 0,
            editors,
            enabled: false,
            submitted: false,
            _subscriptions: subscriptions,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.enabled != enabled {
            self.enabled = enabled;
            cx.notify();
        }
    }

    pub fn allow_submit(&mut self, cx: &mut Context<Self>) {
        self.submitted = false;
        cx.notify();
    }

    fn response(&self, cx: &App) -> Option<Response> {
        let answers = self
            .request
            .params
            .questions
            .iter()
            .enumerate()
            .map(|(index, question)| {
                question
                    .answer(self.selected[index], &self.editors[index].value(cx))
                    .map(|answer| (question.id.clone(), answer))
            })
            .collect::<Option<_>>()?;
        Some(Response { answers })
    }
}

impl Render for QuestionView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let question = &self.request.params.questions[self.page];
        let disabled = !self.enabled || self.submitted;
        let labels = question.labels();
        let page = self.page;
        let count = self.request.params.questions.len();
        let complete = self.response(cx).is_some();
        let options = RadioGroup::new(("question-options", page))
            .selected_index(self.selected[page])
            .disabled(disabled)
            .children(
                labels
                    .into_iter()
                    .enumerate()
                    .map(|(index, label)| Radio::new(("option", index)).label(label)),
            )
            .on_change(cx.listener(move |this, index: &usize, _, cx| {
                this.selected[page] = Some(*index);
                cx.notify();
            }));
        let editor = match &self.editors[page] {
            Editor::Text(input) => Textarea::new(input)
                .h(rems(4.))
                .disabled(disabled)
                .into_any_element(),
            Editor::Secret(input) => Input::new(input).disabled(disabled).into_any_element(),
        };
        div()
            .p_4()
            .border_t_1()
            .border_color(cx.theme().border)
            .flex()
            .flex_col()
            .gap_2()
            .child(format!(
                "Agent question · {} of {count} · {}",
                page + 1,
                question.header
            ))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "Session {} · {}",
                        self.request.params.thread_id,
                        if self.request.params.is_blocking {
                            "Waiting for your answer"
                        } else {
                            "Agent may continue while you answer"
                        }
                    )),
            )
            .child(
                div()
                    .id("question-body")
                    .max_h(px(240.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(question.question.clone())
                    .when(!question.labels().is_empty(), |body| body.child(options))
                    .children(question.options.iter().flatten().map(|option| {
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{}: {}", option.label, option.description))
                    }))
                    .child(editor),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("previous-question")
                            .small()
                            .ghost()
                            .label("Previous")
                            .disabled(disabled || page == 0)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.page -= 1;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("next-question")
                            .small()
                            .ghost()
                            .label("Next")
                            .disabled(disabled || page + 1 == count)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.page += 1;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("submit-answers")
                            .small()
                            .label(if self.submitted {
                                "Reconciling answer…"
                            } else {
                                "Submit answers"
                            })
                            .disabled(disabled || !complete)
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.enabled
                                    && !this.submitted
                                    && let Some(response) = this.response(cx)
                                {
                                    this.submitted = true;
                                    cx.emit(AnswerEvent {
                                        request: this.request.clone(),
                                        response,
                                    });
                                    cx.notify();
                                }
                            })),
                    ),
            )
    }
}
