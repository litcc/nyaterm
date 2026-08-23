//! The five remote panels that poll a host, as GPUI entities.
//!
//! Stats, GPU, NPU, Processes and Docker each show data fetched from the active SSH
//! session on an interval. Until Phase 4 they were built inline by `panel_body`, and
//! the schedule that fetched their data lived in one shell-wide clock
//! (`features/shell/remote_refresh.rs`).
//!
//! **What these entities own is the schedule, not the data.**
//! `NyaTermApp.remote_ops` stays the single authoritative owner of every pane's data,
//! in-flight job, failure streak and `last_refresh_at`, and the decision of whether a
//! refresh is due stays with it -- a panel asks "refresh if due" and the app answers.
//! The panel owns only the timer that does the asking, which is the thing that was in
//! the wrong place.
//!
//! One type with a `kind` discriminant rather than five near-identical types: five
//! instances are five entities with five entity ids, so they are as isolated as five
//! types would be, and the two matches below are smaller than the duplication.
//!
//! These are held by `NyaTermApp` for its whole life, so a hidden panel is **not**
//! dropped -- "not rendered" is not observable as a drop. That is deliberate rather
//! than incidental: the header status bar can want stats, GPU or NPU while the
//! matching panel is closed, so the entity has to outlive its own visibility to be
//! able to own that demand.
//!
//! Nothing here observes the app. A view embedded with `into_any_element` takes
//! GPUI's uncached path, whose `request_layout` calls `render` every frame, so a
//! child view of the app is repainted by the app's own paint and a
//! `cx.observe(&app, ..)` would buy nothing but coupling.

use std::time::Duration;

use gpui::{
    AnyElement, Context, Entity, IntoElement, Render, Task, WeakEntity, Window, div, prelude::*,
};

use crate::features::NyaTermApp;
use crate::models::NavItem;

/// How often a polling panel asks whether its own interval has come due.
///
/// The per-panel intervals are user settings in whole seconds floored at one, so a
/// one-second clock is exactly as fine as the finest thing it can service. This is
/// the cadence the one shell-wide clock used, kept unchanged; what moved is who owns
/// it. Sleeping straight to the next due moment instead would mean a settings change
/// mid-sleep landing up to a whole interval late.
const REMOTE_PANEL_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Which of the five polling panels an entity is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::features) enum RemoteMonitorKind {
    Stats,
    Gpu,
    Npu,
    Processes,
    Docker,
}

impl RemoteMonitorKind {
    pub(in crate::features) const ALL: [RemoteMonitorKind; 5] = [
        RemoteMonitorKind::Stats,
        RemoteMonitorKind::Gpu,
        RemoteMonitorKind::Npu,
        RemoteMonitorKind::Processes,
        RemoteMonitorKind::Docker,
    ];

    /// The nav item whose panel shows this metric.
    pub(in crate::features) fn nav_item(self) -> NavItem {
        match self {
            RemoteMonitorKind::Stats => NavItem::Stats,
            RemoteMonitorKind::Gpu => NavItem::GpuMonitor,
            RemoteMonitorKind::Npu => NavItem::AscendNpuMonitor,
            RemoteMonitorKind::Processes => NavItem::Processes,
            RemoteMonitorKind::Docker => NavItem::Docker,
        }
    }
}

/// One remote polling panel.
pub(in crate::features) struct RemoteMonitorPanel {
    kind: RemoteMonitorKind,
    /// Weak on purpose. `NyaTermApp` owns this entity, so a strong handle back would
    /// be a reference cycle that keeps both alive forever -- which is what
    /// `ConnectionPanel` does today. The app always outlives the panels it owns, so
    /// the upgrade below only fails during teardown.
    app: WeakEntity<NyaTermApp>,
    /// The refresh schedule, and the only record of whether this panel is wanted.
    ///
    /// A `Task` cancels when dropped, so `None` is not merely a flag saying the panel
    /// should not poll -- it is the poll being gone. That makes "an inactive panel
    /// does not poll" a property of the type rather than of a predicate somebody has
    /// to remember to check, and it is why there is no separate `demand: bool`: the
    /// task's existence *is* the demand, so the two cannot disagree.
    clock: Option<Task<()>>,
    /// Paints of *this entity*, so a test can tell the entity route apart from the
    /// inline views it replaced. Both register the same search-input ids, so no
    /// externally visible side effect distinguishes them.
    #[cfg(test)]
    paint_count: usize,
}

impl RemoteMonitorPanel {
    fn new(kind: RemoteMonitorKind, app: WeakEntity<NyaTermApp>) -> Self {
        Self {
            kind,
            app,
            clock: None,
            #[cfg(test)]
            paint_count: 0,
        }
    }

    /// Start or stop this panel's refresh schedule.
    ///
    /// Idempotent, and cheap in the steady state: one `Option::is_some` compare. The
    /// caller reconciles every paint, so this is called far more often than it acts.
    ///
    /// No `cx.notify()` on either edge -- nothing this panel renders depends on
    /// whether it is polling, and notifying from the app's own paint would ask for a
    /// frame that has nothing new in it.
    fn set_demand(&mut self, demand: bool, cx: &mut Context<Self>) {
        if demand == self.clock.is_some() {
            return;
        }
        if !demand {
            // Dropping the task cancels it.
            self.clock = None;
            return;
        }
        let kind = self.kind;
        let app = self.app.clone();
        self.clock = Some(cx.spawn(async move |_panel, cx| {
            loop {
                cx.background_executor()
                    .timer(REMOTE_PANEL_POLL_INTERVAL)
                    .await;
                let Some(app) = app.upgrade() else {
                    break;
                };
                // The app owns the decision: whether the interval is due, whether a
                // job is already in flight, and whether the app is calm enough to
                // submit one. This clock only decides *when to ask*.
                //
                // No error to handle: the strong handle from `upgrade` keeps the app
                // alive for the call, and a released app ends the loop above.
                app.update(cx, |app, cx| app.refresh_remote_monitor_if_due(kind, cx));
            }
        }));
    }

    /// Widened past this module so the session-activation boundary can assert that a
    /// switch reconciles demand without a paint.
    #[cfg(test)]
    pub(in crate::features) fn is_polling(&self) -> bool {
        self.clock.is_some()
    }

    #[cfg(test)]
    fn paint_count(&self) -> usize {
        self.paint_count
    }
}

impl Render for RemoteMonitorPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.paint_count += 1;
        }
        let Some(app) = self.app.upgrade() else {
            return div().into_any_element();
        };
        let kind = self.kind;
        app.update(cx, |app, cx| -> AnyElement {
            match kind {
                RemoteMonitorKind::Stats => app.stats_view(cx).into_any_element(),
                RemoteMonitorKind::Gpu => app.gpu_view(cx).into_any_element(),
                RemoteMonitorKind::Npu => app.npu_view(cx).into_any_element(),
                RemoteMonitorKind::Processes => app.processes_view(cx).into_any_element(),
                RemoteMonitorKind::Docker => app.docker_view(cx).into_any_element(),
            }
        })
    }
}

/// The five panel entities, as one field on `NyaTermApp`.
pub(in crate::features) struct RemotePanels {
    stats: Entity<RemoteMonitorPanel>,
    gpu: Entity<RemoteMonitorPanel>,
    npu: Entity<RemoteMonitorPanel>,
    processes: Entity<RemoteMonitorPanel>,
    docker: Entity<RemoteMonitorPanel>,
}

impl RemotePanels {
    pub(in crate::features) fn new(
        app: WeakEntity<NyaTermApp>,
        cx: &mut Context<NyaTermApp>,
    ) -> Self {
        let panel = |kind: RemoteMonitorKind, cx: &mut Context<NyaTermApp>| {
            let app = app.clone();
            cx.new(|_| RemoteMonitorPanel::new(kind, app))
        };
        Self {
            stats: panel(RemoteMonitorKind::Stats, cx),
            gpu: panel(RemoteMonitorKind::Gpu, cx),
            npu: panel(RemoteMonitorKind::Npu, cx),
            processes: panel(RemoteMonitorKind::Processes, cx),
            docker: panel(RemoteMonitorKind::Docker, cx),
        }
    }

    pub(in crate::features) fn entity(
        &self,
        kind: RemoteMonitorKind,
    ) -> &Entity<RemoteMonitorPanel> {
        match kind {
            RemoteMonitorKind::Stats => &self.stats,
            RemoteMonitorKind::Gpu => &self.gpu,
            RemoteMonitorKind::Npu => &self.npu,
            RemoteMonitorKind::Processes => &self.processes,
            RemoteMonitorKind::Docker => &self.docker,
        }
    }
}

impl NyaTermApp {
    /// Refresh one pane if its interval is due and the app is calm enough.
    ///
    /// The two gates are the ones the shell-wide clock applied to all five at once, so
    /// a panel-owned clock still honours them: no session means nothing to poll, and a
    /// deferral means come back next beat rather than submit now.
    pub(in crate::features) fn refresh_remote_monitor_if_due(
        &mut self,
        kind: RemoteMonitorKind,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.session.active_ssh_config().is_none() {
            return false;
        }
        if self.remote_refresh_is_deferred() {
            return false;
        }
        match kind {
            RemoteMonitorKind::Stats => self.refresh_stats_if_due(cx),
            RemoteMonitorKind::Gpu => self.refresh_gpu_if_due(cx),
            RemoteMonitorKind::Npu => self.refresh_npu_if_due(cx),
            RemoteMonitorKind::Processes => self.refresh_processes_if_due(cx),
            RemoteMonitorKind::Docker => self.refresh_docker_if_due(cx),
        }
    }

    /// Whether anything currently wants this metric kept fresh.
    ///
    /// Two sources of demand, not one. A panel being on screen is the obvious one; the
    /// header status bar is the other, and it can be showing stats, GPU or NPU with
    /// that panel closed. Treating only the panel as demand would stop the header
    /// updating, which is the case the "armed on mount, dropped on unmount" sketch of
    /// this design missed -- and the reason the panel entities are owned for the app's
    /// whole life rather than created when shown.
    ///
    /// The settings flag is checked here as well as by `panel_body`, because a panel
    /// switched off in settings renders a placeholder rather than nothing, and a
    /// placeholder must not poll.
    pub(in crate::features) fn remote_monitor_demand(&self, kind: RemoteMonitorKind) -> bool {
        let summary = self.settings.summary();
        let shown = self.panel_is_rendered(kind.nav_item());
        match kind {
            RemoteMonitorKind::Stats => {
                (shown || self.header_status_needs_remote_stats()) && summary.ui_show_remote_stats
            }
            RemoteMonitorKind::Gpu => {
                (shown || self.header_status_needs_gpu()) && summary.ui_show_gpu_monitor
            }
            RemoteMonitorKind::Npu => {
                (shown || self.header_status_needs_npu()) && summary.ui_show_ascend_npu_monitor
            }
            RemoteMonitorKind::Processes => shown && summary.ui_show_process_manager,
            RemoteMonitorKind::Docker => shown && summary.ui_show_docker_manager,
        }
    }

    /// Start the clocks for panels that are wanted and stop the rest.
    ///
    /// Called from `render`, for the same reason `ensure_idle_lock_clock` is: every
    /// input this reads -- which panels are on screen, what the header is showing,
    /// which panels settings enable, whether a session with an SSH config is active --
    /// changes alongside a repaint, and there is no single event that covers all four.
    /// Enumerating the mutation sites instead would mean a new one silently leaving a
    /// panel unrefreshed -- which is the bug `3904c69b` fixed, where the one call site
    /// ran before any session existed and so never armed anything.
    ///
    /// Cheap enough to run per paint: one settings read, one rendered-panel test per
    /// kind, and five `Option::is_some` compares that do nothing unless demand
    /// actually flipped.
    pub(in crate::features) fn sync_remote_panel_demand(&mut self, cx: &mut Context<Self>) {
        let session_active = self.session.active_ssh_config().is_some();
        for kind in RemoteMonitorKind::ALL {
            let demand = session_active && self.remote_monitor_demand(kind);
            let panel = self.remote_panels.entity(kind).clone();
            panel.update(cx, |panel, cx| panel.set_demand(demand, cx));
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui::{
        AppContext as _, Entity, IntoElement, ParentElement as _, Render, Styled as _,
        TestAppContext, VisualTestContext, div,
    };
    use nyaterm_core::{AppRuntime, RuntimeMode, uuid};

    use super::{RemoteMonitorKind, RemoteMonitorPanel};
    use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
    use crate::features::NyaTermApp;

    const ALL_KINDS: [RemoteMonitorKind; 5] = [
        RemoteMonitorKind::Stats,
        RemoteMonitorKind::Gpu,
        RemoteMonitorKind::Npu,
        RemoteMonitorKind::Processes,
        RemoteMonitorKind::Docker,
    ];

    fn app(cx: &mut TestAppContext) -> Entity<NyaTermApp> {
        // A uuid rather than a clock reading: these tests run in parallel and
        // Windows' ~15ms clock granularity lets a nanosecond timestamp repeat,
        // which would share one config dir and so one settings database.
        let root = std::env::temp_dir().join(format!(
            "nyaterm-remote-panels-{}-{}",
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

    struct PanelHost {
        panels: Vec<Entity<RemoteMonitorPanel>>,
    }

    impl Render for PanelHost {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            div().size_full().children(
                self.panels
                    .iter()
                    .cloned()
                    .map(IntoElement::into_any_element),
            )
        }
    }

    /// Five instances means five entity ids, which is what makes them isolated views
    /// rather than one view with a mode flag.
    #[test]
    fn every_kind_gets_its_own_entity() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        cx.update_entity(&app, |app, _| {
            let mut ids = ALL_KINDS
                .iter()
                .map(|kind| app.remote_panels.entity(*kind).entity_id())
                .collect::<Vec<_>>();
            ids.sort_unstable();
            let distinct = ids.len();
            ids.dedup();
            assert_eq!(ids.len(), distinct, "each panel must be its own entity");
        });
    }

    /// Each panel must paint through its entity.
    ///
    /// The views moved from being built inline during the app's own render to being
    /// child views, which puts them behind GPUI's `with_rendered_view` boundary and
    /// changes the element-id path they live on. This draws all five to confirm that
    /// relocation did not break the ones that register text inputs or read theme
    /// caches the root chrome used to prime first.
    #[test]
    fn all_five_panels_paint_as_child_views() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        cx.update_entity(&app, |app, cx| app.sync_component_theme(cx));
        let panels = cx.update_entity(&app, |app, _| {
            ALL_KINDS
                .iter()
                .map(|kind| app.remote_panels.entity(*kind).clone())
                .collect::<Vec<_>>()
        });

        let (_, cx) = cx.add_window_view(move |_, _| PanelHost { panels });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }

    struct AppHost {
        app: Entity<NyaTermApp>,
    }

    impl Render for AppHost {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            div().size_full().child(self.app.clone())
        }
    }

    /// The shell's panel dispatch must reach the entity.
    ///
    /// The test above renders the panels directly, which would keep passing if
    /// `panel_body` had been left pointing at the old inline views -- so it proves the
    /// entity works, not that anything uses it. This drives the real route:
    /// `render` -> `side_panel_stack` -> `right_panel_body` -> `panel_body`.
    ///
    /// The assertion is the entity's own paint count, not a side effect of the view it
    /// renders: the inline views this replaced register the same search-input ids, so
    /// an id-existence check would pass whether or not `panel_body` was ever switched
    /// over. Counting paints of the entity is the only thing that tells the two apart.
    /// The second half of each pair is what stops it passing vacuously -- it shows the
    /// right side really is rendering, and rendering the *other* panel.
    #[test]
    fn the_shell_panel_dispatch_renders_the_entity() {
        assert_eq!(
            painted_remote_panels(crate::models::NavItem::GpuMonitor),
            (true, false),
            "opening the GPU panel must paint the GPU entity and not the process one"
        );
        assert_eq!(
            painted_remote_panels(crate::models::NavItem::Processes),
            (false, true),
            "and opening the process panel must paint the other one"
        );
    }

    /// Open `panel`, paint the whole app, and report which of the GPU and process
    /// panel entities painted.
    fn painted_remote_panels(panel: crate::models::NavItem) -> (bool, bool) {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        cx.update_entity(&app, |app, cx| {
            app.sync_component_theme(cx);
            let mut summary = app.settings.summary().clone();
            summary.ui_show_gpu_monitor = true;
            summary.ui_show_process_manager = true;
            app.settings.replace_summary(summary);
            // The real activation path, so this cannot pass by poking a combination of
            // panel fields the UI never produces.
            app.open_or_toggle_panel(panel, cx);
        });

        let host_app = app.clone();
        let (_, cx) = cx.add_window_view(move |_, _| AppHost { app: host_app });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        // Twice: the first paint is what teaches `shell.viewport` the window size, and
        // the right side is only laid out inline once it is wide enough.
        for _ in 0..2 {
            cx.update(|window, cx| {
                app.update(cx, |_, cx| cx.notify());
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }

        cx.update(|_, cx| {
            let painted = |kind: RemoteMonitorKind| {
                app.read(cx)
                    .remote_panels
                    .entity(kind)
                    .read(cx)
                    .paint_count()
                    > 0
            };
            (
                painted(RemoteMonitorKind::Gpu),
                painted(RemoteMonitorKind::Processes),
            )
        })
    }

    /// Give the app an active session carrying an SSH config.
    ///
    /// `active_ssh_config` reads the active session's metadata, so this is the whole
    /// requirement. The host is empty because none of these tests advance the clock,
    /// so no refresh is ever submitted and nothing tries to connect.
    fn activate_ssh_session(app: &Entity<NyaTermApp>, cx: &mut TestAppContext) {
        cx.update_entity(app, |app, _| {
            app.session.register_session_metadata(
                "remote-panel-test",
                crate::models::SessionRuntimeMetadata {
                    ssh_config: Some(nyaterm_transport::SshSessionConfig::default()),
                    ssh_multiplex_key: None,
                    source_connection_id: None,
                    ai_execution_profile: nyaterm_core::AiExecutionProfile::Posix,
                    launch_config: crate::models::SessionLaunchConfig::Local(
                        nyaterm_transport::LocalSessionConfig::default(),
                    ),
                    disconnected: false,
                },
            );
            app.session
                .select_active_session("remote-panel-test".to_string());
        });
    }

    fn polling(app: &Entity<NyaTermApp>, cx: &mut TestAppContext) -> Vec<RemoteMonitorKind> {
        cx.update_entity(app, |app, cx| {
            RemoteMonitorKind::ALL
                .into_iter()
                .filter(|kind| app.remote_panels.entity(*kind).read(cx).is_polling())
                .collect()
        })
    }

    /// The state the app spends nearly all of its life in: nothing polls.
    #[test]
    fn nothing_polls_without_a_panel_or_a_session() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        cx.update_entity(&app, |app, cx| app.sync_remote_panel_demand(cx));
        assert_eq!(polling(&app, &mut cx), Vec::new());
    }

    /// An open panel polls, and only that one.
    ///
    /// The session is required: demand is gated on an active SSH config, so a panel
    /// open with no session still costs nothing.
    #[test]
    fn an_open_panel_polls_and_its_neighbours_do_not() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        cx.update_entity(&app, |app, cx| {
            let mut summary = app.settings.summary().clone();
            summary.ui_show_gpu_monitor = true;
            app.settings.replace_summary(summary);
            app.open_or_toggle_panel(crate::models::NavItem::GpuMonitor, cx);
            app.sync_remote_panel_demand(cx);
        });
        assert_eq!(
            polling(&app, &mut cx),
            Vec::new(),
            "no session means nothing to poll, however many panels are open"
        );

        activate_ssh_session(&app, &mut cx);
        cx.update_entity(&app, |app, cx| app.sync_remote_panel_demand(cx));
        assert_eq!(polling(&app, &mut cx), vec![RemoteMonitorKind::Gpu]);
    }

    /// Closing the panel must drop the clock, not merely mark it unwanted.
    ///
    /// This is the success criterion for the whole extraction: an inactive panel does
    /// not poll. It holds because the `Task` handle *is* the demand -- dropping it
    /// cancels the loop -- so there is no flag that can disagree with reality.
    #[test]
    fn closing_a_panel_drops_its_clock() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        activate_ssh_session(&app, &mut cx);
        cx.update_entity(&app, |app, cx| {
            let mut summary = app.settings.summary().clone();
            summary.ui_show_gpu_monitor = true;
            app.settings.replace_summary(summary);
            app.open_or_toggle_panel(crate::models::NavItem::GpuMonitor, cx);
            app.sync_remote_panel_demand(cx);
        });
        assert_eq!(polling(&app, &mut cx), vec![RemoteMonitorKind::Gpu]);

        cx.update_entity(&app, |app, cx| {
            // Toggling the same panel again closes it, which is what the activity bar
            // does.
            app.open_or_toggle_panel(crate::models::NavItem::GpuMonitor, cx);
            app.sync_remote_panel_demand(cx);
        });
        assert_eq!(polling(&app, &mut cx), Vec::new());
    }

    /// The header status bar keeps stats polling with the Stats panel closed.
    ///
    /// This is the case that decided the design. "Armed on mount, dropped on unmount"
    /// would stop the header updating as soon as its panel closed, because the header
    /// is a second, independent consumer of the same metric. It works because the
    /// entity is owned for the app's whole life and its demand is not "am I visible"
    /// but "does anything want this".
    #[test]
    fn the_header_keeps_a_metric_polling_with_its_panel_closed() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        activate_ssh_session(&app, &mut cx);
        cx.update_entity(&app, |app, cx| {
            let mut summary = app.settings.summary().clone();
            summary.ui_header_status_visible = true;
            summary.ui_header_status_mode = crate::models::HeaderStatusMode::Resources
                .persistence_id()
                .to_string();
            app.settings.replace_summary(summary);
            app.sync_remote_panel_demand(cx);
        });
        assert!(
            !cx.update_entity(&app, |app, _| {
                app.panel_is_rendered(crate::models::NavItem::Stats)
            }),
            "the Stats panel must be closed for this test to mean anything"
        );
        assert_eq!(polling(&app, &mut cx), vec![RemoteMonitorKind::Stats]);

        // Switch the header away and the last consumer is gone.
        cx.update_entity(&app, |app, cx| {
            let mut summary = app.settings.summary().clone();
            summary.ui_header_status_mode = crate::models::HeaderStatusMode::Session
                .persistence_id()
                .to_string();
            app.settings.replace_summary(summary);
            app.sync_remote_panel_demand(cx);
        });
        assert_eq!(polling(&app, &mut cx), Vec::new());
    }

    /// A panel switched off in settings renders a placeholder, and a placeholder must
    /// not poll.
    #[test]
    fn a_panel_disabled_in_settings_does_not_poll() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        activate_ssh_session(&app, &mut cx);
        cx.update_entity(&app, |app, cx| {
            let mut summary = app.settings.summary().clone();
            summary.ui_show_gpu_monitor = false;
            app.settings.replace_summary(summary);
            app.open_or_toggle_panel(crate::models::NavItem::GpuMonitor, cx);
            app.sync_remote_panel_demand(cx);
        });
        assert_eq!(polling(&app, &mut cx), Vec::new());
    }

    /// A paint must arm the clock, with nothing calling the reconcile by hand.
    ///
    /// Every other test here calls `sync_remote_panel_demand` directly, and all of them
    /// keep passing with the call removed from `render` -- checked, not assumed. That is
    /// the same gap that let `3904c69b`'s dead clock through: they prove the mechanism and
    /// say nothing about the wiring. This one drives a real paint instead.
    ///
    /// It never advances the clock, so no refresh is submitted and nothing connects;
    /// arming is all that is being asserted.
    #[test]
    fn a_paint_arms_the_clock_for_an_open_panel() {
        let mut cx = TestAppContext::single();
        let app = app(&mut cx);
        activate_ssh_session(&app, &mut cx);
        cx.update_entity(&app, |app, cx| {
            app.sync_component_theme(cx);
            let mut summary = app.settings.summary().clone();
            summary.ui_show_gpu_monitor = true;
            app.settings.replace_summary(summary);
            app.open_or_toggle_panel(crate::models::NavItem::GpuMonitor, cx);
        });
        assert_eq!(
            polling(&app, &mut cx),
            Vec::new(),
            "nothing has painted yet, so no clock can be armed"
        );

        let host_app = app.clone();
        let (_, cx) = cx.add_window_view(move |_, _| AppHost { app: host_app });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();

        let armed = cx.update(|_, cx| {
            RemoteMonitorKind::ALL
                .into_iter()
                .filter(|kind| {
                    app.read(cx)
                        .remote_panels
                        .entity(*kind)
                        .read(cx)
                        .is_polling()
                })
                .collect::<Vec<_>>()
        });
        assert_eq!(
            armed,
            vec![RemoteMonitorKind::Gpu],
            "a paint with the GPU panel open must arm exactly that panel's clock"
        );
    }
}
