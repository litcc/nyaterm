use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use gpui::{
    Context, Entity, FocusHandle, IntoElement, Render, Rgba, ScrollHandle, UniformListScrollHandle,
    WeakEntity, Window,
};
use nyaterm_transport::SftpFileEntry;
use nyaterm_ui::NyaInputState;

use crate::features::NyaTermApp;
use crate::features::session::SftpDuplicatePromptState;
use crate::features::transfers::TRANSFER_CWD_SYNC_POLL_INTERVAL;
use crate::models::{
    TransferBrowserColumnResizeState, TransferBrowserColumnWidths, TransferBrowserSortColumn,
    TransferBrowserSortDirection, TransferJobRowSnapshot, TransferRenameState,
};
use crate::theme::ThemePalette;

use super::TransferBrowserAvailability;

/// Colours the transfers panel needs that are not on the palette itself.
///
/// All three are wallpaper-dependent, so they cannot be derived from the palette
/// alone and have to be resolved where the shell settings live.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::features) struct TransferChrome {
    pub palette: ThemePalette,
    pub transparent_surface: Rgba,
    pub transparent_section_header: Rgba,
    pub surface: Rgba,
    /// For `bounded_dialog_width`, which the duplicate prompt sizes against.
    pub viewport_width: f32,
}

/// The browser's render state, owned.
///
/// `TransferBrowserView` is a borrowed projection, which is exactly right for a
/// caller that already holds the state and wrong for a panel that must not touch
/// it. The costly field -- the listing -- is shared rather than copied; the rest are
/// short paths and small sets.
pub(in crate::features) struct TransferBrowserPresentation {
    pub path: String,
    pub home_dir: String,
    pub path_editing: bool,
    /// The filtered, sorted listing straight from the state's memo. A progress batch
    /// leaves the memo alone, so this is a refcount bump on the hot path.
    pub visible_entries: Arc<[SftpFileEntry]>,
    /// The unfiltered listing, shared. The footer counts against it.
    pub all_entries: Arc<Vec<SftpFileEntry>>,
    pub loading: bool,
    pub error: Option<String>,
    pub search: String,
    pub search_expanded: bool,
    pub list_scroll: UniformListScrollHandle,
    pub horizontal_scroll: ScrollHandle,
    pub visited_history: VecDeque<String>,
    pub favorites: VecDeque<String>,
    pub sort_column: TransferBrowserSortColumn,
    pub sort_direction: TransferBrowserSortDirection,
    pub column_widths: TransferBrowserColumnWidths,
    pub column_resize: Option<TransferBrowserColumnResizeState>,
    pub selected_remote_path: Option<String>,
    pub selected_remote_paths: HashSet<String>,
    pub external_drop_hover: bool,
    pub focus: FocusHandle,
    pub rename: Option<TransferRenameState>,
    pub auto_sync_cwd_enabled: bool,
    pub connection_id: Option<String>,
    /// Built where the field is revealed, never in render.
    pub search_field: Option<Entity<NyaInputState>>,
    pub path_field: Option<Entity<NyaInputState>>,
    /// Built where the rename dialog opens, so the virtualised row builder can show
    /// the field without being able to create one.
    pub rename_field: Option<Entity<NyaInputState>>,
    pub show_hidden_files: bool,
}

/// The queue's render state.
pub(in crate::features) struct TransferQueuePresentation {
    /// Already filtered to the active session and already ordered. Both used to
    /// happen in render, each with a full clone of every job.
    pub rows: Arc<[TransferJobRowSnapshot]>,
    pub has_running: bool,
    pub has_paused: bool,
    pub has_active: bool,
    pub has_completed: bool,
    pub has_stopped: bool,
    pub selected_job_id: Option<String>,
    pub download_path: String,
    pub focus: FocusHandle,
}

/// Everything the panel draws from: data, never elements.
pub(in crate::features) struct TransferSnapshot {
    pub chrome: TransferChrome,
    pub availability: TransferBrowserAvailability,
    pub panel_height: f32,
    pub height_is_resizing: bool,
    pub resize_handle_highlighted: bool,
    pub has_session: bool,
    pub panel_focus: gpui::FocusHandle,
    pub duplicate_prompt: Option<SftpDuplicatePromptState>,
    pub browser: TransferBrowserPresentation,
    pub queue: TransferQueuePresentation,
    /// Whether the browser wants its remote cwd polled.
    ///
    /// The app decides this -- it depends on which panel is open, which the panel
    /// cannot see -- and the panel acts on it by owning or dropping the task.
    pub cwd_sync_demand: bool,
}

pub(in crate::features) struct TransferPanel {
    /// Weak, so the panel does not keep the app alive.
    app: WeakEntity<NyaTermApp>,
    snapshot: Option<TransferSnapshot>,
    /// The cwd-sync poll, owned here rather than by the app.
    ///
    /// A `Task` dropped with its owner, so the poll cannot outlive the panel that
    /// wants it. The panel decides *when to ask*; the app still owns the cwd itself
    /// and every mutation of it, reached through an event-time hop.
    cwd_clock: Option<gpui::Task<()>>,
    #[cfg(test)]
    paint_count: usize,
    #[cfg(test)]
    rows_built: std::cell::Cell<usize>,
}

impl TransferPanel {
    pub(in crate::features) fn new(app: WeakEntity<NyaTermApp>) -> Self {
        Self {
            app,
            snapshot: None,
            cwd_clock: None,
            #[cfg(test)]
            paint_count: 0,
            #[cfg(test)]
            rows_built: std::cell::Cell::new(0),
        }
    }

    pub(in crate::features::pages::transfers) fn snapshot(&self) -> Option<&TransferSnapshot> {
        self.snapshot.as_ref()
    }

    pub(in crate::features) fn set_snapshot(
        &mut self,
        snapshot: TransferSnapshot,
        cx: &mut Context<Self>,
    ) {
        let demand = snapshot.cwd_sync_demand;
        self.snapshot = Some(snapshot);
        self.reconcile_cwd_clock(demand, cx);
        cx.notify();
    }

    /// Own or drop the cwd-sync poll to match demand.
    ///
    /// The task lives here so it is dropped with the panel: a poll cannot outlive the
    /// view that wanted it, which is what "lifecycle-scoped" has to mean. The panel
    /// only decides *when to ask*; every beat hops back to the app, which owns the
    /// cwd and every mutation of it.
    fn reconcile_cwd_clock(&mut self, demand: bool, cx: &mut Context<Self>) {
        if !demand {
            // Dropping the `Task` cancels it.
            self.cwd_clock = None;
            return;
        }
        if self.cwd_clock.is_some() {
            return;
        }
        let app = self.app.clone();
        self.cwd_clock = Some(cx.spawn(async move |_panel, cx| {
            loop {
                cx.background_executor()
                    .timer(TRANSFER_CWD_SYNC_POLL_INTERVAL)
                    .await;
                let Some(app) = app.upgrade() else {
                    break;
                };
                // The app owns the decision -- whether the interval is due, whether a
                // job is in flight, whether the shell is calm enough -- and the cwd.
                // No error to handle: the strong handle from `upgrade` keeps the app
                // alive for the call, and a released app ends the loop above.
                let kept = app.update(cx, |app, cx| {
                    app.sync_transfer_cwd_if_due(cx);
                    app.transfer_cwd_sync_needs_polling()
                });
                if !kept {
                    break;
                }
            }
        }));
    }

    /// The one hop from a panel interaction back to the owner.
    ///
    /// Flushing on the way out means an interaction needs no boundary of its own:
    /// the mutation and the rebuild are one transaction.
    pub(in crate::features::pages::transfers) fn with_app<R: Default>(
        &self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut NyaTermApp, &mut Context<NyaTermApp>) -> R,
    ) -> R {
        let Some(app) = self.app.upgrade() else {
            return R::default();
        };
        app.update(cx, |app, cx| {
            let result = f(app, cx);
            app.flush_transfer_panel_snapshot(cx);
            result
        })
    }

    /// A weak handle for a deferred callback. Render must not use it.
    pub(in crate::features::pages::transfers) fn app_handle(&self) -> WeakEntity<NyaTermApp> {
        self.app.clone()
    }

    #[cfg(test)]
    pub(in crate::features) fn paint_count(&self) -> usize {
        self.paint_count
    }

    #[cfg(test)]
    pub(in crate::features::pages::transfers) fn note_rows_built(&self, count: usize) {
        self.rows_built.set(self.rows_built.get() + count);
    }

    #[cfg(test)]
    pub(in crate::features) fn rows_built(&self) -> usize {
        self.rows_built.get()
    }

    #[cfg(test)]
    pub(in crate::features) fn cwd_clock_is_armed(&self) -> bool {
        self.cwd_clock.is_some()
    }
}

impl Render for TransferPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.paint_count += 1;
        }
        // Zero `NyaTermApp` access, diagnostics included: GPUI records every entity
        // read during a draw, so one app read here would put this panel back on the
        // app's invalidation path and undo the isolation.
        super::transfer_panel(self, window, cx)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui::{
        AppContext as _, Entity, IntoElement, ParentElement as _, Render, Styled as _,
        TestAppContext, VisualTestContext, div, px,
    };
    use nyaterm_core::{AppRuntime, RuntimeMode, uuid};

    use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
    use crate::features::NyaTermApp;
    use crate::models::NavItem;

    fn app(cx: &mut TestAppContext) -> Entity<NyaTermApp> {
        // A uuid rather than a clock reading: these tests run in parallel and
        // Windows' ~15ms clock granularity lets a nanosecond timestamp repeat,
        // which would share one config dir and so one settings database.
        let root = std::env::temp_dir().join(format!(
            "nyaterm-transfer-panel-{}-{}",
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

    /// Mirrors what `single_side_panel` gives the real panel: a constrained,
    /// definitely-sized body and the same cached style. Measuring `.cached()` against
    /// anything looser would not measure the shipped layout.
    struct AppHost {
        app: Entity<NyaTermApp>,
    }

    impl Render for AppHost {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            div().w(px(320.)).h(px(720.)).flex().flex_col().child(
                div().flex_1().min_h_0().overflow_hidden().child(
                    self.app
                        .read(cx)
                        .transfer_panel
                        .clone()
                        .cached(crate::features::layout::cached_panel_style()),
                ),
            )
        }
    }

    fn hosted(cx: &mut TestAppContext) -> (Entity<NyaTermApp>, &mut VisualTestContext) {
        let app = app(cx);
        cx.update_entity(&app, |app, cx| {
            app.sync_component_theme(cx);
            app.open_or_toggle_panel(NavItem::Transfers, cx);
            app.flush_transfer_panel_snapshot(cx);
        });
        let host_app = app.clone();
        let (_, vcx) = cx.add_window_view(move |_, _| AppHost { app: host_app });
        let vcx: &mut VisualTestContext = vcx;
        vcx.run_until_parked();
        for _ in 0..3 {
            vcx.update(|window, cx| {
                app.update(cx, |_, cx| cx.notify());
                _ = window.draw(cx);
            });
            vcx.run_until_parked();
        }
        (app, vcx)
    }

    fn paints(app: &Entity<NyaTermApp>, cx: &mut gpui::App) -> usize {
        app.read(cx).transfer_panel.read(cx).paint_count()
    }

    /// The point of the batch: the panel no longer rides the app's redraws.
    #[test]
    fn an_unrelated_app_notify_does_not_repaint_the_panel() {
        let mut cx = TestAppContext::single();
        let (app, vcx) = hosted(&mut cx);

        let before = vcx.update(|_, cx| paints(&app, cx));
        assert!(
            before > 0,
            "the panel must have painted at least once, or this proves nothing"
        );
        for _ in 0..5 {
            vcx.update(|window, cx| {
                app.update(cx, |_, cx| cx.notify());
                _ = window.draw(cx);
            });
            vcx.run_until_parked();
        }
        assert_eq!(
            vcx.update(|_, cx| paints(&app, cx)),
            before,
            "five unrelated app notifies must not repaint the transfers panel"
        );
    }

    /// A flush reaches the panel inside its own transaction, not on the next paint.
    #[test]
    fn a_flush_reaches_the_snapshot_before_any_paint() {
        let mut cx = TestAppContext::single();
        let (app, vcx) = hosted(&mut cx);

        vcx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.transfer.set_browser_search("needle".to_string());
                app.flush_transfer_panel_snapshot(cx);
                let panel = app.transfer_panel.read(cx);
                assert_eq!(
                    panel.snapshot().expect("flushed").browser.search,
                    "needle",
                    "the search must be in the snapshot before anything paints"
                );
            });
        });
    }

    /// The browser list is virtualised, and the snapshot holds data rather than rows,
    /// so a large directory must still only materialise what the viewport shows.
    #[test]
    fn only_the_visible_range_is_materialised() {
        let mut cx = TestAppContext::single();
        let (app, vcx) = hosted(&mut cx);
        vcx.update(|_, cx| {
            app.update(cx, |app, cx| {
                let entries = (0..500)
                    .map(super::super::tests_support::browser_entry)
                    .collect();
                app.transfer.replace_browser_entries_for_test(entries);
                app.flush_transfer_panel_snapshot(cx);
            });
        });
        for _ in 0..2 {
            vcx.update(|window, cx| {
                app.update(cx, |_, cx| cx.notify());
                _ = window.draw(cx);
            });
            vcx.run_until_parked();
        }

        let built = vcx.update(|_, cx| app.read(cx).transfer_panel.read(cx).rows_built());
        assert!(
            built < 500,
            "a 500-entry listing must not materialise every row; built {built}"
        );
    }

    /// The cwd poll belongs to the panel, and dropping the panel cancels it.
    #[test]
    fn the_panel_owns_the_cwd_clock() {
        let mut cx = TestAppContext::single();
        let (app, vcx) = hosted(&mut cx);
        vcx.update(|_, cx| {
            assert!(
                app.read(cx).transfer_panel.read(cx).cwd_clock_is_armed(),
                "an open browser must leave the poll with the panel"
            );
        });
        let _ = Duration::from_secs(1);
    }
}
