use std::time::{Duration, Instant};

use futures::StreamExt;
use gpui::{Context, Window};

use crate::features::shell::event_pump::helpers::{
    PENDING_SESSION_STATUS_INTERVAL, PENDING_SESSION_STILL_CONNECTING_AFTER,
    PendingSessionAuthWait, RUNTIME_IDLE_TICK_INTERVAL, RUNTIME_QUIET_TICK_INTERVAL,
    RuntimeOutputPressureCounts, SLOW_DIAGNOSTIC_THROTTLE, TITLE_DRAG_ACTIVE_HOLD,
    TRANSFER_AUTO_SYNC_CWD_INTERVAL_SECONDS, connect_settle_active, connect_settle_deadline,
    pending_session_status_message, remote_refresh_due, runtime_output_pressure_active_from_counts,
    runtime_tick_interval_for_pressure, runtime_ui_notify_allowed,
    terminal_cell_metrics_refresh_needed, terminal_input_idle_remaining_delay,
    viewport_change_terminal_session_ids, window_geometry_churn_active,
};
use crate::features::{
    NyaTermApp, session::credential_prompt_target, session::keyboard_interactive_prompt_target,
    text_inputs::TextInputSetup,
};
use crate::models::{HeaderStatusMode, NavItem};

mod bridge;
mod helpers;
mod planes;
mod session_events;

use crate::features::terminal::terminal_runtime::TERMINAL_INPUT_LATENCY_WINDOW;

// These intervals produce wake deadlines at 4ms, 12ms, and 24ms. The timer
// calls below are sequential, so storing the absolute deadlines here would
// accidentally move the final echo poll out to 40ms.
const TERMINAL_INPUT_WAKE_INTERVALS: [Duration; 3] = [
    Duration::from_millis(4),
    Duration::from_millis(8),
    Duration::from_millis(12),
];

impl NyaTermApp {
    pub(in crate::features) fn start_terminal_frame_event_wake(&mut self, cx: &mut Context<Self>) {
        let Some(mut wake_rx) = self.terminal.take_frame_event_wake_receiver() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            while wake_rx.next().await.is_some() {
                if this
                    .update(cx, |this, cx| {
                        this.drain_terminal_frame_event_wake(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(in crate::features) fn refresh_window_render_inputs(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let before_metrics = self.terminal.cell_metrics();
        let vs = window.viewport_size();
        let viewport_changed = self
            .shell
            .viewport
            .update_size((f32::from(vs.width), f32::from(vs.height)), Instant::now());
        if viewport_changed {
            // Geometry churn (resize / some window managers during move).
            self.notify_terminal_surfaces_for_viewport_change(cx);
        }
        if terminal_cell_metrics_refresh_needed(self.terminal.cell_metrics()) {
            self.refresh_terminal_cell_metrics(cx);
        }
        if self.terminal.cell_metrics() != before_metrics {
            self.sync_terminal_cell_metrics_to_screens();
            self.resize_all_known_terminal_surfaces();
            self.refresh_visible_terminal_surfaces(cx);
        }
        viewport_changed || self.terminal.cell_metrics() != before_metrics
    }

    fn notify_terminal_surfaces_for_viewport_change(&mut self, cx: &mut Context<Self>) {
        let session_ids =
            viewport_change_terminal_session_ids(&self.visible_terminal_session_ids());
        for session_id in session_ids {
            self.notify_terminal_surface_only(Some(session_id.as_str()), cx);
        }
    }

    pub(in crate::features) fn mark_user_activity(&mut self) {
        self.security.record_screen_lock_user_activity();
    }

    pub(in crate::features) fn arm_terminal_input_wake(&mut self, cx: &mut Context<Self>) {
        self.mark_terminal_input_latency_activity(cx);
        self.shell.runtime.terminal_input_wake_generation = self
            .shell
            .runtime
            .terminal_input_wake_generation
            .saturating_add(1);
        if self.shell.runtime.terminal_input_wake_armed {
            return;
        }
        self.shell.runtime.terminal_input_wake_armed = true;
        // Keep key dispatch limited to encoding and the PTY notifier. Pull
        // through already queued echo after the current input event. The armed
        // state coalesces a burst of keys into one deferred drain.
        let app = cx.entity();
        cx.defer(move |cx| {
            app.update(cx, |this, cx| {
                this.drain_terminal_input_wake(cx);
            });
        });
        let mut observed_generation = self.shell.runtime.terminal_input_wake_generation;
        cx.spawn(async move |this, cx| {
            loop {
                for delay in TERMINAL_INPUT_WAKE_INTERVALS {
                    cx.background_executor().timer(delay).await;
                    let _ = this.update(cx, |this, cx| {
                        this.drain_terminal_input_wake(cx);
                    });
                }
                let (next_generation, finished) = this
                    .update(cx, |this, _| {
                        let next_generation = this.shell.runtime.terminal_input_wake_generation;
                        if next_generation == observed_generation {
                            this.shell.runtime.terminal_input_wake_armed = false;
                            (next_generation, true)
                        } else {
                            (next_generation, false)
                        }
                    })
                    .unwrap_or((observed_generation, true));
                if finished {
                    break;
                }
                observed_generation = next_generation;
            }
        })
        .detach();
    }

    fn drain_terminal_input_wake(&mut self, cx: &mut Context<Self>) {
        let chrome_dirty = self.drain_session_events_for_input_wake(cx)
            | self.drain_terminal_frame_events_for_input_wake(cx);
        if chrome_dirty {
            cx.notify();
            self.shell.runtime.last_ui_notify_at = Some(Instant::now());
            self.shell.runtime.pending_ui_notify = false;
        }
    }

    fn drain_terminal_frame_event_wake(&mut self, cx: &mut Context<Self>) {
        let chrome_dirty = self.drain_terminal_frame_events(cx);
        if chrome_dirty {
            cx.notify();
            self.shell.runtime.last_ui_notify_at = Some(Instant::now());
            self.shell.runtime.pending_ui_notify = false;
        }
    }

    fn mark_terminal_input_latency_activity(&mut self, cx: &mut Context<Self>) {
        self.shell.runtime.last_terminal_input_at = Some(Instant::now());
        if let Some(session_id) = self
            .session
            .active_id()
            .filter(|session_id| !session_id.is_empty())
        {
            self.shell
                .runtime
                .pending_terminal_input_idle_sessions
                .insert(session_id.to_string());
        }
        if self.shell.runtime.terminal_input_idle_notify_armed {
            return;
        }
        self.shell.runtime.terminal_input_idle_notify_armed = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(TERMINAL_INPUT_LATENCY_WINDOW)
                .await;
            let _ = this.update(cx, |this, cx| {
                this.flush_terminal_input_idle_notify(cx);
            });
        })
        .detach();
    }

    fn flush_terminal_input_idle_notify(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        if let Some(delay) = terminal_input_idle_remaining_delay(
            self.shell.runtime.last_terminal_input_at,
            now,
            TERMINAL_INPUT_LATENCY_WINDOW,
        ) {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(delay).await;
                let _ = this.update(cx, |this, cx| {
                    this.flush_terminal_input_idle_notify(cx);
                });
            })
            .detach();
            return;
        }
        self.shell.runtime.terminal_input_idle_notify_armed = false;
        let session_ids = self
            .shell
            .runtime
            .pending_terminal_input_idle_sessions
            .drain()
            .collect::<Vec<_>>();
        for session_id in session_ids {
            self.notify_terminal_surface_only(Some(session_id.as_str()), cx);
        }
    }

    pub(in crate::features) fn mark_title_drag_activity(&mut self) {
        self.shell
            .viewport
            .mark_title_drag(Instant::now(), TITLE_DRAG_ACTIVE_HOLD);
    }

    pub(in crate::features) fn title_drag_active(&self, now: Instant) -> bool {
        self.shell.viewport.title_drag_active(now)
    }

    pub(in crate::features) fn should_log_slow_diagnostic(
        &mut self,
        key: &'static str,
        now: Instant,
    ) -> bool {
        self.shell
            .diagnostics
            .should_log(key, now, SLOW_DIAGNOSTIC_THROTTLE)
    }

    pub(in crate::features) fn visible_terminal_layout_cache_stats(&self) -> (u64, u64) {
        self.terminal
            .visible_layout_cache_stats(self.visible_terminal_session_ids())
    }

    pub(in crate::features) fn drive_idle_lock(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.security.screen_locked()
            || !self.settings.summary().enable_screen_lock
            || self.settings.summary().idle_lock_minutes == 0
        {
            return false;
        }
        let idle_for = self.security.screen_lock_idle_for();
        let lock_after =
            Duration::from_secs(u64::from(self.settings.summary().idle_lock_minutes) * 60);
        if idle_for < lock_after {
            return false;
        }
        let lock_status = if self.settings.summary().has_master_password {
            "Enter the master password to unlock.".to_string()
        } else {
            "No master password is configured.".to_string()
        };
        self.security.activate_screen_lock(lock_status);
        self.forget_text_inputs("lock-screen.password");
        self.shell.set_status(format!(
            "screen locked after {} minute(s) idle",
            self.settings.summary().idle_lock_minutes
        ));
        if self.settings.summary().has_master_password {
            let field = self.text_input("lock-screen.password", "", TextInputSetup::masked(), cx);
            window.focus(&field.read(cx).focus_handle(), cx);
        } else {
            window.focus(self.security.screen_lock_focus(), cx);
        }
        true
    }

    pub(crate) fn mark_window_runtime_started(&mut self) {
        self.shell.runtime.event_pump_started = true;
    }

    pub(crate) fn window_runtime_running(&self) -> bool {
        self.shell.runtime.event_pump_started
    }

    pub(crate) fn window_runtime_tick_delay(&self) -> Duration {
        // During recent viewport geometry churn (window resize/drag), prefer the
        // idle cadence so full plane ticks do not stack on compositor paints.
        let now = Instant::now();
        if self.title_drag_active(now)
            || window_geometry_churn_active(self.shell.viewport.last_change_at, now)
        {
            return RUNTIME_IDLE_TICK_INTERVAL;
        }
        if self.runtime_quiet_tick_allowed() {
            // The quiet cadence is coarser than CURSOR_BLINK_INTERVAL, so a tick
            // that lands just before the blink deadline pushes the toggle out to
            // the following quiet tick and stretches the visible half-period to
            // roughly twice its setting. Wake at the deadline instead. The delay
            // is recomputed every loop iteration, so this costs one extra wake
            // per blink rather than a permanently faster cadence.
            return match self.cursor_blink_wake_delay(now) {
                Some(delay) => delay.min(RUNTIME_QUIET_TICK_INTERVAL),
                None => RUNTIME_QUIET_TICK_INTERVAL,
            };
        }
        runtime_tick_interval_for_pressure(self.runtime_output_pressure_active())
    }

    /// Time until the caret should next toggle, or `None` when no blink is due.
    ///
    /// Returns `None` during connect settle because the visual plane deliberately
    /// holds the blink phase there; waking for a deadline that will not be
    /// honoured would spin on an already-elapsed instant.
    fn cursor_blink_wake_delay(&self, now: Instant) -> Option<Duration> {
        if connect_settle_active(self.shell.runtime.connect_settle_until, now) {
            return None;
        }
        if !self.settings.summary().cursor_blink || self.visible_terminal_session_ids().is_empty() {
            return None;
        }
        Some(
            self.shell
                .runtime
                .cursor_blink_next_at?
                .saturating_duration_since(now),
        )
    }

    pub(crate) fn window_runtime_tick_needs_update(
        &self,
        viewport_size: (f32, f32),
        now: Instant,
    ) -> bool {
        if !self.shell.runtime.event_pump_started {
            return false;
        }
        if self.shell.viewport.size != viewport_size
            || terminal_cell_metrics_refresh_needed(self.terminal.cell_metrics())
        {
            return true;
        }
        if self
            .shell
            .runtime
            .connect_settle_until
            .is_some_and(|until| now >= until)
        {
            return true;
        }
        if self.title_drag_active(now) {
            return true;
        }

        let output_pressure = self.runtime_output_pressure_active();
        let connect_settle = connect_settle_active(self.shell.runtime.connect_settle_until, now);
        if runtime_ui_notify_allowed(
            false,
            self.shell.runtime.pending_ui_notify,
            false,
            output_pressure || connect_settle,
            self.shell.runtime.last_ui_notify_at,
            now,
        ) {
            return true;
        }
        if !self.runtime_quiet_tick_allowed() {
            return true;
        }
        self.window_runtime_quiet_tick_has_due_work(now)
    }

    fn window_runtime_quiet_tick_has_due_work(&self, now: Instant) -> bool {
        if self.header_status_clock_refresh_due() {
            return true;
        }
        if self.ai.chat_focus_is_pending()
            || self.transfer.rename_focus_is_pending()
            || self.session.prompt_credential_focus_is_pending()
        {
            return true;
        }
        if self.terminal.terminal_file_drop_hover_is_pending() {
            return true;
        }
        if self.transfer.browser_external_drop_hover_is_pending() {
            return true;
        }
        if self.settings.summary().cursor_blink
            && !self.visible_terminal_session_ids().is_empty()
            && self
                .shell
                .runtime
                .cursor_blink_next_at
                .is_some_and(|next| now >= next)
        {
            return true;
        }
        if self.visible_terminal_performance_recovery_due() {
            return true;
        }
        self.terminal_render_requests_pending()
    }

    fn visible_terminal_performance_recovery_due(&self) -> bool {
        self.terminal
            .visible_performance_recovery_due(self.visible_terminal_session_ids())
    }

    fn terminal_render_requests_pending(&self) -> bool {
        let visible_session_ids = self.visible_terminal_session_ids();
        if self
            .terminal
            .visible_live_snapshot_missing(visible_session_ids.iter().copied())
        {
            return true;
        }
        if visible_session_ids
            .iter()
            .any(|session_id| self.terminal_visual_scroll_active_for_session(Some(session_id)))
        {
            return true;
        }
        if !self.terminal.buffer_search_is_open() {
            return false;
        }
        let Some(session_id) = self.session.active_id() else {
            return false;
        };
        let Some(key) = self.terminal_search_key() else {
            return false;
        };
        self.terminal.search_refresh_is_due(session_id, &key)
    }

    pub(in crate::features) fn enter_connect_settle(&mut self) {
        self.shell.runtime.connect_settle_until = Some(connect_settle_deadline(Instant::now()));
    }

    pub(in crate::features) fn runtime_output_pressure_active(&self) -> bool {
        let frame = self.terminal.frame_queue_metrics();
        runtime_output_pressure_active_from_counts(RuntimeOutputPressureCounts {
            session_event_backlog_active: self.shell.runtime.session_event_backlog_active,
            session_event_queued_output_bytes: self.shell.runtime.session_event_queued_output_bytes,
            pending_session_events: self.session.pending_event_count(),
            bridge_queued_events: self.session.event_bridge_queued_event_count()
                + self.session.event_bridge_source_queued_event_count(),
            bridge_queued_output_bytes: self.session.event_bridge_queued_output_bytes()
                + self.session.event_bridge_source_queued_output_bytes(),
            pending_terminal_frame_events: frame.pending_event_count,
            queued_terminal_frame_events: frame.event_count,
            queued_terminal_frame_output_bytes: frame.output_bytes,
        })
    }

    pub(in crate::features) fn runtime_quiet_tick_allowed(&self) -> bool {
        !self.runtime_output_pressure_active()
            && !self.session.start_has_pending()
            && self.session.pending_events_are_empty()
            && !self.session.event_bridge_has_pending_ui_work()
            && !self.terminal_frame_backlog_active()
            && !self.session.has_protocol_runtime_sessions()
            && !self.session.prompt_has_pending_or_active_prompt()
            && !self.terminal.action_link_hover_is_pending()
            && !self.recording.has_pending_auto_start()
            && self.transfer.transfer_jobs_are_empty()
            && !self.shell.runtime.open_tabs_persist_dirty
            && !self.shell.runtime.window_layout_persist_dirty
            && self.terminal.terminal_windows_restore_is_complete()
            && !self.ai.has_background_work()
            && !self.terminal.history_search_is_pending()
            && !self.ai.chat_focus_is_pending()
            && !self.transfer.rename_focus_is_pending()
            && !self.session.prompt_credential_focus_is_pending()
            && !((self.session.active_ssh_config().is_some()
                && matches!(
                    self.current_right_panel(),
                    Some(
                        NavItem::Stats
                            | NavItem::GpuMonitor
                            | NavItem::AscendNpuMonitor
                            | NavItem::Processes
                            | NavItem::Docker
                    )
                ))
                || self.current_left_panel() == Some(NavItem::Transfers))
    }

    pub(super) fn drive_pending_session_status(&mut self) -> bool {
        let Some((name, requested_at)) = self.session.start_pending_status_source() else {
            self.shell.runtime.last_pending_session_status_at = None;
            return false;
        };
        let auth_wait = self.pending_session_auth_wait();
        if auth_wait.is_none() && requested_at.elapsed() < PENDING_SESSION_STILL_CONNECTING_AFTER {
            return false;
        }
        let now = Instant::now();
        if self
            .shell
            .runtime
            .last_pending_session_status_at
            .is_some_and(|last_at| {
                now.saturating_duration_since(last_at) < PENDING_SESSION_STATUS_INTERVAL
            })
        {
            return false;
        }
        self.shell.runtime.last_pending_session_status_at = Some(now);
        let message = pending_session_status_message(&name, auth_wait.as_ref());
        if self.shell.status() == message {
            return false;
        }
        self.shell.set_status(message);
        true
    }

    pub(super) fn drive_remote_auto_refresh(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.session.active_ssh_config().is_none() {
            return false;
        }

        let mut dirty = false;
        let left_panel = self.current_left_panel();
        let right_panel = self.current_right_panel();

        if (right_panel == Some(NavItem::Stats) || self.header_status_needs_remote_stats())
            && self.settings.summary().ui_show_remote_stats
            && !self.remote_ops.stats_is_pending()
            && remote_refresh_due(
                self.remote_ops.stats_last_refresh_at(),
                self.settings.summary().ui_remote_stats_interval.max(1),
            )
        {
            self.refresh_stats(window, cx);
            dirty = true;
        }

        if (right_panel == Some(NavItem::GpuMonitor) || self.header_status_needs_gpu())
            && self.settings.summary().ui_show_gpu_monitor
            && !self.remote_ops.gpu_is_pending()
            && remote_refresh_due(
                self.remote_ops.gpu_last_refresh_at(),
                self.settings.summary().ui_gpu_monitor_interval.max(1),
            )
        {
            self.refresh_gpu_auto(window, cx);
            dirty = true;
        }

        if (right_panel == Some(NavItem::AscendNpuMonitor) || self.header_status_needs_npu())
            && self.settings.summary().ui_show_ascend_npu_monitor
            && !self.remote_ops.npu_is_pending()
            && remote_refresh_due(
                self.remote_ops.npu_last_refresh_at(),
                self.settings
                    .summary()
                    .ui_ascend_npu_monitor_interval
                    .max(1),
            )
        {
            self.refresh_npu_auto(window, cx);
            dirty = true;
        }

        if right_panel == Some(NavItem::Processes)
            && self.settings.summary().ui_show_process_manager
            && !self.remote_ops.process_is_pending()
            && remote_refresh_due(
                self.remote_ops.process_last_refresh_at(),
                self.settings.summary().ui_process_manager_interval.max(3),
            )
        {
            self.refresh_processes(window, cx);
            dirty = true;
        } else if right_panel == Some(NavItem::Docker)
            && self.settings.summary().ui_show_docker_manager
            && !self.remote_ops.docker_is_pending()
        {
            let interval = self.settings.summary().ui_docker_manager_interval.max(3);
            if remote_refresh_due(self.remote_ops.docker_last_refresh_at(), interval) {
                self.refresh_docker(window, cx);
                dirty = true;
            } else if let Some((container_id, last_refresh_at)) =
                self.remote_ops.docker_details_refresh()
                && remote_refresh_due(Some(last_refresh_at), interval)
            {
                self.load_docker_details(container_id, window, cx);
                dirty = true;
            }
        }

        if left_panel == Some(NavItem::Transfers)
            && self.transfer_browser_auto_sync_cwd_enabled()
            && !self.transfer_sync_cwd_job_running()
            && remote_refresh_due(
                self.transfer.browser_auto_sync_cwd_last_at(),
                TRANSFER_AUTO_SYNC_CWD_INTERVAL_SECONDS,
            )
        {
            self.transfer.mark_browser_auto_sync_cwd(Instant::now());
            self.start_transfer_sync_cwd_job(window, cx);
            dirty = true;
        }
        dirty
    }

    fn header_status_needs_gpu(&self) -> bool {
        self.settings.summary().ui_header_status_visible
            && HeaderStatusMode::from_setting(&self.settings.summary().ui_header_status_mode)
                == HeaderStatusMode::Gpu
    }

    fn header_status_needs_npu(&self) -> bool {
        self.settings.summary().ui_header_status_visible
            && HeaderStatusMode::from_setting(&self.settings.summary().ui_header_status_mode)
                == HeaderStatusMode::Npu
    }

    fn pending_session_auth_wait(&self) -> Option<PendingSessionAuthWait> {
        if let Some(prompt) = self.session.prompt_active_agent() {
            return Some(PendingSessionAuthWait::Agent {
                target: format!(
                    "{}@{}:{}",
                    prompt.prompt.username, prompt.prompt.host, prompt.prompt.port
                ),
            });
        }
        if let Some(prompt) = self.session.prompt_active_keyboard_interactive() {
            return Some(PendingSessionAuthWait::Credential {
                target: keyboard_interactive_prompt_target(&prompt.request),
            });
        }
        if let Some(prompt) = self.session.prompt_active_credential() {
            return Some(PendingSessionAuthWait::Credential {
                target: credential_prompt_target(&prompt.prompt),
            });
        }
        if let Some(prompt) = self.session.prompt_active_host_key() {
            return Some(PendingSessionAuthWait::HostKey {
                host: prompt.host_key.host_identifier.clone(),
            });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use gpui::{AppContext as _, TestAppContext};
    use nyaterm_core::{AiExecutionProfile, AppRuntime, RuntimeMode};
    use nyaterm_transport::LocalSessionConfig;

    use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
    use crate::features::NyaTermApp;
    use crate::models::{
        GithubGistAuthEvent, GithubGistAuthJobEvent, SessionLaunchConfig, SessionRuntimeMetadata,
        TerminalSearchMode,
    };

    use super::helpers::{CURSOR_BLINK_INTERVAL, RUNTIME_QUIET_TICK_INTERVAL};

    const SESSION_ID: &str = "event-pump-session";

    /// Windows clock granularity is coarse enough (~15ms) that a wall-clock
    /// timestamp alone collides between tests started in the same tick, and two
    /// apps sharing a directory fight over the same redb file. Keep a counter.
    static TEST_DIR_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn unique_test_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        let sequence = TEST_DIR_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "nyaterm-event-pump-{}-{nanos}-{sequence}",
            std::process::id()
        ))
    }

    /// A workspace with one visible local session and nothing outstanding: the
    /// state `runtime_quiet_tick_allowed` is meant to recognise.
    fn quiet_app_with_visible_session(cx: &mut TestAppContext) -> gpui::Entity<NyaTermApp> {
        let root = unique_test_dir();
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
        let app = cx.new(|cx| NyaTermApp::new(runtime, stores, cx));
        cx.update_entity(&app, |app, _| {
            app.session.register_session_metadata(
                SESSION_ID,
                SessionRuntimeMetadata {
                    ssh_config: None,
                    ssh_multiplex_key: None,
                    source_connection_id: None,
                    ai_execution_profile: AiExecutionProfile::Posix,
                    launch_config: SessionLaunchConfig::Local(LocalSessionConfig {
                        name: "Local session".to_string(),
                        ..LocalSessionConfig::default()
                    }),
                    disconnected: false,
                },
            );
            app.session.select_active_session(SESSION_ID);
            app.terminal
                .seed_session_view(SESSION_ID.to_string(), String::new(), "UTF-8");
            app.shell.show_workspace();
            // A fresh app starts with the terminal window layout restore
            // outstanding, which is itself a reason to stay off the quiet cadence.
            app.terminal.complete_terminal_windows_restore();
            assert!(
                app.runtime_quiet_tick_allowed(),
                "fixture must start on the quiet cadence for these tests to mean anything"
            );
        });
        app
    }

    #[test]
    fn quiet_tick_wakes_on_the_cursor_blink_deadline_instead_of_skipping_it() {
        let mut cx = TestAppContext::single();
        let app = quiet_app_with_visible_session(&mut cx);
        cx.update_entity(&app, |app, _| {
            let mut summary = app.settings.summary().clone();
            summary.cursor_blink = true;
            app.settings.replace_summary(summary);
            assert!(!app.visible_terminal_session_ids().is_empty());

            // A deadline beyond the quiet cadence still yields the quiet cadence;
            // the delay is recomputed each loop iteration, so nothing is lost.
            app.shell.runtime.cursor_blink_next_at = Some(Instant::now() + CURSOR_BLINK_INTERVAL);
            assert_eq!(app.window_runtime_tick_delay(), RUNTIME_QUIET_TICK_INTERVAL);

            // Once the remainder falls inside the quiet cadence the tick must land
            // on the deadline. Returning the full quiet interval here is what
            // pushed the toggle to the following tick and stretched the visible
            // blink half-period to roughly twice CURSOR_BLINK_INTERVAL.
            let remaining = Duration::from_millis(30);
            app.shell.runtime.cursor_blink_next_at = Some(Instant::now() + remaining);
            let delay = app.window_runtime_tick_delay();
            assert!(
                delay <= remaining,
                "delay {delay:?} skips a blink deadline {remaining:?} away"
            );
        });
    }

    #[test]
    fn quiet_tick_holds_the_blink_phase_during_connect_settle() {
        let mut cx = TestAppContext::single();
        let app = quiet_app_with_visible_session(&mut cx);
        cx.update_entity(&app, |app, _| {
            let mut summary = app.settings.summary().clone();
            summary.cursor_blink = true;
            app.settings.replace_summary(summary);
            // The visual plane deliberately freezes blink during connect settle, so
            // an elapsed deadline must not pull the tick delay down to zero.
            app.shell.runtime.cursor_blink_next_at =
                Some(Instant::now() - Duration::from_millis(1));
            app.enter_connect_settle();
            assert_eq!(app.window_runtime_tick_delay(), RUNTIME_QUIET_TICK_INTERVAL);
        });
    }

    /// The replacement for the quiet-gate term this queue used to need.
    ///
    /// No window runtime tick exists in this fixture at all, and
    /// `run_until_parked` advances the executor without advancing the clock, so
    /// the event can only have arrived through the drain task. That is what makes
    /// the old 500ms trap structurally impossible here: a queue with its own
    /// drain task needs no central predicate to stay responsive.
    #[test]
    fn github_gist_auth_events_arrive_without_any_timer() {
        let mut cx = TestAppContext::single();
        let app = quiet_app_with_visible_session(&mut cx);
        let (tx, job_id) = cx.update_entity(&app, |app, cx| {
            let start = app
                .cloud_sync
                .begin_github_auth_for_test()
                .expect("the device flow should start");
            let handles = (start.sender(), start.job_id());
            app.start_github_gist_auth_event_drain(cx);
            handles
        });

        tx.unbounded_send(GithubGistAuthJobEvent {
            job_id,
            event: GithubGistAuthEvent::Polling { slow_down: true },
        })
        .expect("the drain task holds the receiver");
        cx.run_until_parked();

        cx.update_entity(&app, |app, _| {
            assert_eq!(
                app.cloud_sync.github_auth().message.as_deref(),
                Some(rust_i18n::t!("settings.githubGistSlowDown").as_ref()),
                "the polling event should already be applied"
            );
        });
    }

    #[test]
    fn quiet_tick_is_blocked_while_recording_history_search_is_pending() {
        let mut cx = TestAppContext::single();
        let app = quiet_app_with_visible_session(&mut cx);
        cx.update_entity(&app, |app, cx| {
            app.terminal.open_search_for_test();
            app.terminal.set_search_mode(TerminalSearchMode::History);
            app.apply_terminal_search_query("needle".to_string(), cx);
            assert!(
                app.terminal.history_search_is_pending(),
                "fixture must arm a history search"
            );
            assert!(
                !app.runtime_quiet_tick_allowed(),
                "the reply is polled by drain_recording_pipeline_events, so the runtime \
                 must not idle at the quiet cadence while one is outstanding"
            );
        });
    }
}
