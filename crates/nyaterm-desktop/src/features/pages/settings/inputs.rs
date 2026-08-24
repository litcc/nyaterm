//! Building the inputs each Settings tab draws.
//!
//! Creating an input is a mutation: `text_input` and `number_input` build the entity
//! and its change subscription on first use, so a render that calls them writes to
//! authoritative state on the first frame that shows the field. Every Settings
//! section did exactly that, unconditionally, so the first paint of each tab created
//! between one and seven inputs.
//!
//! The seeds and options live here now rather than at the render sites, which look
//! their inputs up by id and cannot create one. That is a relocation, not a
//! duplication: the render sites lost those arguments entirely.
//!
//! One consequence worth naming, because it looks like a behaviour change and is not:
//! `number_input` returns early for an id it already holds, so options derived from
//! state -- `range(1.0, max_chars)`, `disabled(!debounce_enabled)` -- were only ever
//! honoured on the frame that created the input. Building at tab activation keeps that
//! exactly, because activation is the moment that first frame used to be.

use rust_i18n::t;

use gpui::Context;
use nyaterm_ui::NyaNumberInputOptions;

use crate::features::NyaTermApp;
use crate::features::text_inputs::TextInputSetup;
use nyaterm_core::RecordingRotationPolicy;

use crate::models::SettingsTab;

impl NyaTermApp {
    /// Build every input the given tab draws.
    ///
    /// Called when the Settings page opens and whenever a tab is activated. Cheap to
    /// repeat: each `ensure_*` returns early for an input that already exists.
    pub(in crate::features) fn ensure_settings_tab_inputs(
        &mut self,
        tab: SettingsTab,
        cx: &mut Context<Self>,
    ) {
        match tab {
            SettingsTab::General => {}
            SettingsTab::Appearance => self.ensure_appearance_inputs(cx),
            SettingsTab::Interaction => self.ensure_interaction_inputs(cx),
            SettingsTab::Keybindings => self.ensure_keybinding_inputs(cx),
            SettingsTab::TerminalGeneral => self.ensure_terminal_inputs(cx),
            SettingsTab::Search => {}
            SettingsTab::Translation => self.ensure_translation_inputs(cx),
            SettingsTab::AiGeneral => self.ensure_ai_general_inputs(cx),
            SettingsTab::AiModels => self.ensure_ai_model_inputs(cx),
            SettingsTab::AiRules => self.ensure_ai_rule_inputs(cx),
            SettingsTab::Transfer => self.ensure_transfer_inputs(cx),
            SettingsTab::Security => self.ensure_security_inputs(cx),
            SettingsTab::SyncBackup => self.ensure_cloud_sync_settings_inputs(cx),
        }
    }

    fn ensure_appearance_inputs(&mut self, cx: &mut Context<Self>) {
        let terminal_font_size = self.settings.summary().terminal_font_size.to_string();
        self.ensure_number_input(
            "appearance.number.terminal-font-size",
            &terminal_font_size,
            NyaNumberInputOptions::default().range(8.0, 72.0).step(1.0),
            cx,
        );
        let ui_font_size = self.settings.summary().ui_font_size.to_string();
        self.ensure_number_input(
            "appearance.number.ui-font-size",
            &ui_font_size,
            NyaNumberInputOptions::default().range(12.0, 24.0).step(1.0),
            cx,
        );
    }

    fn ensure_interaction_inputs(&mut self, cx: &mut Context<Self>) {
        let word_separators = self.settings.summary().interaction_word_separators.clone();
        self.ensure_text_input(
            "settings.interaction.word-separators",
            &word_separators,
            TextInputSetup::default(),
            cx,
        );
        let summary = self.settings.summary();
        let min_chars = summary.interaction_command_suggestion_min_chars;
        let max_chars = summary.interaction_command_suggestion_max_chars;
        let delay_ms = summary.interaction_duplicate_session_command_delay_ms;
        self.ensure_number_input(
            "settings.number.command-suggestion-min-chars",
            &min_chars.to_string(),
            NyaNumberInputOptions::default()
                .range(1.0, max_chars as f64)
                .step(1.0),
            cx,
        );
        self.ensure_number_input(
            "settings.number.command-suggestion-max-chars",
            &max_chars.to_string(),
            NyaNumberInputOptions::default()
                .range(min_chars as f64, 500.0)
                .step(1.0),
            cx,
        );
        self.ensure_number_input(
            "settings.number.duplicate-session-command-delay",
            &delay_ms.to_string(),
            NyaNumberInputOptions::default()
                .range(0.0, 60_000.0)
                .step(100.0)
                .suffix("ms"),
            cx,
        );
    }

    fn ensure_keybinding_inputs(&mut self, cx: &mut Context<Self>) {
        let search = self.settings.keybinding_presentation().search_draft;
        self.ensure_text_input(
            "settings.keybindings.search",
            &search,
            TextInputSetup::placeholder(t!("settings.keybindingsSearch")),
            cx,
        );
    }

    fn ensure_terminal_inputs(&mut self, cx: &mut Context<Self>) {
        let summary = self.settings.summary().clone();
        self.ensure_text_input(
            "settings.terminal.x11-display",
            &summary.x11_display,
            TextInputSetup::placeholder(t!("settings.x11DisplayPlaceholder")),
            cx,
        );
        self.ensure_text_input(
            "settings.terminal.timestamp-format",
            &summary.terminal_timestamp_format,
            TextInputSetup::placeholder("[HH:mm:ss]"),
            cx,
        );
        for (id, value, min, max, step) in [
            (
                "settings.number.terminal-scrollback-lines",
                summary.terminal_scrollback_lines as f64,
                100.0,
                100_000.0,
                100.0,
            ),
            (
                "settings.number.terminal-keep-alive-interval",
                summary.terminal_keep_alive_interval as f64,
                0.0,
                600.0,
                5.0,
            ),
            (
                "settings.number.remote-stats-interval",
                summary.ui_remote_stats_interval as f64,
                1.0,
                60.0,
                1.0,
            ),
            (
                "settings.number.gpu-monitor-interval",
                summary.ui_gpu_monitor_interval as f64,
                3.0,
                120.0,
                1.0,
            ),
            (
                "settings.number.ascend-npu-monitor-interval",
                summary.ui_ascend_npu_monitor_interval as f64,
                3.0,
                120.0,
                1.0,
            ),
            (
                "settings.number.process-manager-interval",
                summary.ui_process_manager_interval as f64,
                3.0,
                120.0,
                1.0,
            ),
            (
                "settings.number.docker-manager-interval",
                summary.ui_docker_manager_interval as f64,
                3.0,
                120.0,
                1.0,
            ),
        ] {
            self.ensure_number_input(
                id,
                &format!("{}", value as i64),
                NyaNumberInputOptions::default().range(min, max).step(step),
                cx,
            );
        }
    }

    fn ensure_transfer_inputs(&mut self, cx: &mut Context<Self>) {
        let summary = self.settings.summary().clone();
        for (id, value, min, max, step) in [
            (
                "settings.number.transfer-download-threads",
                summary.transfer_download_threads as f64,
                1.0,
                10.0,
                1.0,
            ),
            (
                "settings.number.transfer-upload-threads",
                summary.transfer_upload_threads as f64,
                1.0,
                10.0,
                1.0,
            ),
            (
                "settings.number.transfer-max-retries",
                summary.transfer_max_retries as f64,
                0.0,
                10.0,
                1.0,
            ),
            (
                "settings.number.transfer-buffer-size",
                summary.transfer_buffer_size as f64,
                8.0,
                256.0,
                8.0,
            ),
        ] {
            self.ensure_number_input(
                id,
                &format!("{}", value as i64),
                NyaNumberInputOptions::default().range(min, max).step(step),
                cx,
            );
        }

        for (id, value, placeholder) in [
            (
                "settings.transfer.default-permissions",
                summary.transfer_default_file_permissions.clone(),
                "644".to_string(),
            ),
            (
                "settings.transfer.default-editor",
                summary.transfer_default_editor.clone(),
                t!("settings.defaultEditor").to_string(),
            ),
            (
                "settings.transfer.download-path",
                summary.transfer_download_path.clone(),
                t!("settings.downloadPath").to_string(),
            ),
            (
                "settings.recording.path",
                summary.recording_path.clone(),
                t!("settings.recordingPath").to_string(),
            ),
            (
                "settings.recording.path-template",
                summary.recording_path_template.clone(),
                nyaterm_core::DEFAULT_RECORDING_PATH_TEMPLATE.to_string(),
            ),
        ] {
            self.ensure_text_input(id, &value, TextInputSetup::placeholder(placeholder), cx);
        }

        // Recording lives on the Transfer tab, and both of its inputs are in MiB while
        // the settings they mirror are in bytes.
        let memory_mib =
            (self.settings.summary().recording_memory_limit_bytes / (1024 * 1024)).max(1);
        let rotation_size_mib = match self.settings.summary().recording_rotation {
            RecordingRotationPolicy::Size { max_bytes } => (max_bytes / (1024 * 1024)).max(1),
            _ => 50,
        };
        self.ensure_number_input(
            "settings.number.recording-rotation-size",
            &rotation_size_mib.to_string(),
            NyaNumberInputOptions::default()
                .range(1.0, 102_400.0)
                .step(1.0)
                .suffix("MiB"),
            cx,
        );
        self.ensure_number_input(
            "settings.number.recording-memory-limit",
            &memory_mib.to_string(),
            NyaNumberInputOptions::default()
                .range(1.0, 512.0)
                .step(1.0)
                .suffix("MiB"),
            cx,
        );
    }

    fn ensure_security_inputs(&mut self, cx: &mut Context<Self>) {
        let master_password_draft = self.settings.master_password().draft.to_string();
        self.ensure_text_input(
            "settings.security.master-password",
            &master_password_draft,
            crate::features::text_inputs::secret_input_setup(true),
            cx,
        );
        let idle_minutes = self.settings.summary().idle_lock_minutes;
        self.ensure_number_input(
            "settings.number.idle-lock-minutes",
            &idle_minutes.to_string(),
            NyaNumberInputOptions::default()
                .range(0.0, 1440.0)
                .step(1.0)
                .suffix(t!("common.minutes").to_string()),
            cx,
        );
    }

    fn ensure_ai_general_inputs(&mut self, cx: &mut Context<Self>) {
        self.ensure_ai_settings_inputs(cx);
        let config = self.ai.settings_config().clone();
        for (id, value, min, max, step) in [
            (
                "ai.number.context-line-limit",
                config.context_line_limit as f64,
                50.0,
                500.0,
                50.0,
            ),
            (
                "ai.number.timeout-ms",
                config.timeout_ms as f64,
                5_000.0,
                300_000.0,
                1_000.0,
            ),
            (
                "ai.number.agent-steps",
                config.max_agent_steps.unwrap_or(10) as f64,
                1.0,
                50.0,
                1.0,
            ),
            (
                "ai.number.agent-step-timeout-ms",
                config.agent_step_timeout_ms.unwrap_or(30_000) as f64,
                5_000.0,
                120_000.0,
                1_000.0,
            ),
            (
                "ai.number.terminal-output-lines",
                config.terminal_output_lines as f64,
                0.0,
                100.0,
                1.0,
            ),
        ] {
            self.ensure_number_input(
                id,
                &format!("{}", value as i64),
                NyaNumberInputOptions::default().range(min, max).step(step),
                cx,
            );
        }
    }

    fn ensure_ai_model_inputs(&mut self, cx: &mut Context<Self>) {
        // Three fields per provider credential, all drawn together.
        let credentials = self.ai.settings_config().provider_credentials.clone();
        let drafts = self.ai.settings_credential_secret_drafts().clone();
        for credential in credentials {
            let secret = drafts.get(&credential.id).cloned().unwrap_or_default();
            for (suffix, value, secret_field) in [
                ("name", credential.name.clone(), false),
                (
                    "base-url",
                    credential.base_url.clone().unwrap_or_default(),
                    false,
                ),
                ("api-key", secret, true),
            ] {
                self.ensure_text_input(
                    format!("ai.credential.{}.{suffix}", credential.id),
                    &value,
                    crate::features::text_inputs::secret_input_setup(secret_field),
                    cx,
                );
            }
        }
        let query = self.ai.settings_model_query().to_string();
        self.ensure_text_input(
            "ai.settings.model-search",
            &query,
            TextInputSetup::placeholder(t!("ai.searchModels")),
            cx,
        );
    }

    fn ensure_ai_rule_inputs(&mut self, cx: &mut Context<Self>) {
        // One name and one prompt per terminal action, all drawn together.
        for (name_id, name, prompt_id, prompt) in self.ai_action_input_specs() {
            self.ensure_text_input(name_id, &name, TextInputSetup::placeholder(""), cx);
            self.ensure_text_input(prompt_id, &prompt, TextInputSetup::multi_line(""), cx);
        }
        let file_size_mb =
            (self.ai.settings_config().max_ai_file_size_bytes / (1024 * 1024)).max(1);
        self.ensure_number_input(
            "ai.number.file-size-mb",
            &file_size_mb.to_string(),
            NyaNumberInputOptions::default().range(1.0, 256.0).step(1.0),
            cx,
        );
    }
}

/// Every tab, so the test below cannot silently miss one that gets added.
#[cfg(test)]
const ALL_SETTINGS_TABS: [SettingsTab; 13] = [
    SettingsTab::General,
    SettingsTab::Appearance,
    SettingsTab::Interaction,
    SettingsTab::Keybindings,
    SettingsTab::TerminalGeneral,
    SettingsTab::Search,
    SettingsTab::Translation,
    SettingsTab::AiGeneral,
    SettingsTab::AiModels,
    SettingsTab::AiRules,
    SettingsTab::Transfer,
    SettingsTab::Security,
    SettingsTab::SyncBackup,
];

#[cfg(test)]
mod tests {
    use gpui::{
        AppContext as _, Entity, IntoElement, ParentElement as _, Render, Styled as _,
        TestAppContext, VisualTestContext, div, px,
    };
    use nyaterm_core::{AppRuntime, RuntimeMode, uuid};

    use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
    use crate::features::NyaTermApp;
    use crate::models::{NavItem, SettingsTab};

    use super::ALL_SETTINGS_TABS;

    const VIEWPORT_WIDTH: f32 = 1_100.;

    fn app(cx: &mut TestAppContext) -> Entity<NyaTermApp> {
        // A uuid rather than a clock reading: these tests run in parallel and
        // Windows' ~15ms clock granularity lets a nanosecond timestamp repeat,
        // which would share one config dir and so one settings database.
        let root = std::env::temp_dir().join(format!(
            "nyaterm-settings-inputs-{}-{}",
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

    /// Draws the settings surface through a real window.
    ///
    /// Rendering without one leaks by construction: elements are arena-allocated and
    /// the arena is only cleared when `Window::draw` completes, so an element built
    /// outside a draw holds its entity handles until the process exits. That is why
    /// these tests host a window and step frames rather than calling a section render
    /// directly -- the earlier no-window version reported leaks for entities the
    /// render itself creates, `NyaSelectState` among them, which no production path
    /// owns either.
    struct SettingsHost {
        surface: Option<gpui::AnyElement>,
    }

    impl Render for SettingsHost {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            // `update` here would re-enter the app from a render, and notifying the
            // app to force a frame would then schedule the next one forever. The
            // panel batch will render from a snapshot instead; for now the surface is
            // built inside the app's own update, driven from the test body.
            div().w(px(VIEWPORT_WIDTH)).h(px(800.)).child(
                self.surface
                    .take()
                    .unwrap_or_else(|| div().into_any_element()),
            )
        }
    }

    fn hosted(
        cx: &mut TestAppContext,
    ) -> (
        Entity<NyaTermApp>,
        Entity<SettingsHost>,
        &mut VisualTestContext,
    ) {
        let app = app(cx);
        cx.update_entity(&app, |app, cx| {
            app.sync_component_theme(cx);
            app.open_page(NavItem::Settings, cx);
        });
        let host_app = app.clone();
        let _ = host_app;
        let (host, vcx) = cx.add_window_view(move |_, _| SettingsHost { surface: None });
        let vcx: &mut VisualTestContext = vcx;
        vcx.run_until_parked();
        (app, host, vcx)
    }

    /// Build the settings surface inside the app's own update, hand it to the host,
    /// and draw one real frame.
    ///
    /// The element must be built and drawn in the same draw for the arena to release
    /// it, which is the whole reason these tests are hosted.
    fn draw(host: &Entity<SettingsHost>, app: &Entity<NyaTermApp>, vcx: &mut VisualTestContext) {
        vcx.update(|window, cx| {
            let surface = app.update(cx, |app, cx| app.settings_window_view(VIEWPORT_WIDTH, cx));
            host.update(cx, |host, cx| {
                host.surface = Some(surface);
                cx.notify();
            });
            _ = window.draw(cx);
        });
        vcx.run_until_parked();
    }

    /// Activating a tab must build every input that tab draws, and drawing it must
    /// build none.
    ///
    /// The render sites can no longer create one, so a tab whose ensure list is short
    /// would draw an empty slot. `existing_number_input_box` and
    /// `existing_text_input_box` both trip a debug assertion in that case, so drawing
    /// every tab here turns a gap into a failure rather than a blank row -- which is
    /// how the cross-line call sites, the AI credential rows and the full Cloud Sync
    /// provider field set were found.
    #[test]
    fn every_settings_tab_builds_its_inputs_and_drawing_builds_none() {
        let mut cx = TestAppContext::single();
        let (app, host, vcx) = hosted(&mut cx);

        for tab in ALL_SETTINGS_TABS {
            vcx.update(|_, cx| {
                app.update(cx, |app, cx| {
                    app.ensure_settings_tab_inputs(tab, cx);
                    app.shell.set_settings_active_tab(tab);
                });
            });
            // The first draw is where a missing input would assert.
            draw(&host, &app, vcx);
            let after_first = vcx.update(|_, cx| app.read(cx).text_input_count_for_test());
            draw(&host, &app, vcx);
            assert_eq!(
                vcx.update(|_, cx| app.read(cx).text_input_count_for_test()),
                after_first,
                "drawing the {tab:?} tab again created an input"
            );
        }
    }

    /// Static tab inputs outlive the page; only the row-scoped ones are released.
    ///
    /// `finish_settings_page` forgets exactly three prefixes -- `ai.settings.action.`,
    /// `ai.settings.manual-model.` and `keyword.highlight.` -- and nothing else, so a
    /// number or text input belonging to a tab lives for as long as the app does. That
    /// predates this change: they used to be created by the first render of their tab
    /// and were never forgotten either. Pinning it here so the panel batch, which will
    /// carry these handles in a snapshot, does not mistake it for a leak.
    ///
    /// The release half of the lifecycle is proven by these tests reaching teardown at
    /// all: `TestAppContext` fails on any entity still reachable when it drops, so a
    /// clean exit is the assertion that nothing but the app retains them.
    #[test]
    fn static_settings_inputs_outlive_the_page() {
        let mut cx = TestAppContext::single();
        let (app, host, vcx) = hosted(&mut cx);

        vcx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.ensure_settings_tab_inputs(SettingsTab::Security, cx);
                app.shell.set_settings_active_tab(SettingsTab::Security);
            });
        });
        draw(&host, &app, vcx);

        let probe = vcx.update(|_, cx| {
            app.read(cx)
                .existing_text_input("settings.security.master-password")
                .expect("activation built the master-password input")
                .downgrade()
        });

        vcx.update(|_, cx| {
            app.update(cx, |app, cx| app.cancel_settings(cx));
        });
        draw(&host, &app, vcx);

        assert!(
            vcx.update(|_, _| probe.upgrade().is_some()),
            "a static tab input is app-lifetime; page close must not release it"
        );
    }
}
