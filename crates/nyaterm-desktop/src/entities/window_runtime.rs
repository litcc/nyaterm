use std::time::{Duration, Instant};

use gpui::{Context, Entity, Window};

use crate::features::NyaTermApp;

#[derive(Debug, Default)]
pub struct WindowRuntimeStore {
    pump_started: bool,
}

impl WindowRuntimeStore {
    pub fn ensure_started(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        app: Entity<NyaTermApp>,
    ) -> bool {
        if !self.mark_started() {
            return false;
        }
        app.update(cx, |app, _| app.mark_window_runtime_started());
        window
            .spawn(cx, async move |cx| {
                let mut skipped_updates = 0u64;
                let mut last_skip_log_at = Instant::now();
                loop {
                    let delay = app.read_with(cx, |app, _| app.window_runtime_tick_delay());
                    cx.background_executor().timer(delay).await;
                    let keep_running = cx
                        .update(|window, cx| {
                            let now = Instant::now();
                            let should_update = app
                                .read_with(cx, |app, _| app.window_runtime_tick_needs_update(now));
                            if !should_update {
                                skipped_updates = skipped_updates.saturating_add(1);
                                if skipped_updates > 0
                                    && now.saturating_duration_since(last_skip_log_at)
                                        >= Duration::from_secs(5)
                                {
                                    tracing::info!(
                                        diagnostic = "runtime_tick_skip",
                                        skipped_updates,
                                        next_tick_delay_ms = delay.as_millis(),
                                        "skipped idle window runtime updates"
                                    );
                                    skipped_updates = 0;
                                    last_skip_log_at = now;
                                }
                                return app.read_with(cx, |app, _| app.window_runtime_running());
                            }
                            app.update(cx, |app, cx| app.drive_window_runtime_tick(window, cx))
                        })
                        .unwrap_or(false);
                    if !keep_running {
                        break;
                    }
                }
            })
            .detach();
        true
    }

    pub fn mark_started(&mut self) -> bool {
        if self.pump_started {
            return false;
        }
        self.pump_started = true;
        true
    }

    pub fn pump_started(&self) -> bool {
        self.pump_started
    }
}
