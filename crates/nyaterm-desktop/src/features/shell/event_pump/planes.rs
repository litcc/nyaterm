use std::time::Instant;

use gpui::{Context, Window};

use crate::features::shell::event_pump::helpers::{
    RUNTIME_TICK_SLOW_THRESHOLD, RuntimeIdlePlaneResult, TERMINAL_PERF_HEARTBEAT_INTERVAL,
    connect_settle_active, diagnostic_log_due, runtime_idle_plane_allowed,
    runtime_ui_notify_allowed, window_geometry_churn_active,
};
use crate::features::{
    NyaTermApp, terminal::full_shell_paint_count, terminal::terminal_surface_paint_count,
};
use crate::models::NavItem;

impl NyaTermApp {
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
            if dirty {
                cx.notify();
            }
            return true;
        }
        let geometry_churn = window_geometry_churn_active(self.shell.viewport.last_change_at, now);
        let calm_tick = self.runtime_quiet_tick_allowed();
        if geometry_churn && calm_tick {
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
        if calm_tick && !remote_panels_need_poll && !self.ai.has_background_work() {
            if dirty {
                cx.notify();
            }
            return true;
        }

        let idle = self.drive_runtime_idle_plane(window, cx);
        dirty |= idle.dirty;

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
                render_requests_output_pressure = idle.render_request_output_pressure,
                remote_refresh_ms = idle.remote_refresh.as_millis(),
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

        if !runtime_idle_plane_allowed(demote_idle) {
            result.dirty = dirty;
            return result;
        }

        let stage_started_at = Instant::now();
        dirty |= self.drive_remote_auto_refresh(window, cx);
        result.remote_refresh = stage_started_at.elapsed();

        result.dirty = dirty;
        result
    }
}
