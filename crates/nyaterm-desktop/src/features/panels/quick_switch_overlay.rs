use rust_i18n::t;

use gpui::{
    Context, FontWeight, IntoElement, KeyDownEvent, SharedString, StatefulInteractiveElement as _,
    div, prelude::*, px, rgb, rgba, svg,
};
use nyaterm_core::truncate_preview;
use nyaterm_ui::{NyaInput, NyaScrollable};

use crate::features::{NyaTermApp, text_inputs::TextInputSetup};
use crate::models::QuickSwitchItem;
use crate::widgets::status_pill;

impl NyaTermApp {
    pub(in crate::features) fn quick_switch_overlay(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let (viewport_w, viewport_h) = self.shell.viewport_size();
        let items = self.filtered_quick_switch_items(cx);
        self.update_quick_switch_state(cx, |store| {
            store.clamp_quick_switch_selected_index(items.len())
        });
        let state = self.quick_switch_state(cx);
        let selected_index = state.selected_index();
        let query_input = self.text_input(
            "quick-switch.query",
            state.query(),
            TextInputSetup::placeholder(t!("sessionQuickSwitcher.searchPlaceholder")),
            cx,
        );
        let query_focus = query_input.read(cx).focus_handle();
        let list_max_height = (self.shell.viewport_size().1 * 0.55).clamp(160., 384.);
        let selected_row_bg = rgba((palette.primary << 8) | 0x26);
        let hover_row_bg = self.shell_surface_color(palette.hover);
        let mut rows = div()
            .id(SharedString::from("quick-switch-results"))
            .max_h(px(list_max_height))
            .overflow_y_scrollbar()
            .flex()
            .flex_col();

        if items.is_empty() {
            rows = rows.child(
                div()
                    .h(px(120.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_xs()
                    .text_color(rgb(palette.text_muted))
                    .child(if self.quick_switch_items().is_empty() {
                        t!("sessionQuickSwitcher.noSessions")
                    } else {
                        t!("sessionQuickSwitcher.noMatches")
                    }),
            );
        } else {
            for (index, item) in items.into_iter().enumerate().take(50) {
                let selected = index == selected_index;
                let item_for_click = item.clone();
                let badge = match &item {
                    QuickSwitchItem::Session { active, .. } => {
                        if *active {
                            status_pill(
                                t!("sessionQuickSwitcher.active"),
                                rgb(palette.primary),
                                rgba((palette.primary << 8) | 0x1a),
                            )
                            .into_any_element()
                        } else {
                            div().into_any_element()
                        }
                    }
                    QuickSwitchItem::Connection { .. } => status_pill(
                        t!("sessionQuickSwitcher.saved"),
                        rgb(palette.text_muted),
                        self.shell_surface_color(palette.hover),
                    )
                    .into_any_element(),
                    QuickSwitchItem::Pending { active, .. } => {
                        if *active {
                            status_pill(
                                t!("sessionQuickSwitcher.active"),
                                rgb(palette.primary),
                                rgba((palette.primary << 8) | 0x1a),
                            )
                            .into_any_element()
                        } else {
                            div().into_any_element()
                        }
                    }
                };

                rows = rows.child(
                    div()
                        .id(SharedString::from(format!(
                            "quick-switch-item-{}",
                            item.id()
                        )))
                        .min_h(px(48.))
                        .px_3()
                        .py_2()
                        .flex()
                        .items_center()
                        .gap_3()
                        .bg(if selected {
                            selected_row_bg
                        } else {
                            rgba(0x00000000)
                        })
                        .cursor_pointer()
                        .when(!selected, |this| {
                            this.hover(move |this| this.bg(hover_row_bg))
                        })
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight(500.))
                                        .text_color(rgb(palette.text))
                                        .overflow_hidden()
                                        .child(truncate_preview(item.title(), 54)),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(palette.text_muted))
                                        .overflow_hidden()
                                        .child(truncate_preview(item.subtitle(), 78)),
                                ),
                        )
                        .child(badge)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.update_quick_switch_state(cx, |store| {
                                store.set_quick_switch_selected_index(index)
                            });
                            this.select_quick_switch_item(item_for_click.clone(), window, cx);
                        })),
                );
            }
        }

        div()
            .id(SharedString::from("quick-switch-overlay"))
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .right_0()
            .bg(rgba(0x00000080))
            .flex()
            .items_start()
            .justify_center()
            .pt(px(viewport_h * 0.18))
            .on_click(cx.listener(|this, _, window, cx| {
                this.close_quick_switch(cx);
                window.focus(this.terminal.input_focus(), cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                cx.stop_propagation();
                this.handle_quick_switch_key_down(event, window, cx);
            }))
            .child(
                div()
                    .id(SharedString::from("quick-switch-dialog"))
                    .w(px((viewport_w - 32.).clamp(1., 640.)))
                    .max_w_full()
                    .mx_4()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(self.shell_surface_color(palette.bg))
                    .shadow_lg()
                    .overflow_hidden()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .id("quick-switch-query-input-shell")
                            .relative()
                            .h(px(44.))
                            .flex()
                            .items_center()
                            .gap_3()
                            .px_3()
                            .border_b_1()
                            .border_color(rgb(palette.border))
                            .bg(rgba(0x00000000))
                            .cursor_text()
                            .on_click(move |_, window, cx| {
                                window.focus(&query_focus, cx);
                            })
                            .child(
                                svg()
                                    .size(px(16.))
                                    .flex_none()
                                    .path("icons/fe/search.svg")
                                    .text_color(rgb(palette.text_muted)),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .text_sm()
                                    .text_color(rgb(palette.text))
                                    .child(NyaInput::new(&query_input)),
                            ),
                    )
                    .child(rows)
                    .child(
                        div()
                            .h(px(40.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .px_3()
                            .border_t_1()
                            .border_color(rgb(palette.border))
                            .bg(rgba(0x00000000))
                            .child(div().text_xs().text_color(rgb(palette.text_muted)).child(
                                format!(
                                    "Enter {} / Esc {}",
                                    t!("sessionQuickSwitcher.open"),
                                    t!("sessionQuickSwitcher.close")
                                ),
                            ))
                            .child(
                                div().flex().items_center().child(
                                    div()
                                        .id("quick-switch-new-ssh")
                                        .h(px(28.))
                                        .px_3()
                                        .flex()
                                        .items_center()
                                        .rounded_sm()
                                        .bg(rgb(palette.primary))
                                        .text_color(rgb(palette.on_primary))
                                        .text_xs()
                                        .cursor_pointer()
                                        .hover(|this| this.bg(rgb(palette.primary_hover)))
                                        .child(t!("sessionQuickSwitcher.newSsh"))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.forget_text_inputs("quick-switch.query");
                                            this.update_quick_switch_state(cx, |store| {
                                                store.close_quick_switch()
                                            });
                                            this.open_connection_editor(
                                                None, None, true, window, cx,
                                            );
                                        })),
                                ),
                            ),
                    ),
            )
    }
}
