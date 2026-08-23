use std::time::{Duration, Instant};

use crate::models::{MainMode, NavItem, PanelResizeSide, PanelSide};
use gpui::{
    AnyElement, Context, Div, IntoElement, KeyDownEvent, MouseButton, MouseMoveEvent, MouseUpEvent,
    NavigationDirection, ObjectFit, Render, SharedString, Stateful, Window, canvas, deferred, div,
    img, prelude::*, px, rgb, rgba, svg,
};

use super::NyaTermApp;
use super::terminal::{FULL_SHELL_PAINT_COUNT, terminal_surface_paint_count};
use super::view_widgets::{
    full_window_input_layer, full_window_overlay_layer, passive_overlay_layer,
};
use crate::features::perf::{GpuiPerfContext, record_gpui_perf_sample};
use crate::features::runtime_jobs::ActivitySide;
use crate::theme::ThemePalette;

const WALLPAPER_TILE_ELEMENT_LIMIT: usize = 8192;
const WALLPAPER_TILE_MIN_SIZE: f32 = 8.;
const SSH_AUTH_PROMPT_PRIORITY: usize = usize::MAX;

/// Which overlays the root chrome should render this frame.
///
/// Computed from `NyaTermApp` directly. This used to be read back from a
/// snapshot the same `Render` pass had just published into `OverlayStore`,
/// with a fallback that recomputed exactly these expressions.
struct OverlayFlags {
    tab_actions_open: bool,
    color_picker_open: bool,
    session_info_open: bool,
    multi_line_paste_open: bool,
    terminal_actions_open: bool,
    action_link_menu_open: bool,
    action_link_tooltip_open: bool,
    command_suggestions_open: bool,
    credential_suggestions_open: bool,
    locked: bool,
}

impl NyaTermApp {
    pub(in crate::features) fn gpui_perf_context(
        &self,
        flat_row_count: usize,
        cache_hit: Option<bool>,
    ) -> GpuiPerfContext {
        GpuiPerfContext {
            connection_count: self.connection_state.connections().len(),
            group_count: self.connection_state.groups().len(),
            flat_row_count,
            cache_hit,
            full_shell_paint_count: FULL_SHELL_PAINT_COUNT
                .load(std::sync::atomic::Ordering::Relaxed),
            surface_paint_count: terminal_surface_paint_count(),
            left_panel: self
                .current_left_panel()
                .map(|panel| panel.persistence_id()),
            right_panel: self
                .current_right_panel()
                .map(|panel| panel.persistence_id()),
            resize_active: self.shell.panel_resize_active(),
        }
    }

    pub(crate) fn start_after_window_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_component_theme(cx);
        // The connections panel renders from a snapshot, so it needs one before its
        // first paint. Later ones come from store replies and panel interactions.
        self.flush_connection_panel_snapshot(cx);
        self.refresh_window_render_inputs(window, cx);
        self.queue_cloud_sync_history_refresh(None, cx);
        self.queue_wallpaper_refresh(cx);
        self.start_runtime_data_plane_drain(cx);
        self.start_tunnel_event_drain(cx);
        self.start_translation_event_drain(cx);
        self.start_update_event_drain(cx);
        self.start_github_gist_auth_event_drain(cx);
        self.start_command_persistence_event_drain(cx);
        self.start_stats_event_drain(cx);
        self.start_gpu_event_drain(cx);
        self.start_npu_event_drain(cx);
        self.start_process_event_drain(cx);
        self.start_docker_event_drain(cx);
        self.start_transfer_event_drain(cx);
        self.start_ai_chat_event_drain(cx);
        self.start_ai_discovery_event_drain(cx);
        self.start_recording_event_drain(cx);
        self.start_session_start_event_drain(cx);
        self.start_credential_autofill_match_drain(cx);
        self.start_remote_desktop_event_drain(cx);
        self.start_prompt_activation_drain(cx);
        self.start_shell_persistence_debounce(cx);
        // The restored panel width decides which process-table sort columns exist, and
        // nothing has resized yet, so this is where the initial value goes in.
        self.reconcile_remote_process_sort_columns();
        // The first snapshot. Nothing has mutated yet, so no boundary has fired, and a
        // panel with no snapshot renders empty.
        self.flush_remote_panel_snapshots(cx);
        self.ensure_cursor_blink_clock(cx);
        self.ensure_header_status_clock(cx);
        self.ensure_idle_lock_clock(cx);
        // A focus request can only be honoured once its element exists, which is a
        // result of this paint; arming here is both cheap and the earliest correct
        // point.
        self.ensure_pending_focus_clock(cx);
        self.ensure_post_start_work_clock(cx);
        self.try_restore_open_tabs(window, cx);
        let pending_session_start = self.session.start_has_pending();
        let should_pump = !self.session.restore_is_complete()
            && self
                .stores
                .startup_restore
                .update(cx, |store, _| store.can_pump_queue(pending_session_start));
        if should_pump {
            self.pump_startup_restore_queue(window, cx);
        }

        self.ensure_terminal_focus_reporting(window, cx);
        self.ensure_rdp_focus_reporting(window, cx);
    }

    fn root_chrome(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        self.ensure_paint_theme_caches();
        let palette = self.theme_palette();
        let wallpaper = self.shell.wallpaper_asset().cloned();
        let wallpaper_opacity =
            (self.settings.summary().background_image_opacity.min(100) as f32) / 100.0;
        let wallpaper_fit = self.settings.summary().background_image_fit.clone();
        div()
            .id(SharedString::from("nyaterm-root"))
            .size_full()
            .relative()
            .bg(self.shell_transparent_color(palette.bg))
            .text_color(rgb(palette.text))
            .font(self.gpui_ui_font().font())
            .text_size(px(self.settings.summary().ui_font_size.clamp(12, 24) as f32))
            .on_click(cx.listener(|this, _, _, _| {
                this.mark_user_activity();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.dismiss_transfer_rename_if_open(cx);
                    let changed =
                        this.shell.close_root_menus() || this.remote_ops.docker_menus_open();
                    if changed {
                        this.remote_ops.close_docker_menus();
                        cx.notify();
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, cx| {
                    this.dismiss_transfer_rename_if_open(cx);
                }),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.modal_child_window_open() {
                    this.activate_modal_child_window(cx);
                    cx.stop_propagation();
                    return;
                }
                if this.handle_global_shortcut(event, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.modal_child_window_open() {
                    return;
                }
                this.reconcile_root_pointer_interactions(event, cx);
                if event.dragging() {
                    this.update_transfer_browser_column_resize(event, cx);
                    this.update_panel_resize(event, cx);
                    this.update_transfer_height_resize(event, cx);
                    this.update_bottom_panel_resize(event, cx);
                    this.update_panel_stack_resize(event, cx);
                    this.update_workspace_split_resize(event, cx);
                }
                if this.maybe_send_terminal_any_motion_report(event, cx) {
                    return;
                }
                this.update_terminal_selection_drag(event, cx);
                if event.dragging() {
                    this.update_terminal_scrollbar_drag(event, cx);
                }
                this.update_action_link_hover(event, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _, cx| {
                    if this.modal_child_window_open() {
                        return;
                    }
                    this.finish_transfer_browser_column_resize(cx);
                    this.finish_panel_resize(cx);
                    this.finish_transfer_height_resize(cx);
                    this.finish_bottom_panel_resize(cx);
                    this.finish_panel_stack_resize(cx);
                    this.finish_workspace_split_resize(cx);
                    this.finish_terminal_selection(event, cx);
                    this.finish_terminal_scrollbar_drag(cx);
                    this.clear_terminal_window_drop(cx);
                    this.clear_session_tab_drag(cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Navigate(NavigationDirection::Back),
                cx.listener(|this, _event: &MouseUpEvent, window, cx| {
                    if this.modal_child_window_open() {
                        this.activate_modal_child_window(cx);
                        return;
                    }
                    if this.current_left_panel() == Some(NavItem::Transfers) {
                        cx.stop_propagation();
                        this.open_transfer_browser_history(1, window, cx);
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Navigate(NavigationDirection::Forward),
                cx.listener(|this, _event: &MouseUpEvent, window, cx| {
                    if this.modal_child_window_open() {
                        this.activate_modal_child_window(cx);
                        return;
                    }
                    if this.current_left_panel() == Some(NavItem::Transfers) {
                        cx.stop_propagation();
                        this.open_transfer_browser_history(-1, window, cx);
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(|this, event: &MouseUpEvent, _, cx| {
                    if this.modal_child_window_open() {
                        return;
                    }
                    if this.finish_terminal_mouse_report(event, cx) {
                        cx.stop_propagation();
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(|this, event: &MouseUpEvent, _, cx| {
                    if this.modal_child_window_open() {
                        return;
                    }
                    if this.finish_terminal_mouse_report(event, cx) {
                        cx.stop_propagation();
                    }
                }),
            )
            .when_some(wallpaper, |this, wallpaper| {
                if wallpaper_fit == "tile" {
                    let image = wallpaper.image().clone();
                    let dimensions = wallpaper.dimensions();
                    let layer = div()
                        .absolute()
                        .inset_0()
                        .overflow_hidden()
                        .opacity(wallpaper_opacity)
                        .child(
                            canvas(
                                |_, _, _| (),
                                move |bounds, (), window, _| {
                                    let viewport = (
                                        f32::from(bounds.size.width),
                                        f32::from(bounds.size.height),
                                    );
                                    let (tile_width, tile_height) = fit_wallpaper_tile_size(
                                        viewport,
                                        (dimensions.0 as f32, dimensions.1 as f32),
                                    );
                                    let (columns, rows) =
                                        wallpaper_tile_grid(viewport, (tile_width, tile_height));
                                    for row in 0..rows {
                                        for column in 0..columns {
                                            let tile_bounds = gpui::Bounds::new(
                                                gpui::point(
                                                    bounds.origin.x
                                                        + px(column as f32 * tile_width),
                                                    bounds.origin.y + px(row as f32 * tile_height),
                                                ),
                                                gpui::size(px(tile_width), px(tile_height)),
                                            );
                                            let _ = window.paint_image(
                                                tile_bounds,
                                                tile_bounds,
                                                gpui::Corners::default(),
                                                image.clone(),
                                                0,
                                                false,
                                            );
                                        }
                                    }
                                },
                            )
                            .size_full(),
                        );
                    return this.child(layer);
                }
                let object_fit = match wallpaper_fit.as_str() {
                    "contain" => ObjectFit::Contain,
                    "stretch" | "fill" => ObjectFit::Fill,
                    _ => ObjectFit::Cover,
                };
                let image = img(wallpaper.image().clone())
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .object_fit(object_fit)
                    .opacity(wallpaper_opacity);
                this.child(image)
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .child(self.title_bar(window, cx))
                    .child(self.workspace_surface(palette, window, cx)),
            )
    }

    fn workspace_surface(
        &mut self,
        palette: ThemePalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let started_at = Instant::now();
        let output = if self.shell.main_mode() == MainMode::Page
            && self.shell.selected_nav() == NavItem::Settings
        {
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .bg(self.shell_surface_color(palette.bg))
                .child(self.settings_view(cx))
                .into_any_element()
        } else {
            let compact_layout = !cfg!(target_os = "macos");
            let has_left_activity_items = self.activity_side_has_items(ActivitySide::Left);
            let has_right_activity_items = self.activity_side_has_items(ActivitySide::Right);
            let left_overlay_mode = compact_layout && self.shell.viewport_size().0 < 1024.;
            let right_overlay_mode = compact_layout && self.shell.viewport_size().0 < 768.;
            let left_drawer_open = has_left_activity_items
                && left_overlay_mode
                && self.shell.mobile_left_panel_open()
                && self.left_side_open();
            let right_drawer_open = has_right_activity_items
                && right_overlay_mode
                && self.shell.mobile_right_panel_open()
                && self.right_side_open();
            let mut surface = div()
                .flex()
                .flex_1()
                .min_h_0()
                .relative()
                .overflow_hidden()
                .bg(self.shell_transparent_color(palette.bg))
                .when(has_left_activity_items, |this| {
                    this.child(self.activity_bar(ActivitySide::Left, cx))
                })
                .when(
                    has_left_activity_items && self.left_side_open() && !left_overlay_mode,
                    |this| {
                        this.child(self.sidebar(false, window, cx))
                            .child(self.panel_resize_handle(PanelResizeSide::Left, cx))
                    },
                )
                .child(self.main_surface(cx))
                .when(
                    has_right_activity_items && self.right_side_open() && !right_overlay_mode,
                    |this| {
                        this.child(self.panel_resize_handle(PanelResizeSide::Right, cx))
                            .child(self.right_panel(false, window, cx))
                    },
                )
                .when(has_right_activity_items, |this| {
                    this.child(self.activity_bar(ActivitySide::Right, cx))
                });

            if self.terminal_windows_is_multi_leaf() && self.shell.new_session_menu_is_open() {
                surface = surface.child(self.render_new_session_menu(cx));
            }

            if left_drawer_open || right_drawer_open {
                surface = surface.child(
                    full_window_input_layer("mobile-panel-backdrop")
                        .bg(rgba(0x00000080))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.shell.close_mobile_panels();
                            cx.notify();
                        })),
                );
            }

            if left_drawer_open {
                surface = surface.child(
                    div()
                        .id("mobile-left-drawer")
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(40.))
                        .flex()
                        .shadow_lg()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .h_full()
                                .flex()
                                .flex_col()
                                .child(self.mobile_drawer_header("mobile-left-close", true, cx))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_h_0()
                                        .flex()
                                        .child(self.sidebar(true, window, cx)),
                                ),
                        ),
                );
            }

            if right_drawer_open {
                surface = surface.child(
                    div()
                        .id("mobile-right-drawer")
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right(px(40.))
                        .flex()
                        .shadow_lg()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .h_full()
                                .flex()
                                .flex_col()
                                .child(self.mobile_drawer_header("mobile-right-close", false, cx))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_h_0()
                                        .flex()
                                        .child(self.right_panel(true, window, cx)),
                                ),
                        ),
                );
            }

            surface.into_any_element()
        };
        record_gpui_perf_sample(
            "workspace_surface",
            started_at.elapsed(),
            self.gpui_perf_context(0, None),
        );
        output
    }

    fn mobile_drawer_header(
        &self,
        id: &'static str,
        left: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        div()
            .h(px(40.))
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .px_2()
            .border_b_1()
            .border_color(rgb(palette.border))
            .bg(self.shell_surface_color(palette.surface))
            .child(
                div()
                    .id(id)
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .text_color(rgb(palette.text_muted))
                    .cursor_pointer()
                    .hover(|this| this.bg(rgb(palette.hover)).text_color(rgb(palette.text)))
                    .child(
                        svg()
                            .size(px(16.))
                            .path("icons/window/close.svg")
                            .text_color(rgb(palette.text_muted)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if left {
                            this.shell.close_mobile_panel(PanelSide::Left);
                        } else {
                            this.shell.close_mobile_panel(PanelSide::Right);
                        }
                        cx.notify();
                    })),
            )
    }

    fn overlay_host(
        &mut self,
        content: Stateful<Div>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let terminal_overlays = self.terminal.overlay_visibility();
        let overlay = OverlayFlags {
            tab_actions_open: self.session.dialog_tab_actions_session_id().is_some(),
            color_picker_open: self.session.dialog_color_picker_is_open(),
            session_info_open: self.session.dialog_session_info_is_open(),
            multi_line_paste_open: terminal_overlays.paste_review,
            terminal_actions_open: terminal_overlays.actions,
            action_link_menu_open: terminal_overlays.action_link_menu,
            action_link_tooltip_open: terminal_overlays.action_link_tooltip,
            command_suggestions_open: self.terminal.command_suggestions_open(),
            credential_suggestions_open: self.terminal.credential_suggestions_open(),
            locked: self.security.screen_locked(),
        };
        let quick_switch_open = self.quick_switch_open(cx);
        let transfer_editor_open = self.transfer.editor_inline_overlay_is_open();
        let transfer_external_sync_open = self.active_external_editor_sync_prompt().is_some();
        let ssh_auth_prompt_open = self.session.prompt_has_active_ssh_auth();

        content
            .when(overlay.tab_actions_open, |this| {
                this.child(full_window_overlay_layer(
                    "tab-actions-input-layer",
                    self.tab_actions_overlay(cx),
                ))
            })
            .when(overlay.color_picker_open, |this| {
                this.child(full_window_overlay_layer(
                    "tab-color-input-layer",
                    self.tab_color_picker_overlay(cx),
                ))
            })
            .when(overlay.session_info_open, |this| {
                this.child(full_window_overlay_layer(
                    "session-info-input-layer",
                    self.session_info_overlay(cx),
                ))
            })
            .when(self.transfer.transfer_job_menu().is_some(), |this| {
                this.child(full_window_overlay_layer(
                    "transfer-job-menu-input-layer",
                    self.transfer_job_menu_overlay(cx),
                ))
            })
            .when(transfer_editor_open, |this| {
                this.child(full_window_overlay_layer(
                    "transfer-editor-input-layer",
                    self.transfer_editor_overlay(cx),
                ))
            })
            .when(transfer_external_sync_open, |this| {
                this.child(full_window_overlay_layer(
                    "transfer-external-sync-input-layer",
                    self.transfer_external_sync_prompt_overlay(cx),
                ))
            })
            .when(
                self.transfer.browser_view().favorites_menu.is_some(),
                |this| {
                    this.child(full_window_overlay_layer(
                        "transfer-favorites-menu-input-layer",
                        self.transfer_browser_favorites_menu_overlay(cx),
                    ))
                },
            )
            .when(self.transfer.browser_view().path_menu.is_some(), |this| {
                this.child(full_window_overlay_layer(
                    "transfer-path-menu-input-layer",
                    self.transfer_browser_path_menu_overlay(cx),
                ))
            })
            .when(self.transfer.browser_view().upload_menu.is_some(), |this| {
                this.child(full_window_overlay_layer(
                    "transfer-upload-menu-input-layer",
                    self.transfer_browser_upload_menu_overlay(cx),
                ))
            })
            .when(overlay.multi_line_paste_open, |this| {
                this.child(full_window_overlay_layer(
                    "multi-line-paste-input-layer",
                    self.multi_line_paste_overlay(cx),
                ))
            })
            .when(overlay.terminal_actions_open, |this| {
                this.child(full_window_overlay_layer(
                    "terminal-actions-input-layer",
                    self.terminal_actions_overlay(cx),
                ))
            })
            .when(overlay.action_link_menu_open, |this| {
                this.child(full_window_overlay_layer(
                    "action-link-menu-input-layer",
                    self.action_link_menu_overlay(cx),
                ))
            })
            .when(
                overlay.action_link_tooltip_open
                    && !overlay.action_link_menu_open
                    && !self.translation.dialog_is_open(),
                |this| this.child(passive_overlay_layer(self.action_link_tooltip_overlay(cx))),
            )
            .when(overlay.command_suggestions_open, |this| {
                this.child(passive_overlay_layer(self.command_suggestions_overlay(cx)))
            })
            .when(overlay.credential_suggestions_open, |this| {
                this.child(passive_overlay_layer(
                    self.credential_suggestions_overlay(cx),
                ))
            })
            .when(self.sync_input.is_open(), |this| {
                this.child(full_window_overlay_layer(
                    "sync-groups-input-layer",
                    self.sync_groups_overlay(cx),
                ))
            })
            .when_some(
                self.connection_state.inline_editor_panel_draft(),
                |this, editor| {
                    this.child(full_window_overlay_layer(
                        "connection-editor-input-layer",
                        self.connection_editor_panel(editor, cx),
                    ))
                },
            )
            .when(self.commands.quick_editor_is_inline(), |this| {
                this.child(full_window_overlay_layer(
                    "quick-command-editor-input-layer",
                    self.quick_command_editor_overlay(cx),
                ))
            })
            .when(self.commands.quick_details().is_some(), |this| {
                this.child(full_window_overlay_layer(
                    "quick-command-details-input-layer",
                    self.quick_command_details_overlay(cx),
                ))
            })
            .when(self.commands.quick_variable_prompt().is_some(), |this| {
                this.child(full_window_overlay_layer(
                    "quick-command-variable-input-layer",
                    self.quick_command_variable_prompt_overlay(cx),
                ))
            })
            .when(quick_switch_open, |this| {
                this.child(full_window_overlay_layer(
                    "quick-switch-input-layer",
                    self.quick_switch_overlay(cx),
                ))
            })
            .when(self.shell.activity_bar_context_menu().is_some(), |this| {
                this.child(full_window_overlay_layer(
                    "activity-context-input-layer",
                    self.activity_bar_context_menu_overlay(cx),
                ))
            })
            .when(overlay.locked, |this| {
                this.child(full_window_overlay_layer(
                    "lock-screen-input-layer",
                    self.lock_screen_overlay(window, cx),
                ))
            })
            .when(self.modal_child_window_open(), |this| {
                this.child(full_window_overlay_layer(
                    "modal-owner-input-layer",
                    self.modal_owner_backdrop(cx),
                ))
            })
            // Background SSH operations can request credentials while another
            // modal is open, so authentication must be the topmost overlay.
            .when(ssh_auth_prompt_open, |this| {
                this.child(
                    deferred(full_window_overlay_layer(
                        "ssh-auth-prompt-input-layer",
                        self.ssh_auth_prompt_overlay(cx),
                    ))
                    .with_priority(SSH_AUTH_PROMPT_PRIORITY),
                )
            })
    }

    fn reconcile_root_pointer_interactions(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        self.recover_terminal_mouse_report_after_lost_mouse_up(event, cx);
        if event.dragging() {
            return;
        }
        self.finish_transfer_browser_column_resize(cx);
        self.finish_panel_resize(cx);
        self.finish_transfer_height_resize(cx);
        self.finish_bottom_panel_resize(cx);
        self.finish_panel_stack_resize(cx);
        self.finish_workspace_split_resize(cx);
        self.recover_terminal_selection_after_lost_mouse_up(cx);
        self.finish_terminal_scrollbar_drag(cx);
        self.clear_terminal_window_drop(cx);
        self.clear_session_tab_drag(cx);
    }

    fn modal_child_window_open(&self) -> bool {
        self.shell.settings_window().is_some()
            || self.shell.settings_window_open_pending()
            || self.commands.quick_editor_window_is_open_or_pending()
            || self.connection_state.editor_modal_window_open_or_pending()
            || self.transfer.external_sync_has_window()
            || self.transfer.external_sync_has_pending_window()
    }

    fn activate_modal_child_window(&mut self, cx: &mut Context<Self>) -> bool {
        if self.shell.settings_window().is_some() {
            self.activate_settings_window(cx)
        } else if self.shell.settings_window_open_pending() {
            true
        } else if self.commands.quick_editor_window_is_open() {
            self.activate_quick_command_window(cx)
        } else if self.commands.quick_editor_window_is_pending() {
            true
        } else if self.connection_state.editor_has_window() {
            self.activate_connection_editor_window(cx)
        } else if self.connection_state.editor_window_open_pending() {
            true
        } else if self.transfer.external_sync_has_window() {
            self.activate_transfer_external_sync_window(cx)
        } else {
            self.transfer.external_sync_has_pending_window()
        }
    }

    fn modal_owner_backdrop(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("modal-owner-backdrop")
            .absolute()
            .inset_0()
            .bg(rgba(0x00000066))
            .cursor_pointer()
            .on_mouse_move(|_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.activate_modal_child_window(cx);
            }))
    }

    fn ssh_auth_prompt_overlay(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let host_key_prompt = self.session.prompt_active_host_key().cloned();
        let agent_prompt = self.session.prompt_active_agent().cloned();
        let credential_prompt = self.session.prompt_active_credential().cloned();
        let keyboard_interactive_prompt =
            self.session.prompt_active_keyboard_interactive().cloned();
        let dialog_width = if keyboard_interactive_prompt.is_some() {
            384.
        } else {
            416.
        };
        div()
            .id(SharedString::from("ssh-auth-prompt-overlay"))
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .right_0()
            .bg(rgba(0x00000080))
            .flex()
            .items_center()
            .justify_center()
            .p_3()
            .track_focus(self.session.prompt_credential_focus())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                if !this.focus_active_ssh_prompt_input(window, cx) {
                    window.focus(this.session.prompt_credential_focus(), cx);
                }
                cx.notify();
            }))
            .child(
                div()
                    .w(px(dialog_width))
                    .max_w_full()
                    .when_some(host_key_prompt, |this, prompt| {
                        this.child(self.host_key_prompt_banner(prompt, cx))
                    })
                    .when_some(agent_prompt, |this, prompt| {
                        this.child(self.agent_prompt_banner(prompt, cx))
                    })
                    .when_some(credential_prompt, |this, prompt| {
                        this.child(self.credential_prompt_banner(prompt, cx))
                    })
                    .when_some(keyboard_interactive_prompt, |this, prompt| {
                        this.child(self.keyboard_interactive_prompt_banner(prompt, cx))
                    }),
            )
    }
}

fn wallpaper_tile_grid(viewport: (f32, f32), tile: (f32, f32)) -> (usize, usize) {
    let columns = ((viewport.0.max(1.) / tile.0.max(WALLPAPER_TILE_MIN_SIZE)).ceil() as usize)
        .clamp(1, WALLPAPER_TILE_ELEMENT_LIMIT);
    let requested_rows =
        ((viewport.1.max(1.) / tile.1.max(WALLPAPER_TILE_MIN_SIZE)).ceil() as usize).max(1);
    let rows = requested_rows.min((WALLPAPER_TILE_ELEMENT_LIMIT / columns).max(1));
    (columns, rows)
}

fn fit_wallpaper_tile_size(viewport: (f32, f32), intrinsic: (f32, f32)) -> (f32, f32) {
    let mut tile = (
        intrinsic.0.max(WALLPAPER_TILE_MIN_SIZE),
        intrinsic.1.max(WALLPAPER_TILE_MIN_SIZE),
    );
    for _ in 0..4 {
        let columns = (viewport.0.max(1.) / tile.0).ceil().max(1.);
        let rows = (viewport.1.max(1.) / tile.1).ceil().max(1.);
        let count = columns * rows;
        if count <= WALLPAPER_TILE_ELEMENT_LIMIT as f32 {
            break;
        }
        let scale = (count / WALLPAPER_TILE_ELEMENT_LIMIT as f32).sqrt() * 1.01;
        tile.0 *= scale;
        tile.1 *= scale;
    }
    tile
}

impl Render for NyaTermApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let render_started_at = Instant::now();
        // Reconcile viewport size and cell metrics here rather than on the runtime
        // tick. `render` already holds the window, and the chrome built below reads
        // `shell.viewport_size()`, so this paint sees the fresh values instead of
        // whatever the last tick recorded. Nothing here notifies *this* entity, so it
        // cannot loop: surfaces are separate entities.
        self.refresh_window_render_inputs(window, cx);
        // Safety net for the idle screen-lock deadline. Every unlock and every
        // settings change arms the clock directly; this catches a path added later
        // that forgets to, because a clock that failed to arm means the screen never
        // locks. Costs one bool compare when it is already running, and arming
        // redundantly is harmless -- the clock just re-checks and defers.
        self.ensure_idle_lock_clock(cx);
        // Each polling panel owns its own refresh clock; this starts the ones that are
        // wanted and stops the rest. `render` because every input it reads -- which
        // panels are on screen, what the header is showing, which panels settings
        // enable, whether an SSH session is active -- changes alongside a repaint, and
        // no single event covers all four.
        self.sync_remote_panel_demand(cx);
        // The transfer browser's cwd sync, likewise armed here because the browser
        // being open changes alongside a repaint.
        self.ensure_transfer_cwd_sync_clock(cx);
        FULL_SHELL_PAINT_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let full_shell_paint_count = self.shell.note_full_shell_paint();
        let root_started_at = Instant::now();
        let content = self.root_chrome(window, cx);
        let root_duration = root_started_at.elapsed();
        let overlay_started_at = Instant::now();
        let output = self.overlay_host(content, window, cx);
        let overlay_duration = overlay_started_at.elapsed();
        let render_duration = render_started_at.elapsed();
        record_gpui_perf_sample(
            "root_chrome",
            root_duration,
            self.gpui_perf_context(0, None),
        );
        record_gpui_perf_sample(
            "root_render",
            render_duration,
            self.gpui_perf_context(0, None),
        );
        if render_duration >= Duration::from_millis(12)
            && self.should_log_slow_diagnostic("root_render", Instant::now())
        {
            tracing::warn!(
                diagnostic = "root_render",
                total_ms = render_duration.as_millis(),
                root_chrome_ms = root_duration.as_millis(),
                overlay_host_ms = overlay_duration.as_millis(),
                active_session_id = self.session.active_id().unwrap_or(""),
                visible_session_count = self.visible_terminal_session_ids().len(),
                connect_settle_active = self.shell.connect_settle_active(Instant::now()),
                output_pressure = self.runtime_output_pressure_active(),
                full_shell_paint_count,
                surface_paint_count = terminal_surface_paint_count(),
                "slow root render"
            );
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use gpui::{
        AppContext as _, IntoElement, ParentElement as _, Render, Styled as _, TestAppContext,
        VisualTestContext, div,
    };
    use nyaterm_core::{AiExecutionProfile, AppRuntime, RuntimeMode, uuid};
    use nyaterm_transport::LocalSessionConfig;

    use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
    use crate::models::{SessionLaunchConfig, SessionRuntimeMetadata, TabDockEdge, TabDockZone};

    use super::{fit_wallpaper_tile_size, wallpaper_tile_grid};

    #[test]
    fn wallpaper_tiles_cover_viewport_and_cap_extreme_counts() {
        assert_eq!(wallpaper_tile_grid((1280., 800.), (256., 256.)), (5, 4));
        assert_eq!(wallpaper_tile_grid((1280., 800.), (2048., 2048.)), (1, 1));
        let (columns, rows) = wallpaper_tile_grid((100_000., 100_000.), (1., 1.));
        assert!(columns * rows <= 8192);
        let tile = fit_wallpaper_tile_size((3840., 2160.), (1., 1.));
        let (columns, rows) = wallpaper_tile_grid((3840., 2160.), tile);
        assert!(columns * rows <= 8192);
        assert!(columns as f32 * tile.0 >= 3840.);
        assert!(rows as f32 * tile.1 >= 2160.);
    }

    fn root_render_benchmark_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "nyaterm-root-render-benchmark-{}-{}",
            std::process::id(),
            uuid()
        ))
    }

    fn root_render_benchmark_session(name: String) -> SessionRuntimeMetadata {
        SessionRuntimeMetadata {
            ssh_config: None,
            ssh_multiplex_key: None,
            source_connection_id: None,
            ai_execution_profile: AiExecutionProfile::Posix,
            launch_config: SessionLaunchConfig::Local(LocalSessionConfig {
                name,
                ..LocalSessionConfig::default()
            }),
            disconnected: false,
        }
    }

    struct RootRenderBenchmarkFixture {
        app: gpui::Entity<super::NyaTermApp>,
    }

    impl Render for RootRenderBenchmarkFixture {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            div().size_full().child(self.app.clone())
        }
    }

    fn forced_root_draw(
        app: &gpui::Entity<super::NyaTermApp>,
        cx: &mut VisualTestContext,
    ) -> Duration {
        cx.run_until_parked();
        cx.update(|window, cx| {
            app.update(cx, |_, cx| cx.notify());
            let started_at = Instant::now();
            _ = window.draw(cx);
            started_at.elapsed()
        })
    }

    fn run_root_render_benchmark(cx: &mut TestAppContext) {
        let root = root_render_benchmark_dir();
        let runtime = AppRuntime::from_parts_for_test(
            RuntimeMode::Portable,
            root.clone(),
            root.join("config"),
            root.join("logs"),
            root.join("cache"),
            None,
        );
        let stores = UiStoreHandles {
            startup_restore: cx.new(|_| StartupRestoreStore::default()),
            overlays: cx.new(|_| OverlayStore::default()),
        };
        let app = cx.new(|cx| super::NyaTermApp::new(runtime, stores, cx));
        cx.update_entity(&app, |app, cx| {
            app.sync_component_theme(cx);
            let session_ids = (0..100)
                .map(|index| format!("benchmark-session-{index:03}"))
                .collect::<Vec<_>>();
            for (index, session_id) in session_ids.iter().enumerate() {
                app.session.register_session_metadata(
                    session_id,
                    root_render_benchmark_session(format!("Local session {index:03}")),
                );
                app.terminal
                    .seed_session_view(session_id.clone(), String::new(), "UTF-8");
            }
            let active_session_id = session_ids[0].clone();
            app.session.select_active_session(active_session_id.clone());
            app.shell.show_workspace();
            let target_leaf_id = app
                .terminal
                .ensure_terminal_windows_root(session_ids.clone(), Some(active_session_id))
                .expect("benchmark terminal window root");
            let mut leaf_ids = vec![target_leaf_id.clone()];
            for session_id in session_ids.iter().skip(1).take(7) {
                let result = app.terminal.dock_tab_on_terminal_window_leaf(
                    session_id,
                    &target_leaf_id,
                    TabDockZone::Edge(TabDockEdge::Right),
                );
                let crate::features::terminal::TerminalWindowDockResult::Docked {
                    focused_leaf_id: Some(leaf_id),
                } = result
                else {
                    panic!("benchmark session should create a terminal leaf");
                };
                leaf_ids.push(leaf_id);
            }
            for (index, session_id) in session_ids.iter().skip(8).enumerate() {
                let leaf_id = &leaf_ids[index % leaf_ids.len()];
                if leaf_id == &target_leaf_id {
                    continue;
                }
                let result = app.terminal.dock_tab_on_terminal_window_leaf(
                    session_id,
                    leaf_id,
                    TabDockZone::Center,
                );
                assert!(matches!(
                    result,
                    crate::features::terminal::TerminalWindowDockResult::Docked { .. }
                ));
            }
        });
        let fixture_app = app.clone();
        let (_, cx) =
            cx.add_window_view(move |_, _| RootRenderBenchmarkFixture { app: fixture_app });
        let cx: &mut VisualTestContext = cx;

        for _ in 0..12 {
            _ = forced_root_draw(&app, cx);
        }
        let mut samples = (0..120)
            .map(|_| forced_root_draw(&app, cx))
            .collect::<Vec<_>>();
        samples.sort_unstable();
        let total = samples.iter().copied().sum::<Duration>();
        let average = total / samples.len() as u32;
        let p95_index = ((samples.len() * 95).div_ceil(100)).saturating_sub(1);
        let p95 = samples[p95_index];
        let max = *samples.last().expect("benchmark samples");

        eprintln!(
            "root render benchmark: sessions=100 leaves=8 samples=120 average={average:?} p95={p95:?} max={max:?}"
        );
    }

    #[test]
    #[ignore = "performance benchmark; run manually with --ignored --nocapture"]
    fn root_render_hundred_sessions_eight_terminal_leaves_benchmark() {
        std::thread::Builder::new()
            .name("nyaterm-root-render-benchmark".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                let mut cx = TestAppContext::single();
                run_root_render_benchmark(&mut cx);
            })
            .expect("spawn root render benchmark thread")
            .join()
            .expect("root render benchmark thread");
    }
}
