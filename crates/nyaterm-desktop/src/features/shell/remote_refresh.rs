//! Auto-refresh for the remote panels that poll a host.
//!
//! Stats, GPU, NPU, Processes, Docker and the transfer browser's cwd sync all refresh
//! on user-configured intervals while their panel is open. That is genuinely periodic
//! work -- there is no push from the remote host -- so it stays a poll.
//!
//! It used to be the runtime tick's idle plane, which is the last thing that kept the
//! tick alive. This clock is scoped to "some panel actually wants refreshing", which
//! means an app with no remote panel open costs nothing.
//!
//! **This is an interim owner.** The design puts these timers on the panel entities
//! that Phase 4 extracts, armed on mount and dropped on unmount, which is strictly
//! better: the panel that wants the data owns the timer that fetches it. Keeping the
//! shape here -- one scoped clock over a "does anything need this" predicate -- is what
//! Phase 4 relocates rather than redesigns, and it lets Phase 3 delete the tick without
//! waiting for that extraction.

use std::time::Duration;

use gpui::Context;

use crate::features::NyaTermApp;
use crate::models::NavItem;

/// How often to check whether any panel's refresh interval has come due.
///
/// The per-panel intervals are user settings in whole seconds, floored at one, so a
/// one-second clock is exactly as fine as the finest thing it can service. Each panel
/// still gates itself on its own interval; this only decides how often that is asked.
const REMOTE_REFRESH_POLL_INTERVAL: Duration = Duration::from_secs(1);

impl NyaTermApp {
    /// Refresh the remote panels while any of them is open.
    ///
    /// Idempotent. Armed from `render`, because what it depends on -- which panel is
    /// showing, and whether a session with an SSH config is active -- changes only
    /// alongside a repaint.
    pub(in crate::features) fn ensure_remote_refresh_clock(&mut self, cx: &mut Context<Self>) {
        if self.shell.remote_refresh_clock_is_armed() || !self.remote_panels_need_refresh() {
            return;
        }
        self.shell.set_remote_refresh_clock_armed(true);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(REMOTE_REFRESH_POLL_INTERVAL)
                    .await;
                // `update_in`: each refresh submits a remote job, which needs the
                // window.
                let Ok(keep_running) = this.update_in(cx, |this, window, cx| {
                    if this.drive_remote_auto_refresh(window, cx) {
                        cx.notify();
                    }
                    let running = this.remote_panels_need_refresh();
                    if !running {
                        this.shell.set_remote_refresh_clock_armed(false);
                    }
                    running
                }) else {
                    break;
                };
                if !keep_running {
                    break;
                }
            }
        })
        .detach();
    }

    /// Whether any remote panel or header mode currently wants periodic refreshing.
    ///
    /// Lifted from the runtime tick, which computed exactly this to decide whether its
    /// calm branch could skip the idle plane.
    pub(in crate::features) fn remote_panels_need_refresh(&self) -> bool {
        (self.session.active_ssh_config().is_some()
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
            || self.current_left_panel() == Some(NavItem::Transfers)
    }
}
