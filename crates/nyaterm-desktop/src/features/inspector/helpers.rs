use gpui::{IntoElement, SharedString, div, prelude::*, px, rgb};

pub(in crate::features) fn disabled_inspector_panel(
    palette: crate::theme::ThemePalette,
    detail: impl Into<SharedString>,
) -> impl IntoElement {
    let detail: SharedString = detail.into();
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .px_4()
        .text_center()
        .child(
            div()
                .text_sm()
                .line_height(px(20.))
                .text_color(rgb(palette.text_muted))
                .child(detail),
        )
}
