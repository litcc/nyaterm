use gpui::{
    Context, FontWeight, IntoElement, KeyDownEvent, SharedString, div, prelude::*, px, rgb, rgba,
};
use nyaterm_core::truncate_preview;
use nyaterm_ui::NyaScrollable;

use super::quick_command_icon_mark;
use crate::features::NyaTermApp;

impl NyaTermApp {
    pub(in crate::features) fn quick_command_details_overlay(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let details = self
            .commands
            .quick_details()
            .cloned()
            .expect("quick command details overlay requires open state");
        let anchor_x = details.x;
        let anchor_y = details.y;
        let command = details.command;
        let category = details.category.trim().to_string();
        let show_category = !category.is_empty();
        let description = command
            .description
            .as_deref()
            .map(str::trim)
            .filter(|description| !description.is_empty());
        let command_text = command.command.clone();
        let estimated_h = if description.is_some() { 224. } else { 182. };
        let (viewport_w, viewport_h) = self.shell.viewport_size();
        let (popover_x, popover_y) = quick_command_details_popover_position(
            f32::from(anchor_x) - 304.,
            f32::from(anchor_y) - estimated_h - 6.,
            320.,
            estimated_h,
            viewport_w,
            viewport_h,
        );

        div()
            .id(SharedString::from("quick-command-details-overlay"))
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .right_0()
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_quick_command_details(cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                cx.stop_propagation();
                if event.keystroke.key == "escape" {
                    this.close_quick_command_details(cx);
                }
            }))
            .child(
                div()
                    .id(SharedString::from("quick-command-details-popover"))
                    .absolute()
                    .left(px(popover_x))
                    .top(px(popover_y))
                    .w(px(320.))
                    .overflow_hidden()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgba((palette.border << 8) | 0x99))
                    .bg(self.shell_surface_color(palette.surface))
                    .shadow_lg()
                    .track_focus(self.commands.quick_details_focus())
                    .on_click(cx.listener(|this, _, window, cx| {
                        window.focus(this.commands.quick_details_focus(), cx);
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .border_b_1()
                            .border_color(rgba((palette.border << 8) | 0x4d))
                            .bg(rgba((palette.surface_elevated << 8) | 0x4d))
                            .p_3()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(quick_command_icon_mark(
                                palette,
                                command.icon_tag.as_deref(),
                                command.color_tag.as_deref(),
                            ))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .text_sm()
                                    .font_weight(FontWeight(700.))
                                    .text_color(rgb(palette.text))
                                    .overflow_hidden()
                                    .child(truncate_preview(&command.label, 48)),
                            )
                            .when(show_category, |this| {
                                this.child(
                                    div()
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
                                        .overflow_hidden()
                                        .child(truncate_preview(&category, 16)),
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
                                        .child(description.to_string()),
                                )
                            })
                            .child(
                                div()
                                    .id(SharedString::from("quick-command-details-command-scroll"))
                                    .max_h(px(120.))
                                    .overflow_scrollbar()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgba((palette.border << 8) | 0x66))
                                    .bg(rgba((palette.bg << 8) | 0x80))
                                    .p(px(10.))
                                    .font_family(crate::features::shell::gpui_code_font_family())
                                    .text_size(px(11.))
                                    .line_height(px(17.))
                                    .text_color(rgb(palette.text))
                                    .child(command_text),
                            ),
                    ),
            )
    }
}

fn quick_command_details_popover_position(
    x: f32,
    y: f32,
    popover_w: f32,
    popover_h: f32,
    viewport_w: f32,
    viewport_h: f32,
) -> (f32, f32) {
    let margin = 8.0;
    let max_x = (viewport_w - popover_w - margin).max(margin);
    let max_y = (viewport_h - popover_h - margin).max(margin);
    (x.clamp(margin, max_x), y.clamp(margin, max_y))
}
