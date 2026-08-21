use rust_i18n::t;

use std::borrow::Cow;

use gpui::{
    App, AppContext, Bounds, Context, Entity, IntoElement, Render, Subscription, Window,
    WindowBounds, WindowKind, WindowOptions, div, prelude::*, px, rgb, size,
};
use nyaterm_ui::{NyaWindowHandle, nya_root};

use crate::features::{
    NyaTermApp, view_widgets::child_window_header, view_widgets::child_window_titlebar,
};
use crate::models::ConnectionEditorField;

pub(in crate::features::connections) struct ConnectionEditorWindow {
    app: Entity<NyaTermApp>,
    _app_subscription: Subscription,
    /// Focus is per-window, so the field the main window focused means nothing
    /// here; this window has to claim it on its own first frame.
    focused_initial_field: bool,
}

impl ConnectionEditorWindow {
    fn new(app: Entity<NyaTermApp>, cx: &mut Context<Self>) -> Self {
        let app_subscription = cx.observe(&app, |_, _, cx| cx.notify());
        Self {
            app,
            _app_subscription: app_subscription,
            focused_initial_field: false,
        }
    }
}

impl Render for ConnectionEditorWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(editor) = self.app.read(cx).connection_state.active_editor_draft() else {
            self.app.update(cx, |app, cx| {
                app.connection_state.clear_editor_window();
                cx.notify();
            });
            window.defer(cx, |window, _| window.remove_window());
            return div().size_full().into_any_element();
        };

        let (palette, font, font_size, title) = self.app.read_with(cx, |app, _| {
            (
                app.theme_palette(),
                app.gpui_ui_font().font(),
                app.settings.summary().ui_font_size.clamp(12, 24) as f32,
                app.connection_editor_title().to_string(),
            )
        });
        window.set_window_title(&title);
        if !self.focused_initial_field {
            self.focused_initial_field = true;
            let field = self
                .app
                .read(cx)
                .connection_state
                .editor_fields()
                .get(&ConnectionEditorField::Name)
                .cloned();
            if let Some(field) = field {
                window.focus(&field.read(cx).focus_handle(), cx);
                field.update(cx, |field, cx| field.select_all(window, cx));
            }
        }
        let content = self
            .app
            .update(cx, |app, cx| app.connection_editor_window_view(editor, cx));
        let close_app = self.app.clone();

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
                None,
                false,
                window.is_maximized(),
                move |_, window, cx| {
                    close_app.update(cx, |app, cx| app.close_connection_editor(cx));
                    window.remove_window();
                },
            ))
            .child(div().flex_1().min_h_0().overflow_hidden().child(content))
            .into_any_element()
    }
}

impl NyaTermApp {
    pub(in crate::features) fn connection_editor_title(&self) -> Cow<'static, str> {
        if self.connection_state.editor_is_editing_saved_connection() {
            t!("dialog.editConnection")
        } else {
            t!("dialog.newConnection")
        }
    }

    pub(in crate::features) fn activate_connection_editor_window(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(handle) = self.connection_state.editor_window_handle() else {
            return false;
        };
        let app = cx.entity();
        cx.defer(move |cx| {
            if handle
                .update(cx, |_, window, _| window.activate_window())
                .is_err()
            {
                app.update(cx, |app, cx| {
                    if app.connection_state.clear_editor_window_if_current(handle) {
                        cx.notify();
                    }
                });
            }
        });
        true
    }

    pub(in crate::features) fn open_connection_editor_window(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.activate_connection_editor_window(cx) {
            return true;
        }
        if self.connection_state.editor_window_open_pending() {
            return true;
        }

        self.connection_state.mark_editor_window_pending();
        cx.notify();
        let app = cx.entity();
        cx.defer(move |cx| {
            let should_open = app.read(cx).connection_state.editor_window_open_pending();
            if should_open {
                open_connection_editor_window_now_from_app(app, cx);
            }
        });
        true
    }
}

fn open_connection_editor_window_now_from_app(app: Entity<NyaTermApp>, cx: &mut App) {
    if app.read(cx).connection_state.editor_has_window() {
        app.update(cx, |app, cx| {
            app.connection_state.clear_editor_window_pending();
            app.activate_connection_editor_window(cx);
            cx.notify();
        });
        return;
    }
    if !app.read(cx).connection_state.editor_has_draft() {
        app.update(cx, |app, cx| {
            app.connection_state.clear_editor_window_pending();
            cx.notify();
        });
        return;
    }

    let title = app.read(cx).connection_editor_title().to_string();
    let bounds = Bounds::centered(None, size(px(520.), px(620.)), cx);
    let close_app = app.clone();
    let view_app = app.clone();
    let result: anyhow::Result<NyaWindowHandle> = cx.open_window(
        WindowOptions {
            titlebar: child_window_titlebar(title),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(420.), px(480.))),
            kind: WindowKind::Floating,
            is_minimizable: false,
            ..Default::default()
        },
        move |window, cx| {
            window.on_window_should_close(cx, move |_, cx| {
                close_app.update(cx, |app, cx| {
                    app.connection_state.close_editor();
                    app.shell
                        .set_status(t!("dialog.connectionEditorClosed").to_string());
                    cx.notify();
                });
                true
            });
            let editor_focus = view_app.read(cx).connection_state.editor_focus_handle();
            window.focus(&editor_focus, cx);
            let view = cx.new(|cx| ConnectionEditorWindow::new(view_app, cx));
            cx.new(|cx| nya_root(view, window, cx))
        },
    );

    app.update(cx, |app, cx| match result {
        Ok(handle) => {
            app.connection_state.attach_editor_window(handle);
            cx.notify();
        }
        Err(error) => {
            app.connection_state.clear_editor_window();
            app.shell.set_status(
                t!("dialog.connectionEditorOpenFailed").replace("{{error}}", &error.to_string()),
            );
            cx.notify();
        }
    });
}
