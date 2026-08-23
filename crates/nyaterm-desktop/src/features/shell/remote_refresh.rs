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
    /// Idempotent. Armed from `render`, because every input the predicate reads --
    /// which panel is showing, which metric the header wants, and whether a session
    /// with an SSH config is active -- changes only alongside a repaint. Arming it
    /// from a one-shot at window open instead means arming it before any session
    /// exists, which is a predicate that is always false.
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
                // `update`, not `update_in`: nothing on this path needs the window.
                // With `update_in` a missing window broke the loop *without* clearing
                // the armed flag, which left the flag stuck true and the clock unable
                // to ever re-arm.
                let Ok(keep_running) = this.update(cx, |this, cx| {
                    if this.drive_remote_auto_refresh(cx) {
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

    /// Whether the transfer browser currently wants its cwd synced.
    ///
    /// This was "does any remote panel want refreshing", and it no longer asks about
    /// the five panels: each owns its own clock, armed by
    /// `sync_remote_panel_demand`. What is left needs no session term of its own --
    /// `drive_remote_auto_refresh` checks that -- but keeping it here means a closed
    /// transfer browser retires the clock rather than waking every second to find
    /// nothing.
    pub(in crate::features) fn remote_panels_need_refresh(&self) -> bool {
        self.current_left_panel() == Some(NavItem::Transfers)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui::{
        AppContext as _, IntoElement, ParentElement as _, Render, Styled as _, TestAppContext,
        VisualTestContext, div,
    };
    use nyaterm_core::{AppRuntime, RuntimeMode, uuid};

    use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
    use crate::features::NyaTermApp;
    use crate::models::NavItem;

    fn app(cx: &mut TestAppContext) -> gpui::Entity<NyaTermApp> {
        // A uuid rather than a clock reading: these tests run in parallel and
        // Windows' ~15ms clock granularity lets a nanosecond timestamp repeat,
        // which would share one config dir and so one settings database.
        let root = std::env::temp_dir().join(format!(
            "nyaterm-remote-refresh-{}-{}",
            std::process::id(),
            uuid()
        ));
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
        cx.new(|cx| NyaTermApp::new(runtime, stores, cx))
    }

    /// Host view, so the app is drawn as a child the way the real window draws it.
    struct RepaintHost {
        app: gpui::Entity<NyaTermApp>,
    }

    impl Render for RepaintHost {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            div().size_full().child(self.app.clone())
        }
    }

    /// Give the app a panel that wants refreshing, without a live SSH session.
    ///
    /// The transfer cwd sync is the one term of `remote_panels_need_refresh` that does
    /// not require `active_ssh_config`, which is what makes the predicate reachable
    /// from a unit test at all.
    fn open_transfers_panel(app: &gpui::Entity<NyaTermApp>, cx: &mut TestAppContext) {
        cx.update_entity(app, |app, _| {
            app.shell.panels.multi_open = false;
            app.shell.panels.left_collapsed = false;
            app.shell.panels.active_left = Some(NavItem::Transfers);
        });
    }

    fn close_panels(app: &gpui::Entity<NyaTermApp>, cx: &mut TestAppContext) {
        cx.update_entity(app, |app, _| {
            app.shell.panels.active_left = None;
            app.shell.panels.left_collapsed = true;
        });
    }

    /// The state the app spends nearly all of its life in must cost no wakes.
    #[test]
    fn an_idle_app_arms_no_refresh_clock() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        close_panels(&app, &mut cx);
        cx.update_entity(&app, |app, cx| {
            assert!(
                !app.remote_panels_need_refresh(),
                "no session and no panel means nothing to refresh"
            );
            app.ensure_remote_refresh_clock(cx);
            assert!(!app.shell.remote_refresh_clock_is_armed());
        });
    }

    /// A connect that is still settling must defer a refresh, not cancel it.
    ///
    /// `enter_connect_settle` is what a session start calls, so this drives the same
    /// state the real path produces. The deferral is deliberately not visible in
    /// `remote_panels_need_refresh`: the panel still wants its data, so the clock
    /// stays armed and asks again on the next poll rather than retiring.
    #[test]
    fn a_settling_connect_defers_without_retiring_the_clock() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        open_transfers_panel(&app, &mut cx);
        cx.update_entity(&app, |app, _| {
            assert!(
                !app.remote_refresh_is_deferred(),
                "a calm app defers nothing"
            );
            app.enter_connect_settle();
            assert!(app.remote_refresh_is_deferred());
            assert!(
                app.remote_panels_need_refresh(),
                "deferral must not look like the panel no longer wanting data, or the \
                 clock would retire instead of asking again"
            );
        });
    }

    /// A repaint must arm the clock, because a repaint is the only signal that sees
    /// every input the predicate reads.
    ///
    /// This is the test that matters, and the one whose absence let the bug through:
    /// calling `ensure_remote_refresh_clock` directly passes whether or not anything
    /// in the app ever calls it. Until this commit the only call site was
    /// `start_after_window_open`, which runs once -- before any session exists and
    /// before any panel is restored -- so the predicate was false, the clock never
    /// armed, and none of the five remote panels auto-refreshed at all.
    #[test]
    fn a_repaint_arms_the_clock_for_an_open_panel() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        cx.update_entity(&app, |app, cx| app.sync_component_theme(cx));
        open_transfers_panel(&app, &mut cx);
        cx.update_entity(&app, |app, _| {
            assert!(
                app.remote_panels_need_refresh(),
                "an open transfer browser wants its cwd synced"
            );
            assert!(
                !app.shell.remote_refresh_clock_is_armed(),
                "nothing has painted yet"
            );
        });

        let host_app = app.clone();
        let (_, cx) = cx.add_window_view(move |_, _| RepaintHost { app: host_app });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            app.update(cx, |_, cx| cx.notify());
            _ = window.draw(cx);
        });

        cx.update(|_, cx| {
            app.update(cx, |app, _| {
                assert!(
                    app.shell.remote_refresh_clock_is_armed(),
                    "a paint with a panel open must arm the refresh clock"
                );
            });
        });
    }

    /// The clock must retire itself rather than wake every second forever once the
    /// panel that wanted it is closed.
    ///
    /// Needs a window: the loop body goes through `update_in`, and without one it
    /// fails and breaks out *without* clearing the armed flag -- which would leave the
    /// flag stuck true and the clock unable to ever re-arm. `C4` removes that hazard
    /// by dropping `update_in`, since no submitter on this path uses the window.
    #[test]
    fn the_clock_retires_when_nothing_wants_refreshing() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        cx.update_entity(&app, |app, cx| app.sync_component_theme(cx));
        open_transfers_panel(&app, &mut cx);

        let host_app = app.clone();
        let (_, cx) = cx.add_window_view(move |_, _| RepaintHost { app: host_app });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            app.update(cx, |_, cx| cx.notify());
            _ = window.draw(cx);
        });
        cx.update(|_, cx| {
            app.update(cx, |app, _| {
                assert!(app.shell.remote_refresh_clock_is_armed());
                app.shell.panels.active_left = None;
                app.shell.panels.left_collapsed = true;
            });
        });

        // One poll interval later the clock notices and stops.
        cx.executor().advance_clock(Duration::from_secs(2));
        cx.run_until_parked();
        cx.update(|_, cx| {
            app.update(cx, |app, _| {
                assert!(
                    !app.shell.remote_refresh_clock_is_armed(),
                    "a closed panel must retire the clock, not keep it waking"
                );
            });
        });
    }
}
