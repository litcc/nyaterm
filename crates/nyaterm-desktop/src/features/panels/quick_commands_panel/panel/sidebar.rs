use gpui::{AppContext as _, Context, FontWeight, SharedString, div, prelude::*, px, rgb, rgba};
use nyaterm_ui::{NyaContextMenu, NyaMenuItem, NyaScrollable};

use super::super::super::QuickCommandCategoryOption;
use crate::features::{
    NyaTermApp, commands::QuickCommandDropPosition, commands::QuickCommandDropTarget,
};

use super::{QuickCommandDragKind, QuickCommandDragPayload, QuickCommandDragPreview};

impl NyaTermApp {
    pub(super) fn quick_command_category_sidebar(
        &mut self,
        categories: Vec<QuickCommandCategoryOption>,
        palette: crate::theme::ThemePalette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let mut category_sidebar = div()
            .id(SharedString::from("quick-command-category-scroll"))
            .w(px(176.))
            .h_full()
            .flex_shrink_0()
            // Vertical only. Scrolling both axes lets the rows size to their
            // intrinsic width, which pushed every count pill past the 176px clip.
            .overflow_y_scrollbar()
            .p(px(6.))
            .border_r_1()
            .border_color(rgb(palette.border))
            .flex()
            .flex_col()
            .gap_1();
        for option in categories {
            let id = option.id.clone();
            let drag_option_id = option.id.clone();
            let drag_option_label = option.label.clone();
            let selected = self.commands.quick_selected_category() == option.id;
            let manageable = option.manageable;
            let depth = option.depth;
            let menu_items =
                manageable.then(|| self.quick_command_category_menu_items(option.id.clone(), cx));
            let row = div()
                .id(SharedString::from(format!(
                    "quick-command-category-{}",
                    option.id
                )))
                .relative()
                .h(px(32.))
                .w_full()
                .px_2()
                .flex()
                .items_center()
                .gap_2()
                .pl(px(8. + depth as f32 * 12.))
                .rounded_md()
                .bg(if selected {
                    rgb(palette.hover)
                } else {
                    rgba(0x00000000)
                })
                .text_xs()
                .text_color(if selected {
                    rgb(palette.link)
                } else {
                    rgb(palette.text)
                })
                .cursor_pointer()
                .hover(move |this| this.bg(rgb(palette.hover)))
                .child(
                    div()
                        .size(px(6.))
                        .flex_none()
                        .rounded_full()
                        .when(!selected, |this| this.opacity(0.6))
                        .bg(if selected {
                            rgb(palette.link)
                        } else {
                            rgb(palette.text_dimmed)
                        }),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .font_weight(FontWeight(500.))
                        .child(option.label),
                )
                .child(
                    div()
                        .flex_none()
                        .rounded_sm()
                        .px(px(6.))
                        .py(px(2.))
                        .bg(if selected {
                            rgba((palette.primary << 8) | 0x24)
                        } else {
                            rgb(palette.hover)
                        })
                        .text_size(px(10.))
                        .line_height(px(10.))
                        .text_color(if selected {
                            rgb(palette.primary)
                        } else {
                            rgb(palette.text_dimmed)
                        })
                        .child(option.count.to_string()),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.commands.select_quick_category(id.clone());
                    cx.notify();
                }))
                .when(manageable, |this| {
                    let drag_id = drag_option_id.clone();
                    let drag_label = drag_option_label.clone();
                    let move_target = drag_option_id.clone();
                    let drop_target = drag_option_id.clone();
                    this.cursor_move()
                        .on_drag(
                            QuickCommandDragPayload {
                                kind: QuickCommandDragKind::Category,
                                id: drag_id,
                                label: drag_label,
                            },
                            |payload, position, _, cx| {
                                cx.new(|_| QuickCommandDragPreview {
                                    payload: payload.clone(),
                                    position,
                                })
                            },
                        )
                        .on_drag_move(cx.listener(
                            move |this,
                                  event: &gpui::DragMoveEvent<QuickCommandDragPayload>,
                                  _,
                                  cx| {
                                let _ = event.drag(cx);
                                let relative = if event.bounds.size.height > px(0.) {
                                    ((event.event.position.y - event.bounds.origin.y)
                                        / event.bounds.size.height)
                                        .clamp(0., 1.)
                                } else {
                                    0.5
                                };
                                let position = if relative < 0.25 {
                                    QuickCommandDropPosition::Before
                                } else if relative > 0.75 {
                                    QuickCommandDropPosition::After
                                } else {
                                    QuickCommandDropPosition::Inside
                                };
                                if this.commands.set_quick_drop_target(QuickCommandDropTarget {
                                    id: move_target.clone(),
                                    position,
                                }) {
                                    cx.notify();
                                }
                            },
                        ))
                        .on_drop(cx.listener(
                            move |this, payload: &QuickCommandDragPayload, _, cx| {
                                let position = this
                                    .commands
                                    .quick_drop_target()
                                    .filter(|target| target.id == drop_target)
                                    .map(|target| target.position)
                                    .unwrap_or(QuickCommandDropPosition::Inside);
                                let config = match payload.kind {
                                    QuickCommandDragKind::Command => {
                                        this.commands.move_quick_command_to_category(
                                            &payload.id,
                                            Some(drop_target.clone()),
                                        )
                                    }
                                    QuickCommandDragKind::Category => this
                                        .commands
                                        .move_quick_category(&payload.id, &drop_target, position),
                                };
                                this.finish_quick_command_reorder(config, cx);
                            },
                        ))
                })
                .when(option.id == "uncategorized", |this| {
                    this.on_drop(cx.listener(
                        move |this, payload: &QuickCommandDragPayload, _, cx| {
                            let config = (payload.kind == QuickCommandDragKind::Command)
                                .then(|| {
                                    this.commands
                                        .move_quick_command_to_category(&payload.id, None)
                                })
                                .flatten();
                            this.finish_quick_command_reorder(config, cx);
                        },
                    ))
                });
            category_sidebar = category_sidebar.child(if let Some(menu_items) = menu_items {
                NyaContextMenu::new(row, menu_items).into_any_element()
            } else {
                row.into_any_element()
            });
        }
        category_sidebar.into_any_element()
    }

    fn quick_command_category_menu_items(
        &self,
        category_id: String,
        cx: &mut Context<Self>,
    ) -> Vec<NyaMenuItem> {
        let rename_id = category_id.clone();
        let delete_id = category_id;
        vec![
            NyaMenuItem::action(self.tr("quickCommands.edit"))
                .icon("icons/net/edit.svg")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_rename_quick_command_category(rename_id.clone(), window, cx);
                })),
            NyaMenuItem::separator(),
            NyaMenuItem::action(self.tr("common.delete"))
                .icon("icons/net/delete.svg")
                .danger()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_delete_quick_command_category_confirm(delete_id.clone(), window, cx);
                })),
        ]
    }
}
