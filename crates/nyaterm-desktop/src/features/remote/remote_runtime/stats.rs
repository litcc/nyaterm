use futures::StreamExt as _;
use gpui::{Context, Window};
use nyaterm_transport::RemoteStatsService;

use crate::features::NyaTermApp;
use crate::features::runtime_jobs::StatsJobResult;

impl NyaTermApp {
    pub(in crate::features) fn refresh_stats(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(config) = self.session.active_ssh_config_owned() else {
            self.remote_ops
                .set_stats_status("start an SSH session before inspecting stats");
            self.shell
                .set_status(self.remote_ops.stats_status().to_string());
            cx.notify();
            return;
        };
        if !config.remote_stats_enabled() {
            self.remote_ops
                .set_stats_status("remote stats are unavailable for network device sessions");
            self.shell
                .set_status(self.remote_ops.stats_status().to_string());
            cx.notify();
            return;
        }
        let Some(job_session_id) = self.session.active_id_owned() else {
            self.remote_ops
                .set_stats_status("start an SSH session before inspecting stats");
            cx.notify();
            return;
        };
        if self.remote_ops.stats_is_pending_for(&job_session_id) {
            self.remote_ops
                .set_stats_status("stats refresh already running");
            cx.notify();
            return;
        }

        let ticket = self.remote_ops.begin_stats_job(job_session_id.clone());
        self.remote_ops.mark_stats_refresh_started();
        self.remote_ops
            .set_stats_status("loading remote system stats");
        std::thread::spawn(move || {
            let result = RemoteStatsService::new(config)
                .snapshot()
                .map_err(|error| error.to_string());
            let _ = ticket.tx.unbounded_send(StatsJobResult {
                job_id: ticket.job_id,
                session_id: job_session_id,
                result,
            });
        });
        cx.notify();
    }

    pub(in crate::features) fn toggle_stats_cpu_expanded(&mut self, cx: &mut Context<Self>) {
        self.remote_ops.toggle_stats_cpu_expanded();
        cx.notify();
    }

    /// Deliver stats job replies as they arrive.
    ///
    /// Started once at window open. Before this the runtime tick polled
    /// `next_stats_event`, which meant a reply waited for the next tick and
    /// forced `runtime_quiet_tick_allowed` to carry a `remote_ops` term to keep
    /// that wait short.
    pub(in crate::features) fn start_stats_event_drain(&mut self, cx: &mut Context<Self>) {
        let Some(mut rx) = self.remote_ops.take_stats_event_receiver() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            while let Some(event) = rx.next().await {
                if this
                    .update(cx, |this, cx| {
                        if this.apply_stats_event(event, cx) {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    /// Apply one reply, reporting whether the UI needs a repaint.
    fn apply_stats_event(&mut self, event: StatsJobResult, cx: &mut Context<Self>) -> bool {
        if !self
            .remote_ops
            .complete_stats_event(event.job_id, &event.session_id)
        {
            // A superseded job's reply; the pane has already moved on.
            return false;
        }
        if self.session.active_id() != Some(event.session_id.as_str()) {
            // Another session is active now, but completing the job is
            // itself a state change worth painting.
            return true;
        }
        match event.result {
            Ok(stats) => {
                self.remote_ops.reset_stats_refresh_failures();
                self.remote_ops.set_stats_status(format!(
                    "loaded stats for {} · load {:.2}/{:.2}/{:.2}",
                    if stats.system.hostname.trim().is_empty() {
                        "remote host"
                    } else {
                        stats.system.hostname.as_str()
                    },
                    stats.load.load1,
                    stats.load.load5,
                    stats.load.load15
                ));
                self.shell
                    .set_status(self.remote_ops.stats_status().to_string());
                // The snapshot is the only place the remote OS is reported,
                // so this is where a connection's icon can be filled in.
                self.apply_auto_detected_connection_icon(&event.session_id, &stats.system, cx);
                self.remote_ops.apply_stats(stats);
            }
            Err(error) => {
                self.remote_ops.record_stats_refresh_failure();
                self.remote_ops
                    .set_stats_status(format!("stats refresh failed: {error}"));
                self.shell
                    .set_status(self.remote_ops.stats_status().to_string());
            }
        }
        true
    }
}
