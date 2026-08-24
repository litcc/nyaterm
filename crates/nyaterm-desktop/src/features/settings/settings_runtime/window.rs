use rust_i18n::t;

use gpui::{
    App, AppContext, Bounds, Context, Entity, IntoElement, Render, Subscription, Window,
    WindowBounds, WindowKind, WindowOptions, div, prelude::*, px, rgb, size,
};
use nyaterm_ui::{NyaWindowHandle, nya_root};

use crate::features::pages::settings::panel::{SettingsPanel, SettingsSurface};
use crate::features::{
    NyaTermApp, view_widgets::child_window_header, view_widgets::child_window_titlebar,
};

pub(in crate::features) struct SettingsWindow {
    app: Entity<NyaTermApp>,
    settings_panel: Entity<SettingsPanel>,
    _app_subscription: Subscription,
}

impl SettingsWindow {
    fn new(app: Entity<NyaTermApp>, cx: &mut Context<Self>) -> Self {
        let app_subscription = cx.observe(&app, |_, _, cx| cx.notify());
        let settings_panel = cx.new(|cx| {
            SettingsPanel::new_for_surface(app.downgrade(), SettingsSurface::NativeWindow, cx)
        });
        app.update(cx, |app, cx| {
            app.register_native_settings_panel(&settings_panel, cx);
        });
        Self {
            app,
            settings_panel,
            _app_subscription: app_subscription,
        }
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.app.read(cx).shell.has_settings_draft() {
            self.app.update(cx, |app, cx| {
                app.clear_native_settings_panel(&self.settings_panel, cx);
                app.shell.clear_settings_window();
                cx.notify();
            });
            window.defer(cx, |window, _| window.remove_window());
            return div().size_full().into_any_element();
        }

        let (palette, font, font_size, title) = self.app.read_with(cx, |app, _| {
            (
                app.theme_palette(),
                app.gpui_ui_font().font(),
                app.settings.summary().ui_font_size.clamp(12, 24) as f32,
                t!("settings.title").to_string(),
            )
        });
        window.set_window_title(&title);
        let content = self
            .settings_panel
            .clone()
            .cached(crate::features::layout::cached_panel_style());
        let close_app = self.app.clone();
        let close_panel = self.settings_panel.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(palette.bg))
            .text_color(rgb(palette.text))
            .font(font)
            .text_size(px(font_size))
            .child(child_window_header(
                palette,
                title,
                Some("icons/settings.svg"),
                false,
                window.is_maximized(),
                move |_, window, cx| {
                    close_app.update(cx, |app, cx| {
                        app.clear_native_settings_panel(&close_panel, cx);
                        app.cancel_settings(cx);
                    });
                    window.remove_window();
                },
            ))
            .child(div().flex_1().min_h_0().overflow_hidden().child(content))
            .into_any_element()
    }
}

impl NyaTermApp {
    pub(in crate::features) fn activate_settings_window(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(handle) = self.shell.settings_window() else {
            return false;
        };
        let app = cx.entity();
        cx.defer(move |cx| {
            if handle
                .update(cx, |_, window, _| window.activate_window())
                .is_err()
            {
                app.update(cx, |app, cx| {
                    if app.shell.clear_settings_window_if(handle) {
                        cx.notify();
                    }
                });
            }
        });
        true
    }

    pub(in crate::features) fn open_settings_window(&mut self, cx: &mut Context<Self>) -> bool {
        if self.activate_settings_window(cx) {
            return true;
        }
        if !self.shell.begin_settings_window_open() {
            return true;
        }
        cx.notify();
        let app = cx.entity();
        cx.defer(move |cx| {
            let should_open = app.read(cx).shell.settings_window_open_pending();
            if should_open {
                open_settings_window_now_from_app(app, cx);
            }
        });
        true
    }
}

fn open_settings_window_now_from_app(app: Entity<NyaTermApp>, cx: &mut App) {
    if app.read(cx).shell.settings_window().is_some() {
        app.update(cx, |app, cx| {
            app.shell.cancel_settings_window_open();
            app.activate_settings_window(cx);
            cx.notify();
        });
        return;
    }
    if !app.read(cx).shell.has_settings_draft() {
        app.update(cx, |app, cx| {
            app.shell.cancel_settings_window_open();
            cx.notify();
        });
        return;
    }

    let title = t!("settings.title").to_string();
    let bounds = Bounds::centered(None, size(px(800.), px(560.)), cx);
    let close_app = app.clone();
    let view_app = app.clone();
    let result: anyhow::Result<NyaWindowHandle> = cx.open_window(
        WindowOptions {
            titlebar: child_window_titlebar(title),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(640.), px(480.))),
            kind: WindowKind::Floating,
            is_minimizable: false,
            ..Default::default()
        },
        move |window, cx| {
            window.on_window_should_close(cx, move |_, cx| {
                close_app.update(cx, |app, cx| {
                    app.cancel_settings(cx);
                    app.shell.clear_settings_window();
                });
                true
            });
            let view = cx.new(|cx| SettingsWindow::new(view_app, cx));
            cx.new(|cx| nya_root(view, window, cx))
        },
    );

    app.update(cx, |app, cx| match result {
        Ok(handle) => {
            app.shell.complete_settings_window_open(handle);
            cx.notify();
        }
        Err(error) => {
            app.shell.fail_settings_window_open();
            app.shell
                .set_status(format!("failed to open settings window: {error}"));
            cx.notify();
        }
    });
}
