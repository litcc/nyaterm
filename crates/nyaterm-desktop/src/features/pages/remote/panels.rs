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

use gpui::{AnyElement, Context, Entity, IntoElement, Render, WeakEntity, Window, div, prelude::*};

use crate::features::NyaTermApp;

/// Which of the five polling panels an entity is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::features) enum RemoteMonitorKind {
    Stats,
    Gpu,
    Npu,
    Processes,
    Docker,
}

/// One remote polling panel.
pub(in crate::features) struct RemoteMonitorPanel {
    kind: RemoteMonitorKind,
    /// Weak on purpose. `NyaTermApp` owns this entity, so a strong handle back would
    /// be a reference cycle that keeps both alive forever -- which is what
    /// `ConnectionPanel` does today. The app always outlives the panels it owns, so
    /// the upgrade below only fails during teardown.
    app: WeakEntity<NyaTermApp>,
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
            #[cfg(test)]
            paint_count: 0,
        }
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
}
