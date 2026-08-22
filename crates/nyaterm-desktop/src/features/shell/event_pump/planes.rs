use std::time::Instant;

use gpui::{Context, Window};

use crate::features::shell::event_pump::helpers::{
    RUNTIME_TICK_SLOW_THRESHOLD, RuntimeIdlePlaneResult, RuntimeVisualPlaneResult,
    TERMINAL_PERF_HEARTBEAT_INTERVAL, connect_settle_active, diagnostic_log_due,
    runtime_idle_plane_allowed, runtime_ui_notify_allowed, terminal_performance_tick_session_ids,
    terminal_render_work_pressure_active, window_geometry_churn_active,
};
use crate::features::{
    NyaTermApp, terminal::full_shell_paint_count, terminal::terminal_surface_paint_count,
};
use crate::models::NavItem;

impl NyaTermApp {
    fn drive_startup_restore_queue_tick(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let pending_session_start = self.session.start_has_pending();
        let should_pump = !self.session.restore_is_complete()
            && self
                .stores
                .startup_restore
                .update(cx, |store, _| store.can_pump_queue(pending_session_start));
        if !should_pump {
            return false;
        }
        self.pump_startup_restore_queue(window, cx);
        true
    }

    fn drive_pending_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.ai.chat_focus_is_pending()
            && !self.transfer.rename_focus_is_pending()
            && !self.session.prompt_credential_focus_is_pending()
        {
            return false;
        }
        let mut dirty = false;
        if self.ai.take_chat_focus_request() {
            window.focus(self.ai.chat_focus(), cx);
            dirty = true;
        }
        if let Some(input_id) = self.transfer.pending_rename_input_id()
            && self.focus_text_input_if_present(&input_id, window, cx)
        {
            self.transfer.finish_rename_focus();
            dirty = true;
        }
        if self.session.prompt_credential_focus_is_pending()
            && (self.session.prompt_has_active_credential()
                || self.session.prompt_has_active_keyboard_interactive())
        {
            self.focus_active_ssh_prompt_input(window, cx);
            self.session.prompt_finish_credential_focus();
            dirty = true;
        }
        dirty
    }

    pub(crate) fn drive_window_runtime_tick(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let tick_started_at = Instant::now();
        // Viewport/cell-metrics reconcile happens in `render`, the header clock and
        // connect status on their own timers; see `shell::status_clocks`.
        let mut dirty = false;

        // Skip full planes when the compositor is moving/resizing the window, or
        // when there is simply nothing pending (common during pure window drag).
        let now = Instant::now();
        if self
            .shell
            .runtime
            .connect_settle_until
            .is_some_and(|until| now >= until)
        {
            self.shell.runtime.connect_settle_until = None;
        }
        if self.title_drag_active(now) {
            dirty |= self.drive_pending_focus(window, cx);
            if dirty {
                cx.notify();
            }
            return true;
        }
        let geometry_churn = window_geometry_churn_active(self.shell.viewport.last_change_at, now);
        let calm_tick = self.runtime_quiet_tick_allowed();
        if geometry_churn && calm_tick {
            dirty |= self.drive_pending_focus(window, cx);
            if dirty {
                cx.notify();
            }
            return true;
        }
        // Ultra-light idle: focus + optional blink only. Used for pure window drag
        // (viewport often unchanged) and quiet connected sessions with no sideband.
        // Remote auto-refresh also feeds the title bar's resource and host modes.
        let remote_panels_need_poll = (self.session.active_ssh_config().is_some()
            && (matches!(
                self.current_right_panel(),
                Some(
                    NavItem::Stats
                        | NavItem::GpuMonitor
                        | NavItem::AscendNpuMonitor
                        | NavItem::Processes
                        | NavItem::Docker
                )
            ) || self.header_status_needs_remote_stats()
                || self.header_status_needs_gpu()
                || self.header_status_needs_npu()))
            || self.current_left_panel() == Some(NavItem::Transfers);
        if calm_tick
            && !remote_panels_need_poll
            && !self.recording.has_pending_auto_start()
            && self.terminal.terminal_windows_restore_is_complete()
            && !self.ai.has_background_work()
        {
            dirty |= self.drive_pending_focus(window, cx);
            // During connect settle, skip blink notifies so first frames stay free.
            if !connect_settle_active(self.shell.runtime.connect_settle_until, now) {
                let visual = self.drive_runtime_visual_plane(cx);
                dirty |= visual.dirty;
            }
            if dirty {
                cx.notify();
            }
            return true;
        }

        let idle = self.drive_runtime_idle_plane(window, cx);
        dirty |= idle.dirty;

        let visual = self.drive_runtime_visual_plane(cx);
        dirty |= visual.dirty;

        let visual_dirty = dirty;
        let notify_started_at = Instant::now();
        let notify_now = notify_started_at;
        let connect_settle =
            connect_settle_active(self.shell.runtime.connect_settle_until, notify_now);
        let output_pressure_for_notify = self.runtime_output_pressure_active();
        let throttle_active = output_pressure_for_notify || connect_settle;
        if visual_dirty {
            self.shell.runtime.pending_ui_notify = true;
        }
        let should_notify = runtime_ui_notify_allowed(
            visual_dirty,
            self.shell.runtime.pending_ui_notify,
            false,
            throttle_active,
            self.shell.runtime.last_ui_notify_at,
            notify_now,
        );
        if should_notify {
            cx.notify();
            self.shell.runtime.last_ui_notify_at = Some(notify_now);
            self.shell.runtime.pending_ui_notify = false;
        }
        let notify_duration = notify_started_at.elapsed();
        let output_pressure = self.runtime_output_pressure_active();
        let tick_duration = tick_started_at.elapsed();
        if tick_duration >= RUNTIME_TICK_SLOW_THRESHOLD
            && self.should_log_slow_diagnostic("runtime_tick", Instant::now())
        {
            tracing::warn!(
                diagnostic = "runtime_tick",
                total_ms = tick_duration.as_millis(),
                startup_restore_ms = idle.startup_restore.as_millis(),
                render_requests_output_pressure = idle.render_request_output_pressure,
                pending_focus_ms = idle.pending_focus.as_millis(),
                remote_refresh_ms = idle.remote_refresh.as_millis(),
                visual_runtime_ms = visual.duration.as_millis(),
                notify_ms = notify_duration.as_millis(),
                queued_events = self.shell.runtime.session_event_queued_events,
                queued_output_bytes = self.shell.runtime.session_event_queued_output_bytes,
                frame_command_count = self.terminal.frame_queue_metrics().command_count,
                frame_command_output_bytes = self.terminal.frame_queue_metrics().output_bytes,
                frame_event_count = self.terminal.frame_queue_metrics().event_count,
                frame_event_wake_count = self.terminal.frame_queue_metrics().event_wake_count,
                pending_frame_events = self.terminal.frame_queue_metrics().pending_event_count,
                pending_session_starts = self.session.start_pending_count(),
                output_pressure,
                next_tick_delay_ms = self.window_runtime_tick_delay().as_millis(),
                visual_dirty,
                full_shell_paint_count = self.shell.runtime.full_shell_paint_count,
                surface_frame_notify_count = self.shell.runtime.terminal_surface_frame_notify_count,
                chrome_frame_notify_count = self.shell.runtime.terminal_chrome_frame_notify_count,
                surface_paint_count = terminal_surface_paint_count(),
                notify_requested = visual_dirty,
                "slow runtime tick"
            );
        }
        let heartbeat_now = Instant::now();
        let heartbeat_due = diagnostic_log_due(
            self.shell.runtime.last_terminal_perf_heartbeat_at,
            heartbeat_now,
            TERMINAL_PERF_HEARTBEAT_INTERVAL,
        );
        if heartbeat_due {
            let full_shell_paints = full_shell_paint_count();
            let surface_paints = terminal_surface_paint_count();
            let surface_frame_notifies = self.shell.runtime.terminal_surface_frame_notify_count;
            let chrome_frame_notifies = self.shell.runtime.terminal_chrome_frame_notify_count;
            let (layout_cache_hits, layout_cache_misses) =
                self.visible_terminal_layout_cache_stats();
            let full_shell_paint_delta = full_shell_paints
                .saturating_sub(self.shell.runtime.last_perf_full_shell_paint_count);
            let surface_paint_delta =
                surface_paints.saturating_sub(self.shell.runtime.last_perf_surface_paint_count);
            let surface_frame_notify_delta = surface_frame_notifies
                .saturating_sub(self.shell.runtime.last_perf_surface_frame_notify_count);
            let chrome_frame_notify_delta = chrome_frame_notifies
                .saturating_sub(self.shell.runtime.last_perf_chrome_frame_notify_count);
            let layout_cache_hit_delta =
                layout_cache_hits.saturating_sub(self.shell.runtime.last_perf_layout_cache_hits);
            let layout_cache_miss_delta = layout_cache_misses
                .saturating_sub(self.shell.runtime.last_perf_layout_cache_misses);
            let active_session_id = self.session.active_id().unwrap_or("");
            let active_scroll_offset = self.active_terminal_scroll_offset();
            let active_display_offset = self.active_terminal_display_offset();
            let visible_session_count = self.visible_terminal_session_ids().len();
            let has_runtime_activity = !active_session_id.is_empty()
                || full_shell_paint_delta > 0
                || surface_paint_delta > 0
                || surface_frame_notify_delta > 0
                || chrome_frame_notify_delta > 0
                || self.shell.runtime.session_event_queued_events > 0
                || self.shell.runtime.session_event_queued_output_bytes > 0
                || self.session.event_bridge_queued_output_bytes() > 0
                || self.terminal.frame_queue_metrics().output_bytes > 0
                || self.terminal.frame_queue_metrics().event_count > 0
                || self.terminal.frame_queue_metrics().pending_event_count > 0;
            if has_runtime_activity {
                tracing::debug!(
                    diagnostic = "terminal_perf_heartbeat",
                    active_session_id,
                    visible_session_count,
                    active_scroll_offset,
                    active_display_offset,
                    connect_settle_active = self
                        .shell
                        .runtime
                        .connect_settle_until
                        .is_some_and(|until| heartbeat_now < until),
                    output_pressure,
                    visual_dirty,
                    tick_ms = tick_duration.as_millis(),
                    visual_runtime_ms = visual.duration.as_millis(),
                    notify_ms = notify_duration.as_millis(),
                    queued_session_events = self.shell.runtime.session_event_queued_events,
                    queued_session_output_bytes =
                        self.shell.runtime.session_event_queued_output_bytes,
                    bridge_output_bytes = self.session.event_bridge_queued_output_bytes(),
                    frame_command_count = self.terminal.frame_queue_metrics().command_count,
                    frame_command_output_bytes = self.terminal.frame_queue_metrics().output_bytes,
                    frame_event_count = self.terminal.frame_queue_metrics().event_count,
                    frame_event_wake_count = self.terminal.frame_queue_metrics().event_wake_count,
                    pending_frame_events = self.terminal.frame_queue_metrics().pending_event_count,
                    full_shell_paint_delta,
                    surface_paint_delta,
                    surface_frame_notify_delta,
                    chrome_frame_notify_delta,
                    full_shell_paint_count = full_shell_paints,
                    surface_paint_count = surface_paints,
                    surface_frame_notify_count = surface_frame_notifies,
                    chrome_frame_notify_count = chrome_frame_notifies,
                    layout_cache_hit_delta,
                    layout_cache_miss_delta,
                    layout_cache_hits,
                    layout_cache_misses,
                    "terminal perf heartbeat"
                );
            }
            self.shell.runtime.last_terminal_perf_heartbeat_at = Some(heartbeat_now);
            self.shell.runtime.last_perf_full_shell_paint_count = full_shell_paints;
            self.shell.runtime.last_perf_surface_paint_count = surface_paints;
            self.shell.runtime.last_perf_surface_frame_notify_count = surface_frame_notifies;
            self.shell.runtime.last_perf_chrome_frame_notify_count = chrome_frame_notifies;
            self.shell.runtime.last_perf_layout_cache_hits = layout_cache_hits;
            self.shell.runtime.last_perf_layout_cache_misses = layout_cache_misses;
        }
        self.shell.runtime.event_pump_started
    }

    pub(super) fn drive_runtime_idle_plane(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> RuntimeIdlePlaneResult {
        let mut dirty = false;
        let mut result = RuntimeIdlePlaneResult::default();
        // Idle-plane work does not drain output; one pressure sample is enough for the stage.
        let output_pressure = self.runtime_output_pressure_active();
        let now = Instant::now();
        let geometry_churn = window_geometry_churn_active(self.shell.viewport.last_change_at, now);
        let connect_settle = connect_settle_active(self.shell.runtime.connect_settle_until, now);
        // Geometry churn / connect settle: keep focus only (no remote/layout/DB).
        let demote_idle = output_pressure || geometry_churn || connect_settle;
        result.render_request_output_pressure = demote_idle;

        // Focus transitions remain latency-sensitive even under pressure.
        let stage_started_at = Instant::now();
        dirty |= self.drive_pending_focus(window, cx);
        result.pending_focus = stage_started_at.elapsed();

        if !runtime_idle_plane_allowed(demote_idle) {
            result.dirty = dirty;
            return result;
        }

        // Layout restore opens the config DB — never do it while sessions are
        // still connecting or the data plane is under pressure.
        if !self.terminal.terminal_windows_restore_is_complete()
            && !self.session.start_has_pending()
            && !self.runtime_output_pressure_active()
            && !connect_settle
        {
            self.try_restore_terminal_window_layout(cx);
            if self.terminal.terminal_window_tree_is_some() {
                self.reconcile_terminal_windows();
            }
        }
        // Auto-recording opens files; keep it off the first calm frames after connect.
        if !self.session.start_has_pending()
            && !self.runtime_output_pressure_active()
            && !connect_settle
            && let Some((session_id, session_name)) = self.recording.take_pending_auto_start()
        {
            self.maybe_auto_start_recording(&session_id, &session_name, cx);
        }

        let stage_started_at = Instant::now();
        dirty |= self.drive_startup_restore_queue_tick(window, cx);
        result.startup_restore = stage_started_at.elapsed();

        let stage_started_at = Instant::now();
        dirty |= self.drive_remote_auto_refresh(window, cx);
        result.remote_refresh = stage_started_at.elapsed();

        result.dirty = dirty;
        result
    }

    pub(super) fn drive_runtime_visual_plane(
        &mut self,
        cx: &mut Context<Self>,
    ) -> RuntimeVisualPlaneResult {
        let visual_stage_started_at = Instant::now();
        let mut dirty = false;
        let now = Instant::now();
        let output_pressure = self.runtime_output_pressure_active()
            || connect_settle_active(self.shell.runtime.connect_settle_until, now);
        // Cursor blink is owned by `shell::cursor_blink`'s own timer, not by this
        // plane: at the quiet cadence a tick-driven toggle stretched the visible
        // half-period to roughly twice its setting.
        let render_work_pressure =
            terminal_render_work_pressure_active(output_pressure, self.session.start_has_pending());
        // Large-output protection recovery accounting.
        // Under pressure only touch views that already need recovery accounting.
        let visible_session_ids = self.visible_terminal_session_ids();
        let performance_session_ids = terminal_performance_tick_session_ids(&visible_session_ids);
        let surface_paint_sessions = self.terminal.tick_session_performance(
            performance_session_ids.iter().map(String::as_str),
            render_work_pressure,
            now,
        );
        for session_id in surface_paint_sessions {
            self.notify_terminal_surface_only(Some(session_id.as_str()), cx);
        }
        // Drop overlay only while a platform drag is active.
        if !cx.has_active_drag() && self.terminal.clear_terminal_file_drop_hover() {
            dirty = true;
        }
        if !cx.has_active_drag() && self.transfer.set_browser_external_drop_hover(false) {
            dirty = true;
        }
        RuntimeVisualPlaneResult {
            dirty,
            duration: visual_stage_started_at.elapsed(),
        }
    }
}
