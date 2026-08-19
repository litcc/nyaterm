//! The 320px command card shared by the tile hover tooltip and the eye popover.
//!
//! Tauri renders the same block in both places (`renderCommandDetailsPopover` and
//! the tile `TooltipContent`); the only differences are the shell around it, the
//! execution-mode line the tile adds, and the copy button, which needs a click and
//! so cannot live in a tooltip.

use gpui::{
    AnyElement, Context, FontWeight, IntoElement, Render, SharedString, Window, div, prelude::*,
    px, rgb, rgba, svg,
};
use nyaterm_core::QuickCommand;
use nyaterm_ui::NyaScrollable;

use super::super::quick_command_icon_mark;

pub(in crate::features) struct QuickCommandCardContent<'a> {
    pub command: &'a QuickCommand,
    /// Empty when the command is uncategorized.
    pub category: &'a str,
    /// `Some` adds Tauri's ⚡ / ↵ execution-mode line under the title.
    pub execution_mode: Option<QuickCommandCardExecutionMode>,
    /// Built by the caller so this module stays free of listener generics.
    pub copy_button: Option<AnyElement>,
}

#[derive(Clone, Copy)]
pub(in crate::features) struct QuickCommandCardExecutionMode {
    pub append: bool,
    pub label: &'static str,
}

pub(in crate::features) fn quick_command_detail_card(
    palette: crate::theme::ThemePalette,
    content: QuickCommandCardContent<'_>,
) -> impl IntoElement {
    let QuickCommandCardContent {
        command,
        category,
        execution_mode,
        copy_button,
    } = content;
    let category = category.trim();
    let description = command
        .description
        .as_deref()
        .map(str::trim)
        .filter(|description| !description.is_empty())
        .map(ToOwned::to_owned);

    div()
        .flex()
        .flex_col()
        .child(
            div()
                .border_b_1()
                .border_color(rgba((palette.border << 8) | 0x4d))
                .bg(rgba((palette.surface_elevated << 8) | 0x4d))
                .p_3()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .size(px(16.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(quick_command_icon_mark(
                                    palette,
                                    command.icon_tag.as_deref(),
                                    command.color_tag.as_deref(),
                                    14.,
                                )),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .text_sm()
                                .font_weight(FontWeight(700.))
                                .text_color(rgb(palette.text))
                                .truncate()
                                .child(command.label.clone()),
                        )
                        .when(!category.is_empty(), |this| {
                            this.child(
                                div()
                                    .flex_none()
                                    .max_w(px(112.))
                                    .rounded_full()
                                    .border_1()
                                    .border_color(rgba((palette.primary << 8) | 0x33))
                                    .bg(rgba((palette.primary << 8) | 0x1a))
                                    .px_2()
                                    .py(px(1.))
                                    .text_size(px(10.))
                                    .font_weight(FontWeight(600.))
                                    .text_color(rgb(palette.link))
                                    .truncate()
                                    .child(category.to_string()),
                            )
                        }),
                )
                .when_some(execution_mode, |this, mode| {
                    this.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(11.))
                            .text_color(rgb(palette.text_muted))
                            .child(
                                svg()
                                    .size(px(12.))
                                    .flex_none()
                                    .path(if mode.append {
                                        "icons/keyboard-return.svg"
                                    } else {
                                        "icons/conn/flash.svg"
                                    })
                                    .text_color(rgb(palette.text_muted)),
                            )
                            .child(mode.label),
                    )
                }),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .p_3()
                .when_some(description, |this, description| {
                    this.child(
                        div()
                            .text_xs()
                            .line_height(px(18.))
                            .text_color(rgb(palette.text_muted))
                            .child(description),
                    )
                })
                .child(
                    // The copy button is a sibling of the scroll box, not a child:
                    // an absolutely positioned child would ride the scroll offset.
                    div()
                        .relative()
                        .child(
                            div()
                                .id(SharedString::from("quick-command-card-command"))
                                .max_h(px(120.))
                                .rounded_md()
                                .border_1()
                                .border_color(rgba((palette.border << 8) | 0x66))
                                .bg(rgba((palette.bg << 8) | 0x80))
                                .p(px(10.))
                                .when(copy_button.is_some(), |this| this.pr(px(36.)))
                                .font_family(crate::features::shell::gpui_code_font_family())
                                .text_size(px(11.))
                                .line_height(px(17.))
                                .text_color(rgb(palette.text))
                                .child(command.command.clone())
                                .overflow_y_scrollbar(),
                        )
                        .children(copy_button),
                ),
        )
}

/// The tile hover card. A tooltip rather than an in-tree popover: the panel clips
/// its overflow, and tiles painted after the hovered one would cover it.
pub(in crate::features) struct QuickCommandTooltip {
    palette: crate::theme::ThemePalette,
    surface: gpui::Rgba,
    command: QuickCommand,
    category: String,
    execution_mode: QuickCommandCardExecutionMode,
}

impl QuickCommandTooltip {
    pub(in crate::features) fn new(
        palette: crate::theme::ThemePalette,
        surface: gpui::Rgba,
        command: QuickCommand,
        category: String,
        execution_mode: QuickCommandCardExecutionMode,
    ) -> Self {
        Self {
            palette,
            surface,
            command,
            category,
            execution_mode,
        }
    }
}

impl Render for QuickCommandTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(320.))
            .overflow_hidden()
            .rounded_lg()
            .border_1()
            .border_color(rgba((self.palette.border << 8) | 0x99))
            .bg(self.surface)
            .shadow_lg()
            .child(quick_command_detail_card(
                self.palette,
                QuickCommandCardContent {
                    command: &self.command,
                    category: &self.category,
                    execution_mode: Some(self.execution_mode),
                    copy_button: None,
                },
            ))
    }
}
