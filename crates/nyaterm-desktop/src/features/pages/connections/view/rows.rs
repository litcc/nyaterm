use std::sync::Arc;

use gpui::{
    AnyElement, AppContext as _, Context, FontWeight, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, Render, SharedString, Window, div,
    prelude::{
        FluentBuilder, InteractiveElement, ParentElement, StatefulInteractiveElement, Styled,
    },
    px, relative, rgb, rgba, svg,
};
use nyaterm_core::SavedConnection;
use nyaterm_ui::NyaInput;

use crate::features::{
    connections::ConnectionDragKind, connections::ConnectionDragPayload,
    connections::ConnectionDragPreview, connections::ConnectionDropPosition,
    connections::ConnectionDropTarget, icons::resolve_connection_icon,
    text_inputs::ORDINARY_INPUT_SHELL_PADDING_X_PX, text_inputs::ordinary_input_focus_ring,
    text_inputs::ordinary_input_shell_border_color, view_widgets::connection_type_icon,
};

use super::super::list::{
    ConnectionSectionHeader, connection_detail_rows, connection_tree_indent_px,
};
use super::super::panel::{ConnectionListSnapshot, ConnectionPanel};
use super::CONNECTION_ACTION_CLEARANCE_PX;

/// The label/value card shown after hovering a saved connection.
///
/// This is a tooltip rather than an absolutely positioned child of the row: the
/// panel clips its overflow, and rows painted after the hovered one would cover
/// an in-tree card anyway. As a tooltip it is deferred to the top paint layer and
/// flips to stay inside the window, which is what the old UI got from portalling.
pub(in crate::features) struct ConnectionDetailsTooltip {
    rows: Arc<[(&'static str, String)]>,
}

impl ConnectionDetailsTooltip {
    pub(in crate::features) fn new(rows: Arc<[(&'static str, String)]>) -> Self {
        Self { rows }
    }
}

impl Render for ConnectionDetailsTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut grid = div().flex().flex_col().gap_1();
        for (label, value) in self.rows.iter() {
            grid = grid.child(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .child(
                        div()
                            .w(px(60.))
                            .flex_none()
                            .text_size(px(11.))
                            .text_color(rgb(0x8f98aa))
                            .child(*label),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .text_size(px(11.))
                            .text_color(rgb(0xe5edf7))
                            .child(value.clone()),
                    ),
            );
        }

        div()
            .w(px(228.))
            .rounded_md()
            .border_1()
            .border_color(rgb(0x334155))
            .bg(rgba(0x151b24f2))
            .shadow_lg()
            .px_3()
            .py_2()
            .child(grid)
    }
}

pub(in crate::features::pages::connections) fn connection_section(
    snapshot: &ConnectionListSnapshot,
    section: ConnectionSectionHeader,
    cx: &mut Context<ConnectionPanel>,
) -> impl IntoElement {
    // The flat list only ever emits headers for real groups; root sections are
    // spread into `Connection` rows by `flatten_connection_rows`. The nested
    // body this used to draw under a header went away with virtualization.
    debug_assert!(!section.is_root, "root sections do not become header rows");
    let palette = snapshot.chrome.palette;
    let expanded = snapshot.group_is_expanded(section.group_id.as_deref());
    let group_id = section.group_id.clone();
    let group_label = section.label.clone();
    let count = section.total_count;
    let editing_group = section
        .group_id
        .as_deref()
        .is_some_and(|id| snapshot.group_editor_is_renaming(id));
    let editor_error = editing_group
        .then(|| {
            snapshot
                .group_editor
                .as_ref()
                .and_then(|editor| editor.error.clone())
        })
        .flatten();
    let group_header = div()
        .id(SharedString::from(format!(
            "connection-section-{}",
            section.group_id.clone().unwrap_or_else(|| "root".into())
        )))
        .relative()
        .h(px(28.))
        .min_w(relative(1.))
        .flex()
        .items_center()
        .gap(px(6.))
        .px_2()
        .pl(px(8. + section.depth as f32 * 16.))
        .rounded_sm()
        .cursor_pointer()
        .bg({
            let drop_inside = snapshot.drop_position_for_kind_target(
                ConnectionDragKind::Group,
                section.group_id.as_deref(),
            ) == Some(ConnectionDropPosition::Inside);
            if drop_inside || snapshot.group_is_hovered(section.group_id.as_deref()) {
                rgb(palette.hover)
            } else {
                rgba(0x00000000)
            }
        })
        .when(
            snapshot.drop_position_for_kind_target(
                ConnectionDragKind::Group,
                section.group_id.as_deref(),
            ) == Some(ConnectionDropPosition::Inside),
            |this| {
                this.border_1()
                    .border_color(rgb(snapshot.chrome.palette.link))
            },
        )
        .on_hover({
            let hover_group = section.group_id.clone();
            cx.listener(move |panel, hovered: &bool, _, cx| {
                panel.with_app(cx, |this, cx| {
                    if let Some(group_id) = hover_group.clone()
                        && this
                            .connection_state
                            .set_list_group_hover(group_id, *hovered)
                    {
                        cx.notify();
                    }
                })
            })
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _, _, cx| cx.stop_propagation()),
        )
        // Aims the list's one context menu at this group. Capture, so it runs
        // before the menu is built and regardless of who stops the bubble.
        .capture_any_mouse_down({
            let menu_group_id = section.group_id.clone().unwrap_or_default();
            cx.listener(move |panel, event: &MouseDownEvent, _, cx| {
                panel.with_app(cx, |this, _cx| {
                    if event.button == MouseButton::Right {
                        this.connection_state
                            .prepare_list_group_context_menu(menu_group_id.clone());
                    }
                })
            })
        })
        .when_some(
            (!editing_group)
                .then_some(section.group_id.clone())
                .flatten(),
            |this, drag_group_id| {
                let drop_group_id = drag_group_id.clone();
                let label = section.label.clone();
                this.cursor_move()
                    .on_drag(
                        ConnectionDragPayload {
                            kind: ConnectionDragKind::Group,
                            id: drag_group_id.clone(),
                            label,
                        },
                        |payload, position, _, cx| {
                            cx.new(|_| ConnectionDragPreview::new(payload.clone(), position))
                        },
                    )
                    .on_drag_move(cx.listener({
                        let target_id = drop_group_id.clone();
                        move |panel, event: &gpui::DragMoveEvent<ConnectionDragPayload>, _, cx| {
                            panel.with_app(cx, |this, cx| {
                                let _ = event.drag(cx);
                                let y = event.event.position.y;
                                let bounds = event.bounds;
                                let rel = if bounds.size.height > px(0.) {
                                    ((y - bounds.origin.y) / bounds.size.height).clamp(0., 1.)
                                } else {
                                    0.5
                                };
                                let position = if rel < 0.25 {
                                    ConnectionDropPosition::Before
                                } else if rel > 0.75 {
                                    ConnectionDropPosition::After
                                } else {
                                    ConnectionDropPosition::Inside
                                };
                                let next = ConnectionDropTarget {
                                    id: Some(target_id.clone()),
                                    kind: ConnectionDragKind::Group,
                                    position,
                                };
                                if this.connection_state.set_list_drop_target_if_changed(next) {
                                    cx.notify();
                                }
                            })
                        }
                    }))
                    .on_drop(
                        cx.listener(move |panel, payload: &ConnectionDragPayload, _, cx| {
                            panel.with_app(cx, |this, cx| {
                                let position = this.connection_state.list_drop_position_for_target(
                                    &drop_group_id,
                                    ConnectionDropPosition::Inside,
                                );
                                this.connection_state.clear_list_drop_target();
                                match payload.kind {
                                    ConnectionDragKind::Connection => {
                                        this.move_connection_into_group(
                                            payload.id.clone(),
                                            Some(drop_group_id.clone()),
                                            cx,
                                        );
                                    }
                                    ConnectionDragKind::Group => match position {
                                        ConnectionDropPosition::Inside => {
                                            this.move_group_into_group(
                                                payload.id.clone(),
                                                Some(drop_group_id.clone()),
                                                cx,
                                            );
                                        }
                                        _ => {
                                            this.move_group_before(
                                                payload.id.clone(),
                                                drop_group_id.clone(),
                                                cx,
                                            );
                                        }
                                    },
                                }
                            })
                        }),
                    )
            },
        )
        .on_click(cx.listener(move |panel, _, _, cx| {
            panel.with_app(cx, |this, cx| {
                cx.stop_propagation();
                if editing_group {
                    return;
                }
                if let Some(group_id) = group_id.clone() {
                    this.toggle_connection_group_expanded(group_id, cx);
                }
            })
        }))
        .on_key_down(cx.listener(|panel, event: &KeyDownEvent, _, cx| {
            panel.with_app(cx, |this, cx| {
                this.handle_connection_group_editor_key_down(event, cx);
            })
        }))
        // The name takes the slack so the count sits against the right
        // edge of the panel, where Tauri puts it.
        .child(
            svg()
                .size(px(14.))
                .flex_none()
                .path(if expanded {
                    "icons/chevron-down.svg"
                } else {
                    "icons/fe/forward.svg"
                })
                .text_color(rgb(palette.text_muted)),
        )
        .child(connection_type_icon(
            palette,
            resolve_connection_icon(Some("folder"), "SSH"),
            false,
            16.,
        ))
        .child(if editing_group {
            connection_group_editor_input_box(snapshot, cx)
        } else {
            div()
                .min_w_0()
                .flex_1()
                .text_xs()
                .font_weight(FontWeight(500.))
                .text_color(rgb(palette.text_muted))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(group_label.clone())
                .into_any_element()
        })
        .child(
            div()
                .flex_none()
                .text_xs()
                .text_color(rgb(if editor_error.is_some() {
                    palette.danger
                } else {
                    palette.text_dimmed
                }))
                .child(editor_error.unwrap_or_else(|| count.to_string())),
        );
    div().flex().flex_col().child(group_header)
}

pub(in crate::features::pages::connections) fn connection_inline_group_editor_row(
    snapshot: &ConnectionListSnapshot,
    parent_id: Option<String>,
    depth: usize,
    cx: &mut Context<ConnectionPanel>,
) -> impl IntoElement {
    let palette = snapshot.chrome.palette;
    let editor_error = snapshot
        .group_editor
        .as_ref()
        .and_then(|editor| editor.error.clone());
    div()
        .id(SharedString::from(format!(
            "connection-inline-group-editor-{}",
            parent_id.unwrap_or_else(|| "root".to_string())
        )))
        .relative()
        .h(px(28.))
        .min_w(relative(1.))
        .flex()
        .items_center()
        .gap(px(6.))
        .px_2()
        .pl(px(connection_tree_indent_px(depth)))
        .rounded_sm()
        .bg(rgb(palette.hover))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _, _, cx| cx.stop_propagation()),
        )
        .on_key_down(cx.listener(|panel, event: &KeyDownEvent, _, cx| {
            panel.with_app(cx, |this, cx| {
                this.handle_connection_group_editor_key_down(event, cx);
            })
        }))
        .child(
            svg()
                .size(px(14.))
                .flex_none()
                .path("icons/fe/forward.svg")
                .text_color(rgb(palette.text_muted)),
        )
        .child(connection_type_icon(
            palette,
            resolve_connection_icon(Some("folder"), "SSH"),
            false,
            16.,
        ))
        .child(connection_group_editor_input_box(snapshot, cx))
        .when_some(editor_error, |this, error| {
            this.child(
                div()
                    .flex_none()
                    .text_size(px(11.))
                    .text_color(rgb(palette.danger))
                    .child(error),
            )
        })
}

fn connection_group_editor_input_box(
    snapshot: &ConnectionListSnapshot,
    cx: &mut Context<ConnectionPanel>,
) -> AnyElement {
    let palette = snapshot.chrome.palette;
    let Some(field) = snapshot.group_editor_field.clone() else {
        return div().min_w_0().flex_1().into_any_element();
    };
    let handle = field.read(cx).focus_handle();
    let focused = field.read(cx).has_focus();
    let has_error = snapshot
        .group_editor
        .as_ref()
        .is_some_and(|editor| editor.error.is_some());
    div()
        .h(px(24.))
        .min_w_0()
        .flex_1()
        .px(px(ORDINARY_INPUT_SHELL_PADDING_X_PX))
        .flex()
        .items_center()
        .rounded_sm()
        .border_1()
        .border_color(if has_error {
            rgb(palette.danger)
        } else {
            ordinary_input_shell_border_color(palette, focused)
        })
        .when(focused, |this| {
            this.shadow(ordinary_input_focus_ring(palette))
        })
        .bg(rgb(palette.input))
        .cursor_text()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |_, _, window, cx| {
                cx.stop_propagation();
                window.focus(&handle, cx);
            }),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .text_xs()
                .text_color(rgb(palette.text))
                .child(NyaInput::new(&field)),
        )
        .into_any_element()
}

pub(in crate::features::pages::connections) fn saved_connection_row(
    snapshot: &ConnectionListSnapshot,
    connection: SavedConnection,
    depth: usize,
    cx: &mut Context<ConnectionPanel>,
) -> impl IntoElement {
    let selected = snapshot.is_selected(&connection.id);
    // The arrow keys walk filtered results without disturbing the selection,
    // so the active row gets its own fainter wash plus a ring.
    let keyboard_active = snapshot.is_keyboard_active(&connection.id);
    let connect_connection_dbl = connection.clone();
    let select_id = connection.id.clone();
    let menu_id = connection.id.clone();
    let kind = connection.kind_label();
    let icon_def = resolve_connection_icon(connection.icon.as_deref(), kind);
    let details_rows: Arc<[(&'static str, String)]> =
        connection_detail_rows(&connection, &snapshot.connections_by_id, &snapshot.proxies).into();
    let drop_position = snapshot
        .drop_position_for_kind_target(ConnectionDragKind::Connection, Some(&connection.id));
    let show_before = drop_position == Some(ConnectionDropPosition::Before);
    let show_after = drop_position == Some(ConnectionDropPosition::After);
    let show_inside = drop_position == Some(ConnectionDropPosition::Inside);
    let row_id = connection.id.clone();
    // Tauri ConnectionItem: py-1.5 single-line row (~34px) with hover actions.
    let palette = snapshot.chrome.palette;
    let row_selector = format!("connection-row-{}", connection.id);
    let name_selector = format!("connection-row-name-{}", connection.id);
    div()
        .id(SharedString::from(row_selector.clone()))
        .debug_selector(move || row_selector.clone())
        .relative()
        .h(px(34.))
        // The list scrolls sideways, so a row is at least the panel width and
        // grows past it when the name is long.
        .min_w(relative(1.))
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .pl(px(connection_tree_indent_px(depth)))
        .bg(if selected {
            rgba((palette.primary << 8) | 0x1a)
        } else if keyboard_active {
            rgba((palette.primary << 8) | 0x12)
        } else if show_inside {
            rgb(palette.hover)
        } else {
            rgba(0x00000000)
        })
        .when(keyboard_active && !selected, |this| {
            this.border_1().border_color(rgb(palette.primary))
        })
        .hover(move |this| this.bg(rgb(palette.hover)))
        .when(show_inside, |this| {
            this.border_1().border_color(rgb(palette.link))
        })
        .cursor_pointer()
        .cursor_move()
        .on_drag(
            ConnectionDragPayload {
                kind: ConnectionDragKind::Connection,
                id: connection.id.clone(),
                label: connection.name.clone(),
            },
            |payload, position, _, cx| {
                cx.new(|_| ConnectionDragPreview::new(payload.clone(), position))
            },
        )
        .on_drag_move(cx.listener({
            let target_id = row_id.clone();
            move |panel, event: &gpui::DragMoveEvent<ConnectionDragPayload>, _, cx| {
                panel.with_app(cx, |this, cx| {
                    let _payload = event.drag(cx);
                    let y = event.event.position.y;
                    let bounds = event.bounds;
                    let rel = if bounds.size.height > px(0.) {
                        ((y - bounds.origin.y) / bounds.size.height).clamp(0., 1.)
                    } else {
                        0.5
                    };
                    let position = if rel < 0.33 {
                        ConnectionDropPosition::Before
                    } else if rel > 0.66 {
                        ConnectionDropPosition::After
                    } else {
                        // Mid band: treat as before for connections (reorder only).
                        ConnectionDropPosition::Before
                    };
                    let next = ConnectionDropTarget {
                        id: Some(target_id.clone()),
                        kind: ConnectionDragKind::Connection,
                        position,
                    };
                    if this.connection_state.set_list_drop_target_if_changed(next) {
                        cx.notify();
                    }
                })
            }
        }))
        .on_drop({
            let target_id = connection.id.clone();
            cx.listener(move |panel, payload: &ConnectionDragPayload, _, cx| {
                panel.with_app(cx, |this, cx| {
                    let position = this
                        .connection_state
                        .list_drop_position_for_target(&target_id, ConnectionDropPosition::Before);
                    this.connection_state.clear_list_drop_target();
                    match payload.kind {
                        ConnectionDragKind::Connection => match position {
                            ConnectionDropPosition::After => {
                                this.move_connection_after(
                                    payload.id.clone(),
                                    target_id.clone(),
                                    cx,
                                );
                            }
                            _ => {
                                this.move_connection_before(
                                    payload.id.clone(),
                                    target_id.clone(),
                                    cx,
                                );
                            }
                        },
                        ConnectionDragKind::Group => {
                            let parent = this
                                .connection_state
                                .connections()
                                .iter()
                                .find(|c| c.id == target_id)
                                .and_then(|c| c.group_id.clone());
                            this.move_group_into_group(payload.id.clone(), parent, cx);
                        }
                    }
                })
            })
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _, _, cx| {
                cx.stop_propagation();
            }),
        )
        // Aims the list's one context menu at this row. Capture, so it runs
        // before the menu is built and regardless of who stops the bubble.
        .capture_any_mouse_down(cx.listener(move |panel, event: &MouseDownEvent, _, cx| {
            panel.with_app(cx, |this, cx| {
                if event.button == MouseButton::Right {
                    this.prepare_connection_context_menu(menu_id.clone(), cx);
                }
            })
        }))
        .on_click(
            cx.listener(move |panel, event: &gpui::ClickEvent, window, cx| {
                panel.with_app(cx, |this, cx| {
                    cx.stop_propagation();
                    if event.click_count() >= 2 {
                        this.start_saved_connection(connect_connection_dbl.clone(), window, cx);
                        return;
                    }
                    let modifiers = event.modifiers();
                    let additive = modifiers.control || modifiers.platform;
                    let range = modifiers.shift;
                    this.select_connection(select_id.clone(), additive, range, cx);
                })
            }),
        )
        // Single-line name; the detail card is a real tooltip so it can hang
        // outside the panel instead of covering the rows underneath it.
        .child(connection_type_icon(palette, icon_def, selected, 16.))
        .child(
            div()
                .id(SharedString::from(name_selector.clone()))
                .debug_selector(move || name_selector.clone())
                .flex_none()
                // Match Tauri's `pr-14`: at maximum horizontal scroll the
                // final glyph can move clear of the viewport-fixed actions.
                .pr(px(CONNECTION_ACTION_CLEARANCE_PX))
                .text_size(px(12.))
                .font_weight(FontWeight(500.))
                .text_color(if selected {
                    rgb(palette.link)
                } else {
                    rgb(palette.text)
                })
                // Full name, never clipped — the list scrolls to reach it.
                .whitespace_nowrap()
                .child(connection.name.clone())
                .tooltip(move |_, cx| {
                    cx.new(|_| ConnectionDetailsTooltip::new(details_rows.clone()))
                        .into()
                }),
        )
        .child(div().flex_1().min_w_0())
        .when(show_before, |this| {
            this.child(
                div()
                    .absolute()
                    .left(px(8.))
                    .right(px(8.))
                    .top_0()
                    .h(px(2.))
                    .rounded_full()
                    .bg(rgb(palette.link)),
            )
        })
        .when(show_after, |this| {
            this.child(
                div()
                    .absolute()
                    .left(px(8.))
                    .right(px(8.))
                    .bottom_0()
                    .h(px(2.))
                    .rounded_full()
                    .bg(rgb(palette.link)),
            )
        })
}
