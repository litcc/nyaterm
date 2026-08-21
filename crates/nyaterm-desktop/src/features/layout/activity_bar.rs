use std::borrow::Cow;

use gpui::{
    Context, FontWeight, IntoElement, MouseButton, MouseDownEvent, SharedString, div, prelude::*,
    px, rgb, rgba, svg,
};

use crate::features::NyaTermApp;
use crate::features::runtime_jobs::ActivitySide;
use crate::features::shell::{ActivityBarDragPayload, ActivityBarDragPreview};
use crate::features::view_widgets::activity_icon;
use crate::models::{ActivityBarEntry, ActivityBarZone};
use nyaterm_ui::NyaTooltip;

impl NyaTermApp {
    pub(in crate::features) fn activity_bar_context_menu_overlay(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let Some(menu) = self.shell.activity_bar_context_menu().cloned() else {
            return div().into_any_element();
        };
        let entry_id = menu.entry_id.clone();
        let show_labels = self.shell.activity_bar_layout().show_labels;
        let (viewport_w, viewport_h) = self.shell.viewport_size();
        let menu_w = 180.;
        let submenu_w = 164.;
        let margin = 8.;
        let menu_x = f32::from(menu.x).clamp(margin, (viewport_w - menu_w - margin).max(margin));
        let menu_y = f32::from(menu.y).clamp(margin, (viewport_h - 70. - margin).max(margin));
        let move_to_label = self.tr("activityBar.moveTo");
        let show_labels_label = self.tr("activityBar.showLabel");

        let mut zone_buttons = div().flex().flex_col().gap_1();
        for zone in ActivityBarZone::all() {
            let target = zone;
            let id = entry_id.clone();
            let selected = zone == menu.zone;
            zone_buttons = zone_buttons.child(
                div()
                    .id(SharedString::from(format!(
                        "activity-move-{}",
                        zone.persistence_key()
                    )))
                    .h(px(28.))
                    .px_3()
                    .flex()
                    .items_center()
                    .text_xs()
                    .text_color(rgb(palette.text))
                    .when(selected, |this| this.opacity(0.45))
                    .when(!selected, |this| {
                        this.cursor_pointer()
                            .hover(|this| this.bg(rgb(palette.hover)))
                    })
                    .child(self.tr(zone.i18n_key()))
                    .when(!selected, |this| {
                        this.on_click(cx.listener(move |this, _, _, cx| {
                            this.move_activity_entry(id.clone(), target, None, cx);
                        }))
                    }),
            );
        }

        let submenu_x = if menu_x + menu_w + 4. + submenu_w <= viewport_w - margin {
            menu_x + menu_w + 4.
        } else {
            (menu_x - submenu_w - 4.).max(margin)
        };
        let submenu_y = menu_y.clamp(margin, (viewport_h - 128. - margin).max(margin));

        let parent_menu = div()
            .id(SharedString::from("activity-context-menu"))
            .absolute()
            .top(px(menu_y))
            .left(px(menu_x))
            .w(px(menu_w))
            .rounded_md()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(self.shell_surface_color(palette.surface))
            .shadow_lg()
            .py_1()
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id(SharedString::from("activity-move-to"))
                    .h(px(28.))
                    .px_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_xs()
                    .text_color(rgb(palette.text))
                    .cursor_pointer()
                    .when(menu.move_submenu_open, |this| this.bg(rgb(palette.hover)))
                    .hover(|this| this.bg(rgb(palette.hover)))
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        if *hovered {
                            this.open_activity_bar_move_submenu(cx);
                        }
                    }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open_activity_bar_move_submenu(cx);
                    }))
                    .child(move_to_label)
                    .child(
                        svg()
                            .size(px(12.))
                            .path("icons/fe/forward.svg")
                            .text_color(rgb(palette.text_dimmed)),
                    ),
            )
            .child(div().h(px(1.)).my_1().mx_2().bg(rgb(palette.border)))
            .child(
                div()
                    .id(SharedString::from("activity-toggle-labels"))
                    .h(px(28.))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .text_color(rgb(palette.text))
                    .cursor_pointer()
                    .hover(|this| this.bg(rgb(palette.hover)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_activity_bar_labels(cx);
                        this.close_activity_bar_context_menu(cx);
                    }))
                    .child(
                        div()
                            .w(px(14.))
                            .flex_none()
                            .text_color(rgb(palette.link))
                            .when(show_labels, |this| {
                                this.child(
                                    svg()
                                        .size(px(13.))
                                        .path("icons/check.svg")
                                        .text_color(rgb(palette.link)),
                                )
                            }),
                    )
                    .child(show_labels_label),
            );

        div()
            .id(SharedString::from("activity-context-backdrop"))
            .absolute()
            .inset_0()
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_activity_bar_context_menu(cx);
            }))
            .child(parent_menu)
            .when(menu.move_submenu_open, |this| {
                this.child(
                    div()
                        .id(SharedString::from("activity-move-submenu"))
                        .absolute()
                        .top(px(submenu_y))
                        .left(px(submenu_x))
                        .w(px(submenu_w))
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(palette.border))
                        .bg(self.shell_surface_color(palette.surface))
                        .shadow_lg()
                        .py_1()
                        .flex()
                        .flex_col()
                        .child(zone_buttons)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(|_, _, cx| cx.stop_propagation()),
                )
            })
            .into_any_element()
    }

    pub(in crate::features) fn activity_bar(
        &mut self,
        side: ActivitySide,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (top_zone, bottom_zone) = match side {
            ActivitySide::Left => (ActivityBarZone::LeftTop, ActivityBarZone::LeftBottom),
            ActivitySide::Right => (ActivityBarZone::RightTop, ActivityBarZone::RightBottom),
        };
        let top_entries = self.activity_entries_for_zone(top_zone);
        let bottom_entries = self.activity_entries_for_zone(bottom_zone);
        let top_len = top_entries.len();
        let bottom_len = bottom_entries.len();
        let show_labels = self.shell.activity_bar_layout().show_labels;
        let palette = self.theme_palette();

        // Tauri DropZone: gap-0.5 pt-1
        let mut top = div().flex().flex_col().items_center().gap(px(2.)).pt_1();
        for (index, entry) in top_entries.into_iter().enumerate() {
            top = top.child(self.activity_entry_button(
                entry,
                side,
                top_zone,
                index,
                show_labels,
                cx,
            ));
        }
        // End-of-zone drop target (append).
        top = top.child(self.activity_zone_end_drop_target(top_zone, top_len, cx));

        let mut bottom = div()
            .mt_auto()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2.))
            .pb_1();
        for (index, entry) in bottom_entries.into_iter().enumerate() {
            bottom = bottom.child(self.activity_entry_button(
                entry,
                side,
                bottom_zone,
                index,
                show_labels,
                cx,
            ));
        }
        bottom = bottom.child(self.activity_zone_end_drop_target(bottom_zone, bottom_len, cx));

        div()
            .w(px(40.))
            .flex_none()
            .flex()
            .flex_col()
            .border_color(rgb(palette.border))
            .when(side == ActivitySide::Left, |this| this.border_r_1())
            .when(side == ActivitySide::Right, |this| this.border_l_1())
            .bg(self.shell_surface_color(palette.surface))
            .child(top)
            .child(bottom)
    }

    pub(in crate::features) fn activity_zone_end_drop_target(
        &self,
        zone: ActivityBarZone,
        end_index: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let zone_key = zone.persistence_key();
        div()
            .id(SharedString::from(format!("activity-zone-end-{zone_key}")))
            .w_full()
            .h(px(8.))
            .flex_none()
            .on_drop(
                cx.listener(move |this, payload: &ActivityBarDragPayload, _, cx| {
                    if payload.entry_id.is_empty() {
                        return;
                    }
                    this.move_activity_entry(payload.entry_id.clone(), zone, Some(end_index), cx);
                }),
            )
    }

    pub(in crate::features) fn activity_entry_button(
        &self,
        entry: ActivityBarEntry,
        side: ActivitySide,
        zone: ActivityBarZone,
        index: usize,
        show_labels: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.activity_entry_selected(entry);
        let icon_path = entry.icon_path();
        let tooltip = entry
            .i18n_key()
            .map(|key| self.tr(key))
            .unwrap_or_else(|| Cow::Borrowed(entry.label()))
            .to_string();
        let palette = self.theme_palette();
        let active_color = rgb(palette.primary);
        let icon_color = if selected {
            active_color
        } else {
            rgb(palette.text_muted)
        };
        let entry_id = entry.persistence_id().to_string();
        let context_entry_id = entry_id.clone();
        let indicator = if selected {
            active_color
        } else {
            rgba(0x00000000)
        };

        div()
            .id(SharedString::from(format!("activity-{entry_id}")))
            .relative()
            .when(show_labels, |this| {
                this.w_full()
                    .min_h(px(48.))
                    .px_1()
                    .py_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(2.))
            })
            .when(!show_labels, |this| {
                this.w_full()
                    .h(px(36.))
                    .flex()
                    .items_center()
                    .justify_center()
            })
            .rounded_sm()
            .cursor_pointer()
            .text_sm()
            .font_weight(FontWeight(700.))
            .text_color(if selected {
                // Tauri ActivityBarButton uses primary color when active.
                active_color
            } else {
                rgb(palette.text_muted)
            })
            .bg(rgba(0x00000000))
            .child(
                div()
                    .absolute()
                    .top(px(4.))
                    .bottom(px(4.))
                    .w(px(2.))
                    .rounded_full()
                    .bg(indicator)
                    .when(side == ActivitySide::Left, |this| this.left_0())
                    .when(side == ActivitySide::Right, |this| this.right_0()),
            )
            .child(activity_icon(icon_path, icon_color.into(), 18.))
            .when(show_labels, |this| {
                this.child(
                    div()
                        .w_full()
                        .text_size(px(8.))
                        .font_weight(FontWeight(500.))
                        .text_center()
                        .whitespace_normal()
                        .text_color(if selected {
                            active_color
                        } else {
                            rgb(palette.text_muted)
                        })
                        .child(tooltip.clone()),
                )
            })
            .cursor_move()
            .on_drag(
                ActivityBarDragPayload {
                    entry_id: entry_id.clone(),
                    label: tooltip.clone(),
                    icon_path,
                },
                |payload, position, _, cx| {
                    cx.new(|_| ActivityBarDragPreview::new(payload.clone(), position))
                },
            )
            .on_drop({
                let drop_zone = zone;
                let drop_index = index;
                cx.listener(move |this, payload: &ActivityBarDragPayload, _, cx| {
                    if payload.entry_id.is_empty() {
                        return;
                    }
                    // Drop onto this button inserts before it (Tauri dropIndex == idx).
                    this.move_activity_entry(
                        payload.entry_id.clone(),
                        drop_zone,
                        Some(drop_index),
                        cx,
                    );
                })
            })
            .tooltip({
                let title = tooltip.clone();
                move |window, cx| NyaTooltip::new(title.clone()).build(window, cx)
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.activate_activity_entry(entry, window, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.open_activity_bar_context_menu(
                        context_entry_id.clone(),
                        zone,
                        index,
                        event,
                        cx,
                    );
                }),
            )
    }
}
