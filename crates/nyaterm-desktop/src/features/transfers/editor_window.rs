use rust_i18n::t;

use std::collections::{HashMap, HashSet};

use gpui::{
    App, AppContext, Bounds, Context, Entity, IntoElement, Render, Subscription, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use nyaterm_ui::{NyaWindowHandle, nya_root};

use super::remote_text_editor::RemoteTextEditor;
use crate::features::{
    NyaTermApp, view_widgets::child_window_header, view_widgets::child_window_titlebar,
};

pub(super) struct RemoteFileEditorWindow {
    app: Entity<NyaTermApp>,
    editors: HashMap<String, Entity<RemoteTextEditor>>,
    active_editor_id: Option<String>,
    _app_subscription: Subscription,
}

impl RemoteFileEditorWindow {
    fn new(app: Entity<NyaTermApp>, cx: &mut Context<Self>) -> Self {
        let app_subscription = cx.observe(&app, |_, _, cx| cx.notify());
        Self {
            app,
            editors: HashMap::new(),
            active_editor_id: None,
            _app_subscription: app_subscription,
        }
    }
}

impl Render for RemoteFileEditorWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.app.read(cx).transfer.editor_has_workspace() {
            self.app.update(cx, |app, cx| {
                app.transfer.clear_editor_window_tracking();
                cx.notify();
            });
            window.defer(cx, |window, _| window.remove_window());
            return div().size_full().into_any_element();
        }

        let (palette, font, font_size, title, active_tab, tab_ids) =
            self.app.read_with(cx, |app, _| {
                let workspace = app
                    .transfer
                    .editor_workspace()
                    .expect("editor checked above");
                let editor = workspace
                    .active_tab()
                    .expect("open editor workspace has an active tab");
                let name = if editor.name.trim().is_empty() {
                    &editor.remote_path
                } else {
                    &editor.name
                };
                (
                    app.theme_palette(),
                    app.gpui_ui_font().font(),
                    app.settings.summary().ui_font_size.clamp(12, 24) as f32,
                    format!(
                        "{}{}",
                        if workspace.tabs.iter().any(|tab| tab.dirty) {
                            "* "
                        } else {
                            ""
                        },
                        name
                    ),
                    editor.clone(),
                    workspace
                        .tabs
                        .iter()
                        .map(|tab| tab.id.clone())
                        .collect::<HashSet<_>>(),
                )
            });
        self.editors.retain(|tab_id, _| tab_ids.contains(tab_id));
        if !self.editors.contains_key(&active_tab.id) {
            let editor = cx.new(|cx| RemoteTextEditor::new(self.app.clone(), &active_tab, cx));
            self.editors.insert(active_tab.id.clone(), editor);
        }
        let editor = self
            .editors
            .get(&active_tab.id)
            .expect("active editor created above")
            .clone();
        editor.update(cx, |editor, cx| editor.sync_from_tab(&active_tab, cx));
        if self.active_editor_id.as_deref() != Some(active_tab.id.as_str()) {
            self.active_editor_id = Some(active_tab.id.clone());
            if active_tab.focused_field == crate::models::TransferEditorField::Content {
                window.focus(&editor.read(cx).focus_handle(), cx);
            }
        }
        window.set_window_title(&title);
        let cursor_position = editor.read(cx).cursor_position();
        let content = self.app.update(cx, |app, cx| {
            app.transfer_editor_window_view(editor, cursor_position, cx)
        });
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
                Some("icons/files.svg"),
                true,
                window.is_maximized(),
                move |_, window, cx| {
                    let should_close = close_app.update(cx, |app, cx| {
                        app.close_transfer_editor(cx);
                        !app.transfer.editor_has_workspace()
                    });
                    if should_close {
                        window.remove_window();
                    }
                },
            ))
            .child(div().flex_1().min_h_0().overflow_hidden().child(content))
            .into_any_element()
    }
}

impl NyaTermApp {
    pub(in crate::features) fn open_remote_file_editor_window(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.transfer.editor_window() {
            let app = cx.entity();
            cx.defer(move |cx| {
                if handle
                    .update(cx, |_, window, _| window.activate_window())
                    .is_err()
                {
                    app.update(cx, |app, cx| {
                        if app.transfer.clear_editor_window_if(handle) {
                            cx.notify();
                        }
                    });
                }
            });
            return;
        }
        if self.transfer.editor_window_open_is_pending() {
            return;
        }
        if !self.transfer.begin_editor_window_open() {
            return;
        }
        cx.notify();
        let app = cx.entity();
        cx.defer(move |cx| {
            let should_open = app.read(cx).transfer.editor_window_open_is_pending();
            if should_open {
                open_remote_file_editor_window_now_from_app(app, cx);
            }
        });
    }
}

fn open_remote_file_editor_window_now_from_app(app: Entity<NyaTermApp>, cx: &mut App) {
    if let Some(handle) = app.read(cx).transfer.editor_window() {
        let activate_result = handle.update(cx, |_, window, _| window.activate_window());
        app.update(cx, |app, cx| {
            app.transfer
                .finish_editor_window_activation(handle, activate_result.is_ok());
            cx.notify();
        });
        return;
    }
    if !app.read(cx).transfer.editor_has_workspace() {
        app.update(cx, |app, cx| {
            app.transfer.clear_editor_window_tracking();
            cx.notify();
        });
        return;
    }

    let title = t!("fileEditor.title").to_string();
    let bounds = Bounds::centered(None, size(px(980.), px(720.)), cx);
    let close_app = app.clone();
    let view_app = app.clone();
    let result: anyhow::Result<NyaWindowHandle> = cx.open_window(
        WindowOptions {
            titlebar: child_window_titlebar(title),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(640.), px(480.))),
            ..Default::default()
        },
        move |window, cx| {
            window.on_window_should_close(cx, move |_, cx| {
                close_app.update(cx, |app, cx| {
                    app.close_transfer_editor(cx);
                    let should_close = !app.transfer.editor_has_workspace();
                    if should_close {
                        app.transfer.clear_editor_window_tracking();
                    }
                    should_close
                })
            });
            let view = cx.new(|cx| RemoteFileEditorWindow::new(view_app, cx));
            cx.new(|cx| nya_root(view, window, cx))
        },
    );

    app.update(cx, |app, cx| match result {
        Ok(handle) => {
            app.transfer.finish_editor_window_open(handle);
            cx.notify();
        }
        Err(error) => {
            app.transfer.clear_editor_window_tracking();
            app.shell
                .set_status(format!("failed to open remote editor window: {error}"));
            cx.notify();
        }
    });
}
