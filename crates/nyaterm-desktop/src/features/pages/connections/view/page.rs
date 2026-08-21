use rust_i18n::t;

use std::time::Duration;
use std::time::Instant;

use gpui::{
    AnyElement, Context, IntoElement, KeyDownEvent, ListHorizontalSizingBehavior, MouseButton,
    MouseDownEvent, SharedString, UniformListScrollHandle, div,
    prelude::{InteractiveElement, ParentElement, StatefulInteractiveElement, Styled},
    px, rgb, svg, uniform_list,
};
use nyaterm_ui::{
    NyaContextMenu, NyaDropdownMenu, NyaScrollable, NyaScrollbarAxis, NyaSearchInput,
};

use crate::features::{
    NyaTermApp, connections::ConnectionDragKind, connections::ConnectionDragPayload,
    perf::record_gpui_perf_sample,
};
use crate::models::ConnectionSortMode;

use super::super::list::{
    ConnectionListRow, connection_tree_indent_px, icon_action_button, icon_action_button_styled,
};

const CONNECTION_LIST_ROW_HEIGHT_PX: f32 = 34.;

impl NyaTermApp {
    pub(in crate::features) fn connections_view(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let started_at = Instant::now();
        let model = self.connection_state.connection_list_model();
        let model_stats = model.stats;
        let perf_context =
            self.gpui_perf_context(model_stats.flat_row_count, Some(model_stats.cache_hit));
        record_gpui_perf_sample(
            "connection_sections",
            Duration::from_secs_f64(model_stats.sections_ms / 1000.0),
            perf_context,
        );
        record_gpui_perf_sample(
            "flatten_connection_rows",
            Duration::from_secs_f64(model_stats.flatten_ms / 1000.0),
            perf_context,
        );
        record_gpui_perf_sample(
            "widest_connection_row",
            Duration::from_secs_f64(model_stats.widest_ms / 1000.0),
            perf_context,
        );
        let empty_connections_label = t!("savedConnections.empty");
        let empty_connections_hint = t!("savedConnections.emptyHint");
        let no_results_label = t!("savedConnections.noResults");
        let empty_group_label = t!("savedConnections.emptyGroup");

        // Keep the flattened model cheap to rebuild, then let GPUI instantiate only
        // the rows intersecting the scroll viewport.
        let flat_rows = model.rows;
        // A folder is worth showing even before anything is filed under it, so the
        // empty state waits until there are no folders either. Otherwise a freshly
        // created folder is swallowed by "no saved connections".
        let store_is_empty = self.connection_state.connections().is_empty()
            && self.connection_state.groups().is_empty()
            && self.connection_state.active_group_editor_draft().is_none();
        let nothing_matched = flat_rows.is_empty();
        let palette = self.theme_palette();

        let list_scroll = window
            .use_keyed_state(
                SharedString::from("connections-list-scroll-handle"),
                cx,
                |_, _| UniformListScrollHandle::new(),
            )
            .read(cx)
            .clone();

        let mut list = div()
            .id(SharedString::from("connections-list-scroll"))
            .relative()
            .flex_1()
            .min_h_0()
            .p(px(6.))
            .flex()
            .flex_col()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    // Click empty background clears multi-select (Tauri list onMouseDown).
                    if this.connection_state.list_has_selection() {
                        this.clear_selected_connections(cx);
                    }
                }),
            )
            .on_drop(cx.listener(|this, payload: &ConnectionDragPayload, _, cx| {
                this.connection_state.clear_list_drop_target();
                match payload.kind {
                    ConnectionDragKind::Connection => {
                        this.move_connection_into_group(payload.id.clone(), None, cx);
                    }
                    ConnectionDragKind::Group => {
                        this.move_group_into_group(payload.id.clone(), None, cx);
                    }
                }
            }));
        if store_is_empty {
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .px_4()
                    .py_8()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(palette.text_muted))
                            .child(empty_connections_label),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(palette.text_dimmed))
                            .child(empty_connections_hint),
                    ),
            );
        } else if nothing_matched {
            list = list.child(
                div()
                    .px_4()
                    .py_8()
                    .text_size(px(11.))
                    .text_color(rgb(palette.text_dimmed))
                    .child(no_results_label),
            );
        } else {
            let row_count = flat_rows.len();
            // `uniform_list` derives its scrollable width from one measured row, so
            // point it at the row most likely to be the widest or long names would
            // still be unreachable.
            let widest_row = model.widest_row;
            list = list.child(
                uniform_list(
                    "connections-list-rows",
                    row_count,
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        let mut items = Vec::with_capacity(range.len());
                        for index in range {
                            let Some(row) = flat_rows.get(index).cloned() else {
                                continue;
                            };
                            // A definite width here, so the rows inside can resolve
                            // their `min_w(relative(1.))` and still overflow it when
                            // the name is long. It tracks the horizontal scroll.
                            let item = div()
                                .h(px(CONNECTION_LIST_ROW_HEIGHT_PX))
                                .w_full()
                                .flex_none()
                                .flex()
                                .items_center();
                            items.push(match row {
                                ConnectionListRow::Separator => item
                                    .child(div().mx_2().h(px(1.)).w_full().bg(rgb(palette.border))),
                                ConnectionListRow::GroupHeader(section) => item.child(
                                    div()
                                        .w_full()
                                        .child(this.connection_section(section, true, cx)),
                                ),
                                ConnectionListRow::InlineGroupEditor { parent_id, depth } => item
                                    .child(div().w_full().child(
                                        this.connection_inline_group_editor_row(
                                            parent_id, depth, cx,
                                        ),
                                    )),
                                ConnectionListRow::EmptyGroup { depth } => item.child(
                                    div()
                                        .w_full()
                                        .px_2()
                                        .pl(px(connection_tree_indent_px(depth)))
                                        .h(px(28.))
                                        .flex()
                                        .items_center()
                                        .text_size(px(11.))
                                        .text_color(rgb(palette.text_dimmed))
                                        .child(empty_group_label.clone()),
                                ),
                                ConnectionListRow::Connection {
                                    connection_id,
                                    depth,
                                } => {
                                    let Some(connection) = this
                                        .connection_state
                                        .connection_by_id(&connection_id)
                                        .cloned()
                                    else {
                                        continue;
                                    };
                                    item.child(
                                        div().w_full().child(
                                            this.saved_connection_row(connection, depth, cx),
                                        ),
                                    )
                                }
                            });
                        }
                        items
                    }),
                )
                .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
                .with_width_from_item(widest_row)
                .flex_1()
                .min_h_0()
                .track_scroll(&list_scroll),
            );
        }

        // Both axes ride this one handle: rows are `Unconstrained` and their names
        // are never clipped, so a long or deeply nested name overflows sideways.
        // One `Scrollbar` for both is the right shape here - a shared reveal state
        // matches a single scroll surface, and the vendor skips painting an axis
        // whose content fits.
        let list = list.scrollbar(&list_scroll, NyaScrollbarAxis::Both);
        // One context menu for the whole list, aimed by whatever the press landed
        // on. Rows and group headers re-aim it from their own capture handlers,
        // which run inside this one, so the reset here is what a press on empty
        // space keeps. Nesting a menu per row instead opens both menus on a single
        // right-click, and the one that never receives the click goes on
        // re-focusing itself every layout pass - stranding any dialog opened
        // afterwards, because a dialog is dismissed through actions routed along
        // the focused element's path.
        let menu_app = cx.entity();
        let list = NyaContextMenu::new_dynamic(
            list.capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, _, _| {
                if event.button == MouseButton::Right {
                    this.connection_state.prepare_list_context_menu();
                }
            })),
            move |_, cx| {
                menu_app.update(cx, |this, cx| {
                    this.connection_list_target_context_menu_items(cx)
                })
            },
        );

        // Tauri: PanelHeader (shared stack) + search/action strip + flat tree list.
        // Count is shown in the shared panel header via meta; strip hosts search + icons.
        let output = div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .bg(self.shell_transparent_color(palette.surface))
            .child(self.connections_search_bar(window, cx))
            .child(list);
        record_gpui_perf_sample("connections_view", started_at.elapsed(), perf_context);
        output.into_any_element()
    }

    pub(in crate::features) fn connections_search_bar(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let search_empty = self.connection_state.list_search_is_empty();
        let search_field = self.connection_state.list_search_field();
        // Tauri swaps the glyph, flips it for Z-A and tints it while a name sort is
        // active, so the current mode is readable without hovering for the tooltip.
        let sort_mode = self.connection_state.list_sort_mode();
        let sort_label = match sort_mode {
            ConnectionSortMode::Default => "icons/conn/sort.svg",
            ConnectionSortMode::NameAsc | ConnectionSortMode::NameDesc => {
                "icons/conn/sort-alpha.svg"
            }
        };
        let sort_tint = (sort_mode != ConnectionSortMode::Default).then_some(palette.primary);
        let sort_flipped = sort_mode == ConnectionSortMode::NameDesc;
        let sort_tooltip = t!(match sort_mode {
            ConnectionSortMode::Default => "savedConnections.sortDefault",
            ConnectionSortMode::NameAsc => "savedConnections.sortNameAsc",
            ConnectionSortMode::NameDesc => "savedConnections.sortNameDesc",
        });
        let more_menu = NyaDropdownMenu::new("connections-more")
            .icon("icons/conn/more.svg")
            .icon_size(px(14.))
            .tooltip(t!("common.more"))
            .min_width(px(180.))
            .items(self.connection_more_menu_items(cx))
            .on_trigger(|_, _, cx| cx.stop_propagation());
        let mut search_input = NyaSearchInput::new("connection-search-input", &search_field)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_connection_search_key_down(event, window, cx);
            }));
        if !search_empty {
            search_input = search_input.trailing(
                div()
                    .id(SharedString::from("connection-search-clear"))
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .text_size(px(10.))
                    .text_color(rgb(palette.text_muted))
                    .cursor_pointer()
                    .hover(move |this| {
                        this.bg(rgb(palette.surface_elevated))
                            .text_color(rgb(palette.text))
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.clear_connection_search(window, cx);
                    }))
                    .child(
                        svg()
                            .size(px(13.))
                            .path("icons/window/close.svg")
                            .text_color(rgb(palette.text_muted)),
                    ),
            );
        }

        // Tauri search strip: px-2 py-1.5, input h-7.
        div()
            .h(px(36.))
            .px_2()
            .flex()
            .items_center()
            .gap_1()
            .border_b_1()
            .border_color(rgb(palette.border))
            .bg(self.shell_transparent_color(palette.section_header))
            .child(div().flex_1().min_w_0().child(search_input))
            // Count lives in PanelHeader (Tauri).
            .child(icon_action_button_styled(
                palette,
                "connections-sort",
                sort_label,
                sort_tooltip,
                sort_tint,
                sort_flipped,
                cx.listener(|this, _, _, cx| {
                    this.cycle_connection_sort_mode(cx);
                }),
            ))
            .child(icon_action_button(
                palette,
                "connections-temp-ssh",
                "icons/conn/flash.svg",
                t!("temporarySsh.title"),
                cx.listener(|this, _, window, cx| {
                    this.open_temporary_ssh_link_dialog(window, cx);
                }),
            ))
            .child(icon_action_button(
                palette,
                "connections-new-group",
                "icons/fe/new-folder.svg",
                t!("savedConnections.newFolder"),
                cx.listener(|this, _, window, cx| {
                    this.open_connection_group_editor(None, None, window, cx);
                }),
            ))
            .child(icon_action_button(
                palette,
                "connections-new",
                "icons/conn/add.svg",
                t!("savedConnections.newConnection"),
                cx.listener(|this, _, window, cx| {
                    this.open_connection_editor(None, None, false, window, cx);
                }),
            ))
            .child(more_menu)
    }
}
