use std::fmt::Write as _;
use std::time::{Duration, Instant};

pub(super) const TRANSFER_AUTO_SYNC_CWD_INTERVAL_SECONDS: u32 = 3;
pub(super) const SESSION_EVENT_DRAIN_BATCH: usize = 256;
pub(super) const SESSION_EVENT_DRAIN_IDLE_OUTPUT_BUDGET: usize = 32 * 1024;
pub(super) const SESSION_EVENT_DRAIN_PRESSURE_OUTPUT_BUDGET: usize = 8 * 1024;
pub(super) const SESSION_EVENT_DRAIN_WALL_BUDGET: Duration = Duration::from_millis(8);
pub(super) const SESSION_EVENT_INPUT_WAKE_DRAIN_BATCH: usize = 32;
pub(super) const SESSION_EVENT_INPUT_WAKE_OUTPUT_BUDGET: usize = 4 * 1024;
pub(super) const SESSION_EVENT_INPUT_WAKE_WALL_BUDGET: Duration = Duration::from_millis(1);
pub(super) const RUNTIME_BACKGROUND_EVENT_DRAIN_WALL_BUDGET: Duration = Duration::from_millis(6);
pub(super) const RUNTIME_BACKGROUND_EVENT_DRAIN_SLOW: Duration = Duration::from_millis(12);
/// One data-plane drain cycle is budgeted 8ms of session events plus 4ms of frame
/// applies, so anything past this overran both.
pub(super) const RUNTIME_DATA_PLANE_DRAIN_SLOW: Duration = Duration::from_millis(16);
pub(super) const RUNTIME_IDLE_TICK_INTERVAL: Duration = Duration::from_millis(50);
pub(super) const RUNTIME_QUIET_TICK_INTERVAL: Duration = Duration::from_millis(500);
/// Match display frame pacing; 8ms stacked full ticks under pressure contended with window drag paints.
pub(super) const RUNTIME_PRESSURE_TICK_INTERVAL: Duration = Duration::from_millis(16);
/// After viewport size changes, hold pressure cadence for this long.
pub(super) const WINDOW_GEOMETRY_CHURN_HOLD: Duration = Duration::from_millis(200);
/// Window move drags may not resize the viewport, especially on Windows. Hold a
/// short quiet window from title-bar mouse down so terminal paints do not fight
/// compositor movement.
pub(super) const TITLE_DRAG_ACTIVE_HOLD: Duration = Duration::from_millis(1200);
/// After a session becomes live, demote idle/visual for this long so first-frame
/// output does not compete with chrome rebuilds (does not raise tick cadence).
pub(super) const CONNECT_SETTLE_HOLD: Duration = Duration::from_millis(750);
/// Under output pressure / connect settle, coalesce full-shell paints.
pub(super) const UI_PAINT_THROTTLE: Duration = Duration::from_millis(33);
pub(super) const TERMINAL_FRAME_APPLY_PRESSURE_INTERVAL: Duration = Duration::from_millis(16);
pub(super) const SLOW_DIAGNOSTIC_THROTTLE: Duration = Duration::from_secs(2);
pub(super) const TERMINAL_PERF_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
pub(super) const RUNTIME_TICK_SLOW_THRESHOLD: Duration = Duration::from_millis(40);
pub(super) const SESSION_EVENT_DRAIN_SLOW_TOTAL: Duration = Duration::from_millis(20);
pub(super) const SESSION_EVENT_DRAIN_SLOW_CHUNK: Duration = Duration::from_millis(8);
pub(super) const PENDING_SESSION_STILL_CONNECTING_AFTER: Duration = Duration::from_secs(15);
pub(super) const PENDING_SESSION_STATUS_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(super) struct SessionEventDrainTimings {
    pub(super) output_total: Duration,
    pub(super) zmodem: Duration,
    pub(super) trzsz: Duration,
    pub(super) decode: Duration,
    pub(super) recording: Duration,
    pub(super) terminal_append: Duration,
    pub(super) credential_autofill: Duration,
    pub(super) ai_capture: Duration,
}

#[derive(Default)]
pub(super) struct RuntimeBackgroundDrainTimings {
    pub(super) credential_autofill: Duration,
    pub(super) recording: Duration,
    pub(super) transfer: Duration,
    pub(super) ai: Duration,
    pub(super) remote: Duration,
    pub(super) maintenance: Duration,
    pub(super) budget_exhausted: bool,
}

#[derive(Default)]
pub(super) struct RuntimeControlPlaneResult {
    pub(super) dirty: bool,
    pub(super) duration: Duration,
    pub(super) timings: RuntimeControlPlaneDrainTimings,
}

pub(super) struct RuntimeDataPlaneResult {
    pub(super) dirty: bool,
    pub(super) background_total: Duration,
    pub(super) background_timings: RuntimeBackgroundDrainTimings,
}

/// Whether the frame drain may run this cycle, and why not when it may not.
///
/// Computed between the session-event drain and the frame drain, because
/// `runtime_background_should_defer_terminal_frames` reads what the former just
/// drained.
#[derive(Clone, Copy, Default)]
pub(super) struct TerminalFrameApplyDecision {
    /// Frames were held back for pacing; the caller must come back on a timer.
    pub(super) defer: bool,
    pub(super) deferred_after_output: bool,
    pub(super) paced: bool,
}

/// One drain cycle of the runtime data plane, for the caller's wake decision.
pub(super) struct RuntimeDataPlaneDrain {
    /// `None` parks on the wake; `Some(delay)` comes back after `delay`.
    pub(super) wake_delay: Option<Duration>,
    pub(super) session_events: Duration,
    pub(super) terminal_frames: Duration,
    pub(super) decision: TerminalFrameApplyDecision,
}

#[derive(Default)]
pub(super) struct RuntimeIdlePlaneResult {
    pub(super) dirty: bool,
    pub(super) startup_restore: Duration,
    pub(super) terminal_resize: Duration,
    pub(super) render_requests: Duration,
    pub(super) render_request_output_pressure: bool,
    pub(super) pending_focus: Duration,
    pub(super) action_link_tooltip: Duration,
    pub(super) remote_refresh: Duration,
    pub(super) idle_lock: Duration,
}

pub(super) struct RuntimeVisualPlaneResult {
    pub(super) dirty: bool,
    pub(super) duration: Duration,
}

#[derive(Default)]
pub(super) struct RuntimeControlPlaneDrainTimings {
    pub(super) session_start: Duration,
    pub(super) prompts: Duration,
}

#[derive(Clone, Copy)]
pub(super) struct SessionEventDrainBudget {
    pub(super) max_events: usize,
    pub(super) max_output_bytes: usize,
    pub(super) wall_budget: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PendingSessionAuthWait {
    Credential { target: String },
    HostKey { host: String },
    Agent { target: String },
}

pub(super) fn diagnostic_log_due(
    last_at: Option<Instant>,
    now: Instant,
    throttle: Duration,
) -> bool {
    last_at.is_none_or(|last_at| {
        now.checked_duration_since(last_at)
            .is_none_or(|elapsed| elapsed >= throttle)
    })
}

pub(super) fn terminal_cell_metrics_refresh_needed(metrics: Option<(f32, f32)>) -> bool {
    metrics.is_none()
}

pub(super) fn pending_session_status_message(
    name: &str,
    auth_wait: Option<&PendingSessionAuthWait>,
) -> String {
    match auth_wait {
        Some(PendingSessionAuthWait::Credential { target }) => {
            format!("waiting for SSH credential for {target}")
        }
        Some(PendingSessionAuthWait::HostKey { host }) => {
            format!("waiting for SSH host key decision for {host}")
        }
        Some(PendingSessionAuthWait::Agent { target }) => {
            format!("waiting for SSH Agent approval for {target}")
        }
        None => format!("still connecting to {name}"),
    }
}

pub(super) fn runtime_tick_interval_for_pressure(output_pressure: bool) -> Duration {
    if output_pressure {
        RUNTIME_PRESSURE_TICK_INTERVAL
    } else {
        RUNTIME_IDLE_TICK_INTERVAL
    }
}

pub(super) fn viewport_change_terminal_session_ids(visible_session_ids: &[&str]) -> Vec<String> {
    visible_session_ids
        .iter()
        .copied()
        .filter(|session_id| !session_id.is_empty())
        .map(str::to_string)
        .collect()
}

pub(super) fn window_geometry_churn_active(
    last_viewport_change_at: Option<Instant>,
    now: Instant,
) -> bool {
    last_viewport_change_at.is_some_and(|at| {
        now.checked_duration_since(at)
            .is_some_and(|elapsed| elapsed < WINDOW_GEOMETRY_CHURN_HOLD)
    })
}

pub(super) fn connect_settle_active(until: Option<Instant>, now: Instant) -> bool {
    until.is_some_and(|until| now < until)
}

pub(super) fn connect_settle_deadline(now: Instant) -> Instant {
    now + CONNECT_SETTLE_HOLD
}

/// Whether a runtime tick should call cx.notify immediately.
/// Under pressure/settle, terminal-driven dirtiness is coalesced to ~30fps so
/// the full shell (title/tabs/status/sidebars) is not rebuilt every frame tick.
pub(super) fn runtime_ui_notify_allowed(
    visual_dirty: bool,
    pending_ui_notify: bool,
    force_immediate: bool,
    throttle_active: bool,
    last_ui_notify_at: Option<Instant>,
    now: Instant,
) -> bool {
    if force_immediate {
        return visual_dirty || pending_ui_notify;
    }
    if !visual_dirty && !pending_ui_notify {
        return false;
    }
    if !throttle_active {
        return true;
    }
    last_ui_notify_at.is_none_or(|last| {
        now.checked_duration_since(last)
            .is_none_or(|elapsed| elapsed >= UI_PAINT_THROTTLE)
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct RuntimeOutputPressureCounts {
    pub session_event_backlog_active: bool,
    pub session_event_queued_output_bytes: usize,
    pub pending_session_events: usize,
    pub bridge_queued_events: usize,
    pub bridge_queued_output_bytes: usize,
    pub pending_terminal_frame_events: usize,
    pub queued_terminal_frame_events: usize,
    pub queued_terminal_frame_output_bytes: usize,
}

pub(super) fn runtime_output_pressure_active_from_counts(
    counts: RuntimeOutputPressureCounts,
) -> bool {
    let RuntimeOutputPressureCounts {
        session_event_backlog_active,
        session_event_queued_output_bytes,
        pending_session_events,
        bridge_queued_events,
        bridge_queued_output_bytes,
        pending_terminal_frame_events,
        queued_terminal_frame_events,
        queued_terminal_frame_output_bytes,
    } = counts;
    session_event_backlog_active
        || session_event_queued_output_bytes > 0
        || pending_session_events > 0
        || bridge_queued_events > 0
        || bridge_queued_output_bytes > 0
        || pending_terminal_frame_events > 0
        || queued_terminal_frame_events > 0
        || queued_terminal_frame_output_bytes > 0
}

pub(super) fn session_event_drain_is_slow(total: Duration, max_chunk: Duration) -> bool {
    total >= SESSION_EVENT_DRAIN_SLOW_TOTAL || max_chunk >= SESSION_EVENT_DRAIN_SLOW_CHUNK
}

pub(super) fn session_event_drain_budget(output_pressure: bool) -> SessionEventDrainBudget {
    SessionEventDrainBudget {
        max_events: SESSION_EVENT_DRAIN_BATCH,
        max_output_bytes: if output_pressure {
            SESSION_EVENT_DRAIN_PRESSURE_OUTPUT_BUDGET
        } else {
            SESSION_EVENT_DRAIN_IDLE_OUTPUT_BUDGET
        },
        wall_budget: SESSION_EVENT_DRAIN_WALL_BUDGET,
    }
}

pub(super) fn session_event_input_wake_drain_budget() -> SessionEventDrainBudget {
    SessionEventDrainBudget {
        max_events: SESSION_EVENT_INPUT_WAKE_DRAIN_BATCH,
        max_output_bytes: SESSION_EVENT_INPUT_WAKE_OUTPUT_BUDGET,
        wall_budget: SESSION_EVENT_INPUT_WAKE_WALL_BUDGET,
    }
}

pub(super) fn session_event_backlog_active(
    drained_events: usize,
    drained_output_bytes: usize,
    queued_output_bytes: usize,
    budget: SessionEventDrainBudget,
) -> bool {
    drained_events >= budget.max_events
        || drained_output_bytes >= budget.max_output_bytes
        || queued_output_bytes > 0
}

pub(super) fn terminal_frame_backlog_active_from_counts(
    pending_events: usize,
    queued_events: usize,
    queued_commands: usize,
) -> bool {
    pending_events > 0 || queued_events > 0 || queued_commands > 0
}

pub(super) fn runtime_background_should_defer_terminal_frames(
    output_event_count: usize,
    drained_output_bytes: usize,
    terminal_frame_backlog_active: bool,
    terminal_frame_apply_paced: bool,
    user_scroll_frame_pending: bool,
    input_latency_active: bool,
) -> bool {
    if user_scroll_frame_pending || input_latency_active {
        return false;
    }
    let drained_output = output_event_count > 0 || drained_output_bytes > 0;
    drained_output && (!terminal_frame_backlog_active || terminal_frame_apply_paced)
}

/// How long before the data-plane drain task should look again, or `None` to park
/// on its wake.
///
/// This is the pacing the runtime tick used to provide, kept rather than removed:
/// the 16ms matches `RUNTIME_PRESSURE_TICK_INTERVAL` and the 50ms matches
/// `RUNTIME_IDLE_TICK_INTERVAL`, so a busy data plane comes back exactly as often
/// as it did on the tick. What disappears is the wake when there is nothing to do.
///
/// `protocol_runtime_sessions` is the trzsz/zmodem carve-out: those four worker
/// drains ride inside `drain_session_events` for output ordering and poll
/// `try_recv_event` with no wake of their own, so while a transfer is live the task
/// must keep coming back for them.
pub(super) fn runtime_data_plane_wake_delay(
    work_remaining: bool,
    protocol_runtime_sessions: bool,
) -> Option<Duration> {
    if work_remaining {
        return Some(TERMINAL_FRAME_APPLY_PRESSURE_INTERVAL);
    }
    if protocol_runtime_sessions {
        return Some(RUNTIME_IDLE_TICK_INTERVAL);
    }
    None
}

pub(super) fn terminal_frame_apply_should_defer(
    last_apply_at: Option<Instant>,
    now: Instant,
    output_pressure: bool,
    user_scroll_frame_pending: bool,
    input_latency_active: bool,
) -> bool {
    if user_scroll_frame_pending || input_latency_active {
        return false;
    }
    output_pressure
        && last_apply_at.is_some_and(|last_apply_at| {
            now.saturating_duration_since(last_apply_at) < TERMINAL_FRAME_APPLY_PRESSURE_INTERVAL
        })
}

pub(super) fn terminal_user_scroll_frame_apply_pending(
    last_user_scroll_at: Option<Instant>,
    visible_scrolled_surface: bool,
    now: Instant,
    active_window: Duration,
) -> bool {
    visible_scrolled_surface
        && last_user_scroll_at.is_some_and(|last| {
            now.checked_duration_since(last)
                .is_some_and(|elapsed| elapsed < active_window)
        })
}

pub(super) fn terminal_input_idle_remaining_delay(
    last_input_at: Option<Instant>,
    now: Instant,
    active_window: Duration,
) -> Option<Duration> {
    let last_input_at = last_input_at?;
    active_window
        .checked_sub(now.saturating_duration_since(last_input_at))
        .filter(|delay| !delay.is_zero())
}

pub(super) fn terminal_render_work_pressure_active(
    runtime_output_pressure: bool,
    pending_session_start: bool,
) -> bool {
    runtime_output_pressure || pending_session_start
}

pub(super) fn runtime_idle_plane_allowed(runtime_output_pressure: bool) -> bool {
    !runtime_output_pressure
}

pub(super) fn terminal_performance_tick_session_ids(visible_session_ids: &[&str]) -> Vec<String> {
    let mut ids = Vec::with_capacity(visible_session_ids.len());
    for session_id in visible_session_ids {
        if !session_id.is_empty() && !ids.iter().any(|id| id == session_id) {
            ids.push((*session_id).to_string());
        }
    }
    ids
}

pub(super) fn session_event_drain_should_yield(
    started_at: Instant,
    has_pending_events: bool,
    transport_queued_events: usize,
    transport_queued_output_bytes: usize,
    budget: SessionEventDrainBudget,
) -> bool {
    if started_at.elapsed() < budget.wall_budget {
        return false;
    }
    has_pending_events || transport_queued_events > 0 || transport_queued_output_bytes > 0
}

pub(super) fn runtime_background_event_drain_budget_exhausted(started_at: Instant) -> bool {
    started_at.elapsed() >= RUNTIME_BACKGROUND_EVENT_DRAIN_WALL_BUDGET
}

pub(super) fn remote_refresh_due(last_refresh_at: Option<Instant>, interval_seconds: u32) -> bool {
    last_refresh_at.is_none_or(|last_refresh_at| {
        last_refresh_at.elapsed() >= Duration::from_secs(u64::from(interval_seconds))
    })
}

pub(super) fn terminal_output_dropped_marker(bytes: usize) -> String {
    format!("\r\n[nyaterm: dropped {bytes} queued output byte(s)]\r\n")
}

pub(super) fn terminal_log_plain_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x1b' => out.push_str("\\x1b"),
            ch if ch.is_control() => {
                let _ = write!(out, "\\u{{{:x}}}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{
        CONNECT_SETTLE_HOLD, PendingSessionAuthWait, RUNTIME_BACKGROUND_EVENT_DRAIN_WALL_BUDGET,
        RUNTIME_IDLE_TICK_INTERVAL, RUNTIME_PRESSURE_TICK_INTERVAL, RuntimeOutputPressureCounts,
        SESSION_EVENT_DRAIN_BATCH, SESSION_EVENT_DRAIN_IDLE_OUTPUT_BUDGET,
        SESSION_EVENT_DRAIN_PRESSURE_OUTPUT_BUDGET, SESSION_EVENT_DRAIN_SLOW_CHUNK,
        SESSION_EVENT_DRAIN_SLOW_TOTAL, SESSION_EVENT_DRAIN_WALL_BUDGET, SLOW_DIAGNOSTIC_THROTTLE,
        TERMINAL_FRAME_APPLY_PRESSURE_INTERVAL, UI_PAINT_THROTTLE, WINDOW_GEOMETRY_CHURN_HOLD,
        connect_settle_active, connect_settle_deadline, diagnostic_log_due,
        pending_session_status_message, runtime_background_event_drain_budget_exhausted,
        runtime_background_should_defer_terminal_frames, runtime_data_plane_wake_delay,
        runtime_idle_plane_allowed, runtime_output_pressure_active_from_counts,
        runtime_tick_interval_for_pressure, runtime_ui_notify_allowed,
        session_event_backlog_active, session_event_drain_budget, session_event_drain_is_slow,
        session_event_drain_should_yield, session_event_input_wake_drain_budget,
        terminal_cell_metrics_refresh_needed, terminal_frame_apply_should_defer,
        terminal_frame_backlog_active_from_counts, terminal_input_idle_remaining_delay,
        terminal_log_plain_text, terminal_output_dropped_marker,
        terminal_performance_tick_session_ids, terminal_render_work_pressure_active,
        terminal_user_scroll_frame_apply_pending, viewport_change_terminal_session_ids,
        window_geometry_churn_active,
    };

    #[test]
    fn output_dropped_marker_is_plain_terminal_text() {
        let marker = terminal_output_dropped_marker(42);
        assert_eq!(
            marker,
            "\r\n[nyaterm: dropped 42 queued output byte(s)]\r\n"
        );
        assert!(marker.is_ascii());
        assert!(!marker.contains('\x1b'));
    }

    #[test]
    fn terminal_log_plain_text_escapes_control_sequences() {
        let message = "失败\x1b]52;c;AAAA\x07\nnext\tfield";
        let escaped = terminal_log_plain_text(message);

        assert_eq!(escaped, "失败\\x1b]52;c;AAAA\\u{7}\\nnext\\tfield");
        assert!(!escaped.contains('\x1b'));
        assert!(!escaped.contains('\x07'));
        assert!(!escaped.contains('\n'));
        assert!(escaped.contains("失败"));
    }

    #[test]
    fn diagnostic_log_due_respects_throttle_window() {
        let start = Instant::now();

        assert!(diagnostic_log_due(None, start, SLOW_DIAGNOSTIC_THROTTLE));
        assert!(!diagnostic_log_due(
            Some(start),
            start + Duration::from_millis(1999),
            SLOW_DIAGNOSTIC_THROTTLE
        ));
        assert!(diagnostic_log_due(
            Some(start),
            start + SLOW_DIAGNOSTIC_THROTTLE,
            SLOW_DIAGNOSTIC_THROTTLE
        ));
    }

    #[test]
    fn terminal_cell_metrics_refreshes_only_after_invalidation() {
        assert!(terminal_cell_metrics_refresh_needed(None));
        assert!(!terminal_cell_metrics_refresh_needed(Some((8.4, 18.0))));
    }

    #[test]
    fn pending_session_status_reports_auth_wait_reason() {
        assert_eq!(
            pending_session_status_message("server", None),
            "still connecting to server"
        );
        assert_eq!(
            pending_session_status_message(
                "server",
                Some(&PendingSessionAuthWait::Credential {
                    target: "user@example:22 (attempt 1)".to_string(),
                }),
            ),
            "waiting for SSH credential for user@example:22 (attempt 1)"
        );
        assert_eq!(
            pending_session_status_message(
                "server",
                Some(&PendingSessionAuthWait::HostKey {
                    host: "example:22".to_string(),
                }),
            ),
            "waiting for SSH host key decision for example:22"
        );
        assert_eq!(
            pending_session_status_message(
                "server",
                Some(&PendingSessionAuthWait::Agent {
                    target: "user@example:22".to_string(),
                }),
            ),
            "waiting for SSH Agent approval for user@example:22"
        );
    }

    #[test]
    fn runtime_tick_interval_uses_fast_cadence_under_output_pressure() {
        assert_eq!(
            runtime_tick_interval_for_pressure(false),
            RUNTIME_IDLE_TICK_INTERVAL
        );
        assert_eq!(
            runtime_tick_interval_for_pressure(true),
            RUNTIME_PRESSURE_TICK_INTERVAL
        );
    }

    #[test]
    fn viewport_change_terminal_session_ids_skips_empty_ids() {
        assert_eq!(
            viewport_change_terminal_session_ids(&["", "alpha", "beta"]),
            vec!["alpha".to_string(), "beta".to_string()]
        );
    }

    #[test]
    fn window_geometry_churn_holds_after_recent_viewport_change() {
        let now = Instant::now();
        assert!(!window_geometry_churn_active(None, now));
        assert!(window_geometry_churn_active(Some(now), now));
        assert!(!window_geometry_churn_active(
            Some(now - WINDOW_GEOMETRY_CHURN_HOLD - Duration::from_millis(1)),
            now
        ));
    }

    #[test]
    fn connect_settle_active_until_deadline() {
        let now = Instant::now();
        assert!(!connect_settle_active(None, now));
        assert!(connect_settle_active(
            Some(now + Duration::from_millis(100)),
            now
        ));
        assert!(!connect_settle_active(
            Some(now - Duration::from_millis(1)),
            now
        ));
    }

    #[test]
    fn runtime_ui_notify_throttles_under_pressure() {
        let now = Instant::now();
        assert!(!runtime_ui_notify_allowed(
            false, false, false, true, None, now
        ));
        assert!(runtime_ui_notify_allowed(
            true, false, false, true, None, now
        ));
        assert!(!runtime_ui_notify_allowed(
            true,
            false,
            false,
            true,
            Some(now),
            now
        ));
        assert!(runtime_ui_notify_allowed(
            true,
            false,
            false,
            true,
            Some(now - UI_PAINT_THROTTLE),
            now
        ));
        assert!(runtime_ui_notify_allowed(
            true,
            false,
            true,
            true,
            Some(now),
            now
        ));
        assert!(runtime_ui_notify_allowed(
            false,
            true,
            false,
            false,
            Some(now),
            now
        ));
    }

    #[test]
    fn runtime_output_pressure_tracks_output_and_frame_backlog_only() {
        assert!(!runtime_output_pressure_active_from_counts(
            RuntimeOutputPressureCounts::default()
        ));
        assert!(runtime_output_pressure_active_from_counts(
            RuntimeOutputPressureCounts {
                session_event_backlog_active: true,
                ..Default::default()
            }
        ));
        assert!(runtime_output_pressure_active_from_counts(
            RuntimeOutputPressureCounts {
                session_event_queued_output_bytes: 1,
                ..Default::default()
            }
        ));
        assert!(runtime_output_pressure_active_from_counts(
            RuntimeOutputPressureCounts {
                pending_session_events: 1,
                ..Default::default()
            }
        ));
        assert!(runtime_output_pressure_active_from_counts(
            RuntimeOutputPressureCounts {
                bridge_queued_events: 1,
                ..Default::default()
            }
        ));
        assert!(runtime_output_pressure_active_from_counts(
            RuntimeOutputPressureCounts {
                bridge_queued_output_bytes: 1,
                ..Default::default()
            }
        ));
        assert!(runtime_output_pressure_active_from_counts(
            RuntimeOutputPressureCounts {
                pending_terminal_frame_events: 1,
                ..Default::default()
            }
        ));
        assert!(runtime_output_pressure_active_from_counts(
            RuntimeOutputPressureCounts {
                queued_terminal_frame_events: 1,
                ..Default::default()
            }
        ));
        assert!(runtime_output_pressure_active_from_counts(
            RuntimeOutputPressureCounts {
                queued_terminal_frame_output_bytes: 1,
                ..Default::default()
            }
        ));
    }

    #[test]
    fn session_event_drain_slow_budget_flags_total_or_chunk() {
        assert!(!session_event_drain_is_slow(
            Duration::from_millis(19),
            Duration::from_millis(7)
        ));
        assert!(session_event_drain_is_slow(
            SESSION_EVENT_DRAIN_SLOW_TOTAL,
            Duration::from_millis(1)
        ));
        assert!(session_event_drain_is_slow(
            Duration::from_millis(1),
            SESSION_EVENT_DRAIN_SLOW_CHUNK
        ));
    }

    #[test]
    fn session_event_drain_budget_reduces_output_under_pressure() {
        let idle = session_event_drain_budget(false);
        let pressure = session_event_drain_budget(true);

        assert_eq!(idle.max_events, SESSION_EVENT_DRAIN_BATCH);
        assert_eq!(pressure.max_events, SESSION_EVENT_DRAIN_BATCH);
        assert_eq!(
            idle.max_output_bytes,
            SESSION_EVENT_DRAIN_IDLE_OUTPUT_BUDGET
        );
        assert_eq!(
            pressure.max_output_bytes,
            SESSION_EVENT_DRAIN_PRESSURE_OUTPUT_BUDGET
        );
        assert!(pressure.max_output_bytes < idle.max_output_bytes);
        assert_eq!(pressure.wall_budget, SESSION_EVENT_DRAIN_WALL_BUDGET);
    }

    #[test]
    fn session_event_input_wake_budget_prioritizes_ui_latency() {
        let regular = session_event_drain_budget(false);
        let input_wake = session_event_input_wake_drain_budget();

        assert!(input_wake.max_events < regular.max_events);
        assert!(input_wake.max_output_bytes < regular.max_output_bytes);
        assert!(input_wake.wall_budget < regular.wall_budget);
    }

    #[test]
    fn session_event_backlog_tracks_budget_without_user_status() {
        let budget = session_event_drain_budget(false);

        assert!(!session_event_backlog_active(1, 128, 0, budget));
        assert!(session_event_backlog_active(
            budget.max_events,
            128,
            0,
            budget
        ));
        assert!(session_event_backlog_active(
            1,
            budget.max_output_bytes,
            0,
            budget
        ));
        assert!(session_event_backlog_active(1, 128, 1, budget));
    }

    #[test]
    fn terminal_frame_backlog_tracks_pending_events_and_commands() {
        assert!(!terminal_frame_backlog_active_from_counts(0, 0, 0));
        assert!(terminal_frame_backlog_active_from_counts(1, 0, 0));
        assert!(terminal_frame_backlog_active_from_counts(0, 1, 0));
        assert!(terminal_frame_backlog_active_from_counts(0, 0, 1));
    }

    #[test]
    fn runtime_background_defers_terminal_frames_after_output() {
        assert!(!runtime_background_should_defer_terminal_frames(
            0, 0, false, false, false, false
        ));
        assert!(runtime_background_should_defer_terminal_frames(
            1, 0, false, false, false, false
        ));
        assert!(runtime_background_should_defer_terminal_frames(
            0, 1, false, false, false, false
        ));
        assert!(!runtime_background_should_defer_terminal_frames(
            1, 1024, true, true, true, false
        ));
    }

    #[test]
    fn runtime_background_does_not_starve_due_terminal_frame_apply() {
        assert!(runtime_background_should_defer_terminal_frames(
            1, 1024, true, true, false, false
        ));
        assert!(!runtime_background_should_defer_terminal_frames(
            1, 1024, true, false, false, false
        ));
        assert!(!runtime_background_should_defer_terminal_frames(
            1, 1024, true, true, false, true
        ));
    }

    #[test]
    fn terminal_frame_apply_pacing_only_defers_under_recent_pressure() {
        let now = Instant::now();

        assert!(!terminal_frame_apply_should_defer(
            None, now, true, false, false
        ));
        assert!(!terminal_frame_apply_should_defer(
            Some(now),
            now,
            false,
            false,
            false
        ));
        assert!(terminal_frame_apply_should_defer(
            Some(now),
            now,
            true,
            false,
            false
        ));
        assert!(!terminal_frame_apply_should_defer(
            Some(now),
            now + TERMINAL_FRAME_APPLY_PRESSURE_INTERVAL,
            true,
            false,
            false
        ));
        assert!(!terminal_frame_apply_should_defer(
            Some(now),
            now,
            true,
            true,
            false
        ));
        assert!(!terminal_frame_apply_should_defer(
            Some(now),
            now,
            true,
            false,
            true
        ));
    }

    #[test]
    fn runtime_data_plane_wake_delay_keeps_the_ticks_pacing() {
        // Remaining work is paced at the interval the pressure tick used, so a
        // burst that outruns one drain cycle still comes back on time rather than
        // looping tight or -- the failure this guards -- parking on a wake that
        // nothing is going to send.
        assert_eq!(
            runtime_data_plane_wake_delay(true, false),
            Some(TERMINAL_FRAME_APPLY_PRESSURE_INTERVAL)
        );
        assert_eq!(
            runtime_data_plane_wake_delay(true, true),
            Some(TERMINAL_FRAME_APPLY_PRESSURE_INTERVAL),
            "output pacing outranks the sideband poll"
        );
        // trzsz/zmodem worker queues have no wake of their own, so a live transfer
        // keeps the idle cadence the tick gave them.
        assert_eq!(
            runtime_data_plane_wake_delay(false, true),
            Some(RUNTIME_IDLE_TICK_INTERVAL)
        );
        // Nothing outstanding: park. This is the polling that goes away.
        assert_eq!(runtime_data_plane_wake_delay(false, false), None);
    }

    #[test]
    fn terminal_user_scroll_frame_apply_pending_tracks_recent_scrolled_surface() {
        let now = Instant::now();

        assert!(terminal_user_scroll_frame_apply_pending(
            Some(now),
            true,
            now,
            TERMINAL_FRAME_APPLY_PRESSURE_INTERVAL
        ));
        assert!(!terminal_user_scroll_frame_apply_pending(
            Some(now),
            false,
            now,
            TERMINAL_FRAME_APPLY_PRESSURE_INTERVAL
        ));
        assert!(!terminal_user_scroll_frame_apply_pending(
            Some(now - TERMINAL_FRAME_APPLY_PRESSURE_INTERVAL),
            true,
            now,
            TERMINAL_FRAME_APPLY_PRESSURE_INTERVAL
        ));
        assert!(!terminal_user_scroll_frame_apply_pending(
            None,
            true,
            now,
            TERMINAL_FRAME_APPLY_PRESSURE_INTERVAL
        ));
    }

    #[test]
    fn terminal_input_idle_remaining_delay_uses_remainder_of_window() {
        let now = Instant::now();
        let active_window = Duration::from_millis(80);

        assert_eq!(
            terminal_input_idle_remaining_delay(Some(now), now, active_window),
            Some(active_window)
        );
        assert_eq!(
            terminal_input_idle_remaining_delay(
                Some(now - Duration::from_millis(1)),
                now,
                active_window
            ),
            Some(active_window - Duration::from_millis(1))
        );
        assert_eq!(
            terminal_input_idle_remaining_delay(Some(now - active_window), now, active_window),
            None
        );
        assert_eq!(
            terminal_input_idle_remaining_delay(
                Some(now - active_window - Duration::from_millis(1)),
                now,
                active_window
            ),
            None
        );
        assert_eq!(
            terminal_input_idle_remaining_delay(None, now, active_window),
            None
        );
    }

    #[test]
    fn connect_settle_deadline_uses_hold_window() {
        let now = Instant::now();
        let deadline = connect_settle_deadline(now);

        assert!(connect_settle_active(Some(deadline), now));
        assert!(!connect_settle_active(
            Some(deadline),
            now + CONNECT_SETTLE_HOLD
        ));
    }

    #[test]
    fn terminal_render_work_pressure_includes_pending_connection_work() {
        assert!(!terminal_render_work_pressure_active(false, false));
        assert!(terminal_render_work_pressure_active(true, false));
        assert!(terminal_render_work_pressure_active(false, true));
    }

    #[test]
    fn runtime_idle_plane_waits_for_output_calm() {
        assert!(runtime_idle_plane_allowed(false));
        assert!(!runtime_idle_plane_allowed(true));
    }

    #[test]
    fn terminal_performance_ticks_only_visible_sessions() {
        assert_eq!(
            terminal_performance_tick_session_ids(&["a", "b", "a", ""]),
            vec!["a".to_string(), "b".to_string()]
        );
        assert!(terminal_performance_tick_session_ids(&[]).is_empty());
    }

    #[test]
    fn session_event_drain_yields_when_backlog_remains_after_wall_budget() {
        let start = Instant::now() - SESSION_EVENT_DRAIN_WALL_BUDGET;
        let budget = session_event_drain_budget(true);

        assert!(!session_event_drain_should_yield(
            start, false, 0, 0, budget
        ));
        assert!(session_event_drain_should_yield(start, true, 0, 0, budget));
        assert!(session_event_drain_should_yield(start, false, 1, 0, budget));
        assert!(session_event_drain_should_yield(start, false, 0, 1, budget));
        assert!(!session_event_drain_should_yield(
            Instant::now(),
            true,
            1,
            1,
            budget
        ));
    }

    #[test]
    fn runtime_background_event_drain_budget_exhaustion_tracks_elapsed_time() {
        let start = Instant::now() - RUNTIME_BACKGROUND_EVENT_DRAIN_WALL_BUDGET;

        assert!(runtime_background_event_drain_budget_exhausted(start));
        assert!(!runtime_background_event_drain_budget_exhausted(
            Instant::now()
        ));
    }
}
