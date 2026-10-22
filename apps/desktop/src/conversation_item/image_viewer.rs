//! A window-local view of an already prepared conversation image.
use gpui_kit::base::Selectable;
use gpui_kit::component::{ActiveTheme, Sizable, WindowExt, button::Button};
use gpui_kit::{
    App, ClipboardItem, Context, ImageSource, ObjectFit, Render, SharedString, StyledImage, Window,
    div, img, prelude::*, px,
};

pub(super) fn open(
    source: ImageSource,
    label: SharedString,
    identity: SharedString,
    window: &mut Window,
    cx: &mut App,
) {
    // Allocate once per opening. Kit may rebuild the dialog builder on later frames.
    let excerpt: String = identity.chars().take(120).collect();
    let excerpt = if excerpt.len() < identity.len() {
        format!("{excerpt}…")
    } else {
        excerpt
    };
    let viewer = cx.new(|_| ImageViewer {
        source,
        label,
        identity,
        excerpt: excerpt.into(),
        actual_size: false,
    });
    // Kit's existing Root owns modality, Escape and the previous focus handle.
    window.open_dialog(cx, move |dialog, window, _| {
        dialog
            .title("Image viewer")
            .w((window.viewport_size().width - px(64.)).max(px(0.)))
            .margin_top(px(24.))
            .content({
                let viewer = viewer.clone();
                move |content, _, _| content.min_h_0().child(viewer.clone())
            })
    });
}

struct ImageViewer {
    source: ImageSource,
    label: SharedString,
    identity: SharedString,
    excerpt: SharedString,
    actual_size: bool,
}

impl Render for ImageViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let identity = self.identity.clone();
        let image = img(self.source.clone())
            .with_loading(|| div().child("Loading image…").into_any_element())
            .with_fallback(|| {
                div()
                    .child("Image unavailable — file missing, inaccessible or unsupported")
                    .into_any_element()
            });
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_sm().truncate().child(self.label.clone()))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .truncate()
                    .child(self.excerpt.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("image-fit")
                            .small()
                            .label("Fit")
                            .selected(!self.actual_size)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.actual_size = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("image-actual-size")
                            .small()
                            .label("100%")
                            .selected(self.actual_size)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.actual_size = true;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("image-copy-source")
                            .small()
                            .label("Copy source")
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    identity.to_string(),
                                ));
                            }),
                    )
                    .child(
                        Button::new("image-close")
                            .small()
                            .label("Close")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    ),
            )
            .child(
                div()
                    // Distinct scrollers reset the old offset when returning to Fit.
                    .id(if self.actual_size {
                        "image-100"
                    } else {
                        "image-fit"
                    })
                    .relative()
                    .w_full()
                    .h((window.viewport_size().height - px(220.)).max(px(0.)))
                    .min_h_0()
                    .bg(cx.theme().muted)
                    .when(self.actual_size, |view| view.overflow_scroll())
                    .when(!self.actual_size, |view| view.overflow_hidden())
                    .child(if self.actual_size {
                        // GPUI uses intrinsic image dimensions when neither axis is set.
                        image.flex_shrink_0().into_any_element()
                    } else {
                        // Intrinsic image dimensions must not enlarge the dialog body.
                        image
                            .absolute()
                            .inset_0()
                            .size_full()
                            .object_fit(ObjectFit::Contain)
                            .into_any_element()
                    }),
            )
    }
}
