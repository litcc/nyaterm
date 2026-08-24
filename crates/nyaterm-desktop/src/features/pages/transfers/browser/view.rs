use rust_i18n::t;

use gpui::{
    AnyElement, Context, IntoElement, KeyDownEvent, ListHorizontalSizingBehavior, MouseButton,
    MouseDownEvent, SharedString, div, prelude::*, px, rgb, rgba, svg, uniform_list,
};
use nyaterm_core::truncate_preview;
use nyaterm_ui::{NyaContextMenu, NyaHorizontalScrollbar, NyaSearchInput, NyaUniformListScrollbar};

use crate::features::{NyaTermApp, text_inputs::TextInputSetup, transfers::format_file_size};
use crate::models::TransferBrowserSortColumn;

use super::super::browser_filter::transfer_browser_footer_stats;
use super::super::{
    FILE_BROWSER_HEADER_HEIGHT_PX, TransferBrowserAvailability,
    TransferBrowserEntryRowPresentation, TransferBrowserSortHeaderState,
    normalized_transfer_browser_path, sort_header_cell, transfer_browser_entry_row,
    transfer_browser_parent_entry_row, transfer_browser_table_width,
};
use super::helpers::{
    compact_transfer_footer_button, compact_transfer_footer_button_active,
    compact_transfer_toolbar_button, compact_transfer_toolbar_button_active,
    compact_transfer_toolbar_button_enabled, compact_transfer_upload_menu_button,
    transfer_toolbar_divider,
};

const FILE_BROWSER_SCROLLBAR_SIZE_PX: f32 = 16.;

impl NyaTermApp {
    pub(in crate::features::pages::transfers) fn transfer_browser_view(
        &mut self,
        availability: TransferBrowserAvailability,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let transparent_surface = self.shell_transparent_color(palette.surface);
        let section_header = self.shell_transparent_color(palette.section_header);
        if availability != TransferBrowserAvailability::Browsable {
            let (title, description) = match availability {
                TransferBrowserAvailability::UnsupportedSession => (
                    t!("fileExplorer.unsupportedSession"),
                    Some(t!("fileExplorer.unsupportedSessionDesc")),
                ),
                TransferBrowserAvailability::NoSession
                | TransferBrowserAvailability::DisconnectedSsh => {
                    (t!("fileExplorer.connectToSession"), None)
                }
                TransferBrowserAvailability::Browsable => unreachable!(),
            };

            return div()
                .id(SharedString::from("transfer-browser-panel"))
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .bg(transparent_surface)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .px_4()
                        .gap_1()
                        .text_center()
                        .child(
                            svg()
                                .size(px(28.))
                                .flex_none()
                                .path("icons/fe/folder-off.svg")
                                .text_color(rgb(palette.text_dimmed)),
                        )
                        .child(
                            div()
                                .mt_1()
                                .text_size(px(12.))
                                .text_color(rgb(palette.text_muted))
                                .child(title),
                        )
                        .when_some(description, |this, description| {
                            this.child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(palette.text_dimmed))
                                    .child(description),
                            )
                        }),
                );
        }

        let can_transfer = availability == TransferBrowserAvailability::Browsable;
        let _selected = self
            .transfer
            .browser_view()
            .selected_remote_path
            .as_deref()
            .map(|path| truncate_preview(path, 56))
            .unwrap_or_else(|| "none".to_string());
        let visible_entries = self.visible_transfer_browser_entries();
        let column_widths = self.transfer.browser_view().column_widths;
        let table_width = transfer_browser_table_width(column_widths);
        let resizing_column = self
            .transfer
            .browser_view()
            .column_resize
            .map(|state| state.column);
        let sort_header_state = TransferBrowserSortHeaderState {
            header_bg: section_header,
            active_column: self.transfer.browser_view().sort_column,
            direction: self.transfer.browser_view().sort_direction,
            resizing_column,
        };
        let show_hidden_files = self.settings.summary().ui_file_explorer_show_hidden_files;
        let browser = self.transfer.browser_view();
        let footer_stats = transfer_browser_footer_stats(
            browser.entries,
            &visible_entries,
            browser.selected_remote_path.as_deref(),
            browser.selected_remote_paths,
            show_hidden_files,
        );
        let footer_size_text =
            if footer_stats.selected_item_count > 0 && footer_stats.selected_file_size > 0 {
                format!(
                    "{}/{}",
                    format_file_size(Some(footer_stats.selected_file_size)),
                    format_file_size(Some(footer_stats.total_file_size))
                )
            } else {
                format_file_size(Some(footer_stats.total_file_size))
            };
        let search_active = !self.transfer.browser_view().search.trim().is_empty();
        let search_expanded = self.transfer.browser_view().search_expanded || search_active;
        let app = cx.entity();
        // The field is built where the search is revealed, not here: this is a
        // render path, and `text_input` would create it on the first frame.
        let search_input = search_expanded
            .then(|| self.existing_text_input("transfer.browser.search"))
            .flatten()
            .map(|field| {
                div()
                    .h_full()
                    .flex_1()
                    .min_w_0()
                    .px_1()
                    .flex()
                    .items_center()
                    .child(
                        NyaSearchInput::new("transfer-browser-search", &field).on_key_down(
                            cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                if event.keystroke.key == "escape" {
                                    cx.stop_propagation();
                                    this.clear_or_close_transfer_browser_search(window, cx);
                                }
                            }),
                        ),
                    )
                    .into_any_element()
            });
        let current_browser_path =
            normalized_transfer_browser_path(self.transfer.browser_view().path);
        let has_parent_entry =
            can_transfer && current_browser_path != "/" && current_browser_path != ".";
        let show_list_scrollbar =
            can_transfer && !self.transfer.browser_view().loading && !visible_entries.is_empty();
        let auto_sync_cwd = self.transfer_browser_auto_sync_cwd_enabled();
        let cwd_tracking_available = self.active_transfer_browser_connection_id().is_some();
        let external_drop_hover = self.transfer.browser_view().external_drop_hover;
        let rows: AnyElement = if self.transfer.browser_view().loading {
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .px_4()
                        .py_8()
                        .text_size(px(12.))
                        .text_color(rgb(palette.text_dimmed))
                        .child(t!("fileExplorer.loading")),
                )
                .into_any_element()
        } else if self.transfer.browser_view().entries.is_empty() {
            if has_parent_entry && self.transfer.browser_view().error.is_none() {
                div()
                    .flex()
                    .flex_col()
                    .child(transfer_browser_parent_entry_row(
                        palette,
                        column_widths,
                        cx,
                    ))
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .px_4()
                            .py_8()
                            .gap_1()
                            .child(
                                if let Some(error) = self.transfer.browser_view().error.as_deref() {
                                    div()
                                        .text_size(px(12.))
                                        .text_color(rgb(palette.danger))
                                        .child(truncate_preview(error, 120))
                                } else {
                                    div()
                                        .text_size(px(12.))
                                        .text_color(rgb(palette.text_muted))
                                        .child(t!("fileExplorer.emptyDirectory"))
                                },
                            ),
                    )
                    .into_any_element()
            }
        } else if visible_entries.is_empty() {
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .px_4()
                        .py_8()
                        .text_size(px(11.))
                        .text_color(rgb(palette.text_dimmed))
                        .child(t!("fileExplorer.noSearchResults")),
                )
                .into_any_element()
        } else {
            let parent_count = usize::from(has_parent_entry);
            let total_entries = visible_entries.len() + parent_count;
            let name_placeholder = t!("fileExplorer.name");
            let mut list = uniform_list(
                "transfer-browser-rows",
                total_entries,
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    let renaming = this.transfer.rename_dialog().cloned();
                    let selected_remote_path =
                        this.transfer.browser_view().selected_remote_path.clone();
                    let selected_remote_paths =
                        this.transfer.browser_view().selected_remote_paths.clone();
                    let mut items = Vec::with_capacity(range.len());
                    for index in range {
                        if has_parent_entry && index == 0 {
                            items.push(
                                transfer_browser_parent_entry_row(palette, column_widths, cx)
                                    .into_any_element(),
                            );
                            continue;
                        }
                        let Some(entry) = visible_entries
                            .get(index.saturating_sub(parent_count))
                            .cloned()
                        else {
                            continue;
                        };
                        let rename_input = renaming
                            .as_ref()
                            .filter(|state| state.old_path == entry.path)
                            .map(|state| {
                                this.text_input_box(
                                    format!("transfer.rename.{}", state.old_path),
                                    &state.value,
                                    TextInputSetup::placeholder(name_placeholder.clone()),
                                    cx,
                                )
                                .into_any_element()
                            });
                        items.push(
                            transfer_browser_entry_row(
                                TransferBrowserEntryRowPresentation {
                                    palette,
                                    entry,
                                    selected_remote_path: selected_remote_path.clone(),
                                    selected_remote_paths: &selected_remote_paths,
                                    column_widths,
                                    rename_state: renaming.clone(),
                                    rename_input,
                                },
                                cx,
                            )
                            .into_any_element(),
                        );
                    }
                    items
                }),
            );
            list.style().restrict_scroll_to_axis = Some(true);
            list.h_full()
                .min_h_0()
                .min_w(table_width)
                .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::FitList)
                .track_scroll(self.transfer.browser_view().list_scroll)
                .into_any_element()
        };

        div()
            .id(SharedString::from("transfer-browser-panel"))
            .size_full()
            .flex()
            .flex_col()
            .relative()
            .overflow_hidden()
            .bg(transparent_surface)
            .track_focus(self.transfer.browser_view().focus)
            .can_drop(|drag, _, _| drag.is::<gpui::ExternalPaths>())
            .on_drag_move(cx.listener(
                |this, event: &gpui::DragMoveEvent<gpui::ExternalPaths>, _, cx| {
                    this.set_transfer_browser_external_drop_hover(
                        event.bounds.contains(&event.event.position),
                        cx,
                    );
                },
            ))
            .on_drop(cx.listener(|this, paths: &gpui::ExternalPaths, _, cx| {
                this.handle_transfer_browser_external_file_drop(paths.paths().to_vec(), cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_transfer_browser_key_down(event, window, cx);
            }))
            .when(can_transfer, |this| {
                this.child(
                    div()
                        .relative()
                        .h(px(36.))
                        .px_1()
                        .border_b_1()
                        .border_color(rgb(palette.border))
                        .bg(section_header)
                        .flex()
                        .items_center()
                        .gap(px(2.))
                        .child(compact_transfer_toolbar_button(
                            palette,
                            "transfer-browser-new-file",
                            "icons/fe/new-file.svg",
                            t!("fileExplorer.newFile"),
                            cx.listener(|this, _, window, cx| {
                                this.open_transfer_new_file_dialog(window, cx);
                            }),
                        ))
                        .child(compact_transfer_toolbar_button(
                            palette,
                            "transfer-browser-new-folder",
                            "icons/fe/new-folder.svg",
                            t!("fileExplorer.newFolder"),
                            cx.listener(|this, _, window, cx| {
                                this.open_transfer_new_folder_dialog(window, cx);
                            }),
                        ))
                        .child(transfer_toolbar_divider(palette))
                        .child(compact_transfer_upload_menu_button(
                            palette,
                            t!("fileExplorer.upload"),
                            cx,
                        ))
                        .child(compact_transfer_toolbar_button_enabled(
                            palette,
                            "transfer-browser-download-selected",
                            "icons/fe/download.svg",
                            t!("fileExplorer.downloadSelected"),
                            footer_stats.selected_item_count > 0,
                            cx.listener(|this, _, window, cx| {
                                this.start_selected_sftp_download_jobs(window, cx);
                            }),
                        ))
                        .child(compact_transfer_toolbar_button_enabled(
                            palette,
                            "transfer-browser-delete-selected",
                            "icons/fe/delete.svg",
                            t!("fileExplorer.delete"),
                            footer_stats.selected_item_count > 0,
                            cx.listener(|this, _, window, cx| {
                                this.open_selected_transfer_delete_dialog(window, cx);
                            }),
                        ))
                        .child(transfer_toolbar_divider(palette))
                        .child(compact_transfer_toolbar_button(
                            palette,
                            "transfer-browser-go-up",
                            "icons/fe/up.svg",
                            t!("fileExplorer.goUp"),
                            cx.listener(|this, _, window, cx| {
                                this.open_transfer_parent_directory(window, cx);
                            }),
                        ))
                        .child(compact_transfer_toolbar_button(
                            palette,
                            "transfer-browser-refresh",
                            "icons/fe/refresh.svg",
                            t!("fileExplorer.refresh"),
                            cx.listener(|this, _, window, cx| {
                                this.refresh_transfer_browser(window, cx);
                            }),
                        ))
                        .child(div().flex_1())
                        .child(compact_transfer_toolbar_button_active(
                            palette,
                            "transfer-browser-expand-search",
                            "icons/fe/search.svg",
                            t!("fileExplorer.search"),
                            search_active || search_expanded,
                            cx.listener(|this, _, window, cx| {
                                this.focus_transfer_browser_search(None, window, cx);
                            }),
                        ))
                        .child(compact_transfer_toolbar_button_active(
                            palette,
                            "transfer-browser-toggle-hidden-files",
                            "icons/eye.svg",
                            if show_hidden_files {
                                t!("fileExplorer.hideHiddenFiles")
                            } else {
                                t!("fileExplorer.showHiddenFiles")
                            },
                            show_hidden_files,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_transfer_browser_hidden_files(cx);
                            }),
                        ))
                        .when(search_expanded, |toolbar| {
                            toolbar.child(
                                div()
                                    .id(SharedString::from("transfer-browser-search-overlay"))
                                    .absolute()
                                    .top(px(2.))
                                    .bottom(px(2.))
                                    .left(px(4.))
                                    .right(px(4.))
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(0x388bfd))
                                    .bg(transparent_surface)
                                    .px_1()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        svg()
                                            .size(px(16.))
                                            .flex_none()
                                            .path("icons/fe/search.svg")
                                            .text_color(rgb(palette.link)),
                                    )
                                    .children(search_input)
                                    .child(
                                        div()
                                            .id(SharedString::from("transfer-browser-clear-search"))
                                            .size(px(20.))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_sm()
                                            .text_size(px(12.))
                                            .text_color(rgb(palette.text_muted))
                                            .cursor_pointer()
                                            .hover(|this| {
                                                this.bg(rgb(palette.surface_elevated))
                                                    .text_color(rgb(palette.text))
                                            })
                                            .child(
                                                svg()
                                                    .size(px(13.))
                                                    .path("icons/window/close.svg")
                                                    .text_color(rgb(palette.text_muted)),
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.clear_or_close_transfer_browser_search(
                                                    window, cx,
                                                );
                                            })),
                                    ),
                            )
                        }),
                )
                .child(self.transfer_browser_path_row(current_browser_path.clone(), cx))
            })
            .child(NyaContextMenu::new_dynamic(
                div()
                    .id(SharedString::from("transfer-browser-table-viewport"))
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_hidden()
                    .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        if event.button == MouseButton::Right {
                            this.begin_transfer_browser_context_menu(cx);
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, _: &MouseDownEvent, window, cx| {
                            this.prepare_transfer_browser_current_context_menu(window, cx);
                        }),
                    )
                    .child(
                        div()
                            .id(SharedString::from("transfer-browser-table-scroll"))
                            .size_full()
                            .min_w_0()
                            .overflow_x_scroll()
                            .overflow_y_hidden()
                            .restrict_scroll_to_axis()
                            .track_scroll(self.transfer.browser_view().horizontal_scroll)
                            .child(
                                div()
                                    .min_w(table_width)
                                    .h_full()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_0()
                                            .child(sort_header_cell(
                                                palette,
                                                TransferBrowserSortColumn::Name,
                                                t!("fileExplorer.name"),
                                                column_widths.name,
                                                sort_header_state,
                                                cx,
                                            ))
                                            .child(sort_header_cell(
                                                palette,
                                                TransferBrowserSortColumn::Modified,
                                                t!("fileExplorer.mtime"),
                                                column_widths.modified,
                                                sort_header_state,
                                                cx,
                                            ))
                                            .child(sort_header_cell(
                                                palette,
                                                TransferBrowserSortColumn::Size,
                                                t!("fileExplorer.size"),
                                                column_widths.size,
                                                sort_header_state,
                                                cx,
                                            ))
                                            .child(sort_header_cell(
                                                palette,
                                                TransferBrowserSortColumn::Permissions,
                                                t!("fileExplorer.permissions"),
                                                column_widths.permissions,
                                                sort_header_state,
                                                cx,
                                            ))
                                            .child(sort_header_cell(
                                                palette,
                                                TransferBrowserSortColumn::Owner,
                                                t!("fileExplorer.owner"),
                                                column_widths.owner,
                                                sort_header_state,
                                                cx,
                                            ))
                                            .child(sort_header_cell(
                                                palette,
                                                TransferBrowserSortColumn::Group,
                                                t!("fileExplorer.group"),
                                                column_widths.group,
                                                sort_header_state,
                                                cx,
                                            )),
                                    )
                                    .child(div().relative().flex_1().min_h_0().child(rows)),
                            ),
                    )
                    .when(show_list_scrollbar, |this| {
                        this.child(
                            // Spans the row viewport rather than just the track strip:
                            // the bar's hitbox is what hover-to-reveal watches, and it
                            // also makes the thumb proportional to the real viewport.
                            // `Scrollbar::vertical` still lays its track at the right edge.
                            div()
                                .absolute()
                                .top(px(FILE_BROWSER_HEADER_HEIGHT_PX))
                                .bottom_0()
                                .left_0()
                                .right_0()
                                .child(NyaUniformListScrollbar::new(
                                    "transfer-browser-vertical-scrollbar",
                                    self.transfer.browser_view().list_scroll,
                                )),
                        )
                    })
                    .child(
                        // Also viewport-spanning, and it keeps the header so hovering the
                        // column titles reveals the bar too. Inset by a full track width
                        // on the right: the two axes are independent `Scrollbar` elements,
                        // so the vendor's own corner-avoidance does not apply.
                        div()
                            .absolute()
                            .inset_0()
                            .right(px(FILE_BROWSER_SCROLLBAR_SIZE_PX))
                            .child(NyaHorizontalScrollbar::new(
                                "transfer-browser-horizontal-scrollbar",
                                self.transfer.browser_view().horizontal_scroll,
                            )),
                    ),
                move |_, cx| {
                    app.update(cx, |this, cx| this.transfer_browser_context_menu_items(cx))
                },
            ))
            // Tauri FileExplorer footer: totals left, cwd sync / send icons right.
            .child(
                div()
                    .h(px(28.))
                    .flex_none()
                    .px_2()
                    .border_t_1()
                    .border_color(rgb(palette.border))
                    .bg(transparent_surface)
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_3()
                            .text_size(px(11.))
                            .text_color(rgb(palette.text_muted))
                            .when(
                                !self.transfer.browser_view().loading
                                    && self.transfer.browser_view().error.is_none()
                                    && footer_stats.total_item_count > 0,
                                |this| {
                                    this.child(if footer_stats.selected_item_count > 0 {
                                        t!("fileExplorer.selectedItems", selected = footer_stats.selected_item_count.to_string(),, total = footer_stats.total_item_count.to_string(),)
                                    } else {
                                        t!("fileExplorer.totalItems", count = footer_stats.total_item_count.to_string(),)
                                    })
                                    .child(footer_size_text)
                                },
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_0()
                            .child(compact_transfer_footer_button(
                                palette,
                                "transfer-browser-footer-sync-cwd",
                                "icons/fe/folder-sync.svg",
                                if cwd_tracking_available {
                                    t!("fileExplorer.syncTerminalPath")
                                } else {
                                    t!("fileExplorer.cwdTrackingUnavailable")
                                },
                                cwd_tracking_available,
                                cx.listener(|this, _, _window, cx| {
                                    this.start_transfer_sync_cwd_job(cx);
                                }),
                            ))
                            .child(compact_transfer_footer_button_active(
                                palette,
                                "transfer-browser-footer-auto-sync",
                                "icons/fe/sync.svg",
                                if cwd_tracking_available {
                                    t!("fileExplorer.autoSyncTerminalPath")
                                } else {
                                    t!("fileExplorer.cwdTrackingUnavailable")
                                },
                                auto_sync_cwd,
                                cwd_tracking_available,
                                cx.listener(|this, _, _window, cx| {
                                    this.toggle_transfer_browser_auto_sync_cwd(cx);
                                }),
                            ))
                            .child(compact_transfer_footer_button(
                                palette,
                                "transfer-browser-footer-send-path",
                                "icons/fe/send-path.svg",
                                t!("fileExplorer.sendToTerminal"),
                                true,
                                cx.listener(|this, _, _, cx| {
                                    this.send_current_transfer_browser_path_to_terminal(cx);
                                }),
                            )),
                    ),
            )
            .when(external_drop_hover, |this| {
                this.child(
                    div()
                        .absolute()
                        .inset_2()
                        .rounded_lg()
                        .border_2()
                        .border_color(rgb(palette.link))
                        .bg(rgba(0x3b82f624))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .max_w(px(340.))
                                .rounded_lg()
                                .border_1()
                                .border_color(rgb(palette.link))
                                .bg(rgb(palette.surface))
                                .px_6()
                                .py_4()
                                .shadow_lg()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(gpui::FontWeight(700.))
                                        .text_color(rgb(palette.text))
                                        .child(t!("fileExplorer.externalDropOverlayTitle")),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(palette.text_muted))
                                        .child(t!("fileExplorer.externalDropOverlayHint")),
                                ),
                        ),
                )
            })
    }
}
