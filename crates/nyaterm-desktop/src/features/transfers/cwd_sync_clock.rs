//! The transfer browser's remote cwd sync, on its own clock.
//!
//! When "Auto CWD" is on for a connection, the browser follows the terminal's working
//! directory by listing the remote cwd on an interval. There is no push from the host,
//! so this is genuinely periodic and stays a poll.
//!
//! It used to ride the shell-wide clock in `features/shell/remote_refresh.rs` alongside
//! the five remote monitor panels. Those took their own clocks in the previous commit,
//! leaving this the only member; it gets a clock of its own here so that file can go.
//!
//! Extracting the Transfers *panel* into an entity is a separate job. This is only the
//! schedule, which is why the armed flag lives on `TransferFeatureState` rather than
//! becoming a `Task` on an entity that does not exist yet.

use std::time::{Duration, Instant};

use gpui::Context;

use crate::features::NyaTermApp;
use crate::features::remote::remote_refresh_due;
use crate::models::NavItem;

/// How often to check whether the cwd sync has come due.
///
/// Matches the interval it services, which is a constant rather than a setting, so
/// there is nothing finer to sample for.
const TRANSFER_CWD_SYNC_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// How stale the browser's cwd may get before it is re-listed.
///
/// Was `TRANSFER_AUTO_SYNC_CWD_INTERVAL_SECONDS` in the event pump's helpers, moved
/// here with the only thing that read it.
const TRANSFER_AUTO_SYNC_CWD_INTERVAL_SECONDS: u32 = 3;

impl NyaTermApp {
    /// Keep the transfer browser's cwd in step while it is open.
    ///
    /// Idempotent. Armed from `render`, because what it depends on -- the browser being
    /// open, and Auto CWD being on for the active connection -- changes alongside a
    /// repaint and has no single event that covers both.
    /// Arm the cwd-sync clock if the transfer browser now wants one.
    ///
    /// Called from the panel-stack transitions that can reveal or hide the browser,
    /// not from a render. Arming from `NyaTermApp::render` meant every unrelated
    /// repaint in the app re-armed it, and it meant the demand check ran on the paint
    /// path; the browser becoming visible is an event, so it is treated as one. The
    /// clock retires itself when the demand goes away, so nothing has to disarm it.
    ///
    /// Idempotent, which matters because the two panel entry points call into each
    /// other depending on the multi-open mode.
    pub(in crate::features) fn ensure_transfer_cwd_sync_clock(&mut self, cx: &mut Context<Self>) {
        if self.transfer.cwd_sync_clock_is_armed() || !self.transfer_cwd_sync_needs_polling() {
            return;
        }
        self.transfer.set_cwd_sync_clock_armed(true);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(TRANSFER_CWD_SYNC_POLL_INTERVAL)
                    .await;
                let Ok(keep_running) = this.update(cx, |this, cx| {
                    if this.sync_transfer_cwd_if_due(cx) {
                        cx.notify();
                    }
                    let running = this.transfer_cwd_sync_needs_polling();
                    if !running {
                        this.transfer.set_cwd_sync_clock_armed(false);
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

    /// Whether the transfer browser is open at all.
    ///
    /// Only the panel, not Auto CWD or the session: both of those change without a
    /// repaint that would re-arm the clock, so treating them as reasons to retire it
    /// would mean enabling Auto CWD had no effect until something else painted.
    /// `sync_transfer_cwd_if_due` re-checks them every beat instead.
    pub(in crate::features) fn transfer_cwd_sync_needs_polling(&self) -> bool {
        self.current_left_panel() == Some(NavItem::Transfers)
    }

    /// List the remote cwd if it has gone stale.
    ///
    /// The same conditions and the same deferral gate the shell-wide clock applied.
    fn sync_transfer_cwd_if_due(&mut self, cx: &mut Context<Self>) -> bool {
        if self.session.active_ssh_config().is_none() || self.remote_refresh_is_deferred() {
            return false;
        }
        if !self.transfer_browser_auto_sync_cwd_enabled()
            || self.transfer_sync_cwd_job_running()
            || !remote_refresh_due(
                self.transfer.browser_auto_sync_cwd_last_at(),
                TRANSFER_AUTO_SYNC_CWD_INTERVAL_SECONDS,
            )
        {
            return false;
        }
        self.transfer.mark_browser_auto_sync_cwd(Instant::now());
        self.start_transfer_sync_cwd_job(cx);
        true
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui::{AppContext as _, TestAppContext};
    use nyaterm_core::{AppRuntime, RuntimeMode, uuid};

    use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
    use crate::features::NyaTermApp;
    use crate::models::NavItem;

    fn app(cx: &mut TestAppContext) -> gpui::Entity<NyaTermApp> {
        // A uuid rather than a clock reading: these tests run in parallel and
        // Windows' ~15ms clock granularity lets a nanosecond timestamp repeat,
        // which would share one config dir and so one settings database.
        let root = std::env::temp_dir().join(format!(
            "nyaterm-transfer-cwd-sync-{}-{}",
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

    fn open_transfer_browser(app: &gpui::Entity<NyaTermApp>, cx: &mut TestAppContext) {
        cx.update_entity(app, |app, cx| {
            app.open_or_toggle_panel(NavItem::Transfers, cx);
        });
    }

    /// A closed transfer browser costs no wakes, which is the state the app is in
    /// nearly always.
    #[test]
    fn a_closed_browser_arms_no_clock() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        cx.update_entity(&app, |app, cx| {
            assert!(!app.transfer_cwd_sync_needs_polling());
            app.ensure_transfer_cwd_sync_clock(cx);
            assert!(!app.transfer.cwd_sync_clock_is_armed());
        });
    }

    /// An open browser arms the clock even with Auto CWD off.
    ///
    /// Deliberate: Auto CWD is a per-connection setting that changes without a repaint,
    /// so gating the *clock* on it would mean switching it on had no effect until
    /// something else painted. The per-beat check is what makes it cheap instead.
    #[test]
    fn an_open_browser_arms_the_clock_regardless_of_auto_cwd() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        open_transfer_browser(&app, &mut cx);
        cx.update_entity(&app, |app, cx| {
            assert!(
                !app.transfer_browser_auto_sync_cwd_enabled(),
                "no connection is selected, so Auto CWD cannot be on"
            );
            app.ensure_transfer_cwd_sync_clock(cx);
            assert!(app.transfer.cwd_sync_clock_is_armed());
        });
    }

    /// Closing the browser retires the clock rather than leaving it waking every
    /// second forever.
    #[test]
    fn closing_the_browser_retires_the_clock() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        open_transfer_browser(&app, &mut cx);
        cx.update_entity(&app, |app, cx| {
            app.ensure_transfer_cwd_sync_clock(cx);
            assert!(app.transfer.cwd_sync_clock_is_armed());
            // Toggling the same panel again closes it, which is what the activity bar
            // does.
            app.open_or_toggle_panel(NavItem::Transfers, cx);
            assert!(!app.transfer_cwd_sync_needs_polling());
        });

        cx.executor().advance_clock(Duration::from_secs(2));
        cx.run_until_parked();
        cx.update_entity(&app, |app, _| {
            assert!(
                !app.transfer.cwd_sync_clock_is_armed(),
                "a closed browser must retire the clock, not keep it waking"
            );
        });
    }

    struct AppHost {
        app: gpui::Entity<NyaTermApp>,
    }

    impl gpui::Render for AppHost {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            use gpui::{ParentElement as _, Styled as _};
            gpui::div().size_full().child(self.app.clone())
        }
    }

    /// A paint must arm the clock, with nothing calling `ensure` by hand.
    ///
    /// The three tests above all keep passing with the call removed from `render` --
    /// checked, not assumed -- because they drive `ensure_transfer_cwd_sync_clock`
    /// themselves. That is the gap that let `3904c69b`'s dead clock through: they prove
    /// the mechanism and say nothing about the wiring.
    /// Opening the browser arms the clock at that moment, with no window in play.
    ///
    /// Arming used to happen in `NyaTermApp::render`, which made a paint the trigger.
    /// The browser becoming visible is an event, so it is treated as one.
    #[test]
    fn opening_the_browser_arms_the_clock_without_a_paint() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        cx.update_entity(&app, |app, _| {
            assert!(
                !app.transfer.cwd_sync_clock_is_armed(),
                "nothing has opened the browser yet"
            );
        });

        open_transfer_browser(&app, &mut cx);

        cx.update_entity(&app, |app, _| {
            assert!(
                app.transfer.cwd_sync_clock_is_armed(),
                "opening the transfer browser must arm the cwd sync clock, with no paint"
            );
        });
    }

    /// The inverse, and the reason the arming moved: repaints of an app that is not
    /// showing the browser must not start polling a remote for its cwd.
    #[test]
    fn unrelated_root_paints_cannot_arm_the_clock() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        cx.update_entity(&app, |app, cx| app.sync_component_theme(cx));

        let host_app = app.clone();
        let (_, cx) = cx.add_window_view(move |_, _| AppHost { app: host_app });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();

        for _ in 0..5 {
            cx.update(|window, cx| {
                app.update(cx, |_, cx| cx.notify());
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }

        cx.update(|_, cx| {
            assert!(
                !app.read(cx).transfer.cwd_sync_clock_is_armed(),
                "five repaints with the browser closed must not arm the cwd sync clock"
            );
        });
    }
}
