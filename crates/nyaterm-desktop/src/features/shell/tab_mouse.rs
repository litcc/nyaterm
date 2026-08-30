use gpui::{
    ClickEvent, ClipboardItem, Context, FontWeight, InteractiveElement as _, IntoElement,
    MouseButton, MouseDownEvent, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _, px,
    rgb, rgba, svg,
};
use nyaterm_core::truncate_preview;

use crate::features::{NyaTermApp, formatting::short_id, view_widgets::mono_icon};

#[derive(Clone, Debug)]
pub(in crate::features) struct SessionTabDragPayload {
    pub session_id: String,
    pub display_name: String,
    pub kind_label: &'static str,
    pub kind_icon: &'static str,
    pub preview_background: u32,
    pub preview_border: u32,
    pub preview_text: u32,
    pub preview_text_muted: u32,
    pub preview_accent: u32,
}

pub(in crate::features) struct SessionTabDragPreview {
    payload: SessionTabDragPayload,
    position: gpui::Point<gpui::Pixels>,
}

impl SessionTabDragPreview {
    pub(in crate::features) fn new(
        payload: SessionTabDragPayload,
        position: gpui::Point<gpui::Pixels>,
    ) -> Self {
        Self { payload, position }
    }
}

impl Render for SessionTabDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .pl(self.position.x - px(94.))
            .pt(self.position.y - px(18.))
            .child(
                div()
                    .w(px(188.))
                    .h(px(36.))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_1()
                    .border_color(rgb(self.payload.preview_border))
                    .bg(rgb(self.payload.preview_background))
                    .shadow_lg()
                    .relative()
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .h(px(2.))
                            .bg(rgb(self.payload.preview_accent)),
                    )
                    .child(
                        svg()
                            .size(px(13.))
                            .path(self.payload.kind_icon)
                            .text_color(rgb(self.payload.preview_accent)),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight(700.))
                                    .text_color(rgb(self.payload.preview_text))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(truncate_preview(&self.payload.display_name, 28)),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(self.payload.preview_text_muted))
                                    .child(self.payload.kind_label),
                            ),
                    ),
            )
    }
}

/// Hover tooltip for session tabs (Tauri TabBar host / SSH address).
pub(in crate::features) struct SessionTabTooltip {
    pub title: String,
    pub lines: Vec<String>,
}

impl SessionTabTooltip {
    pub(in crate::features) fn new(title: String, lines: Vec<String>) -> Self {
        Self { title, lines }
    }
}

impl Render for SessionTabTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut body = div().flex().flex_col().gap_1().child(
            div()
                .text_size(px(12.))
                .font_weight(FontWeight(700.))
                .text_color(rgb(0xe5edf7))
                .child(self.title.clone()),
        );
        for line in &self.lines {
            let copyable = line_looks_copyable(line);
            let copy_value = line.clone();
            let row = div()
                .flex()
                .items_center()
                .gap_2()
                .min_w_0()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .text_size(px(11.))
                        .text_color(rgb(0x8f98aa))
                        .overflow_hidden()
                        .child(line.clone()),
                )
                .when(copyable, |this| {
                    this.child(
                        div()
                            .id(SharedString::from(format!(
                                "tab-tooltip-copy-{}",
                                copy_value.chars().take(24).collect::<String>()
                            )))
                            .size(px(18.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .text_size(px(10.))
                            .text_color(rgb(0x8f98aa))
                            .hover(|style| style.bg(rgb(0x334155)).text_color(rgb(0xe5edf7)))
                            .cursor_pointer()
                            .child(mono_icon("icons/copy.svg", rgb(0x8f98aa).into(), 11.))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.stop_propagation();
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    copy_value.clone(),
                                ));
                            })),
                    )
                });
            body = body.child(row);
        }
        div()
            .px_3()
            .py_2()
            .max_w(px(280.))
            .rounded_md()
            .border_1()
            .border_color(rgb(0x334155))
            .bg(rgba(0x151b24f2))
            .shadow_lg()
            .child(body)
    }
}

fn line_looks_copyable(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with("cwd ") || trimmed.starts_with("Disconnected") {
        return false;
    }
    // host, endpoint labels, ssh -p user@host
    true
}

#[derive(Debug, Clone, Copy)]
pub(in crate::features) enum TabMouseActionTarget {
    Double,
    Middle,
    Right,
}

impl NyaTermApp {
    pub(in crate::features) fn set_tab_mouse_action(
        &mut self,
        target: TabMouseActionTarget,
        action: &str,
        cx: &mut Context<Self>,
    ) {
        let action = normalize_tab_mouse_action(action);
        match target {
            TabMouseActionTarget::Double => {
                if self.settings.summary().interaction_tab_double_click_action == action {
                    return;
                }
                self.settings
                    .set_tab_double_click_action(action.to_string());
            }
            TabMouseActionTarget::Middle => {
                if self.settings.summary().interaction_tab_middle_click_action == action {
                    return;
                }
                self.settings
                    .set_tab_middle_click_action(action.to_string());
            }
            TabMouseActionTarget::Right => {
                if self.settings.summary().interaction_tab_right_click_action == action {
                    return;
                }
                self.settings.set_tab_right_click_action(action.to_string());
            }
        }
        self.save_interaction_settings(cx);
    }

    pub(in crate::features) fn handle_session_tab_click(
        &mut self,
        session_id: String,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let ClickEvent::Mouse(mouse) = event {
            let is_left_double_click = mouse.down.button == MouseButton::Left
                && mouse.up.button == MouseButton::Left
                && event.click_count() >= 2;
            if is_left_double_click {
                let action = self
                    .settings
                    .summary()
                    .interaction_tab_double_click_action
                    .clone();
                if action != "none" {
                    cx.stop_propagation();
                    self.run_tab_mouse_action(session_id, action, window, cx);
                    return;
                }
            }
        }

        self.select_session(session_id, cx);
    }

    pub(in crate::features) fn handle_session_tab_mouse_down(
        &mut self,
        session_id: String,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let action = match event.button {
            MouseButton::Middle => self
                .settings
                .summary()
                .interaction_tab_middle_click_action
                .clone(),
            MouseButton::Right => self
                .settings
                .summary()
                .interaction_tab_right_click_action
                .clone(),
            _ => return,
        };
        let enabled = self.tab_mouse_action_enabled(&session_id, &action);
        if event.button == MouseButton::Right && (action == "none" || !enabled) {
            cx.stop_propagation();
            self.open_tab_actions_at(
                session_id,
                Some((f32::from(event.position.x), f32::from(event.position.y))),
                window,
                cx,
            );
        } else if action != "none" && enabled {
            cx.stop_propagation();
            self.run_tab_mouse_action(session_id, action, window, cx);
        }
    }

    pub(in crate::features) fn reorder_session_relative(
        &mut self,
        dragged_session_id: String,
        target_session_id: String,
        insert_after: bool,
        cx: &mut Context<Self>,
    ) {
        if dragged_session_id == target_session_id {
            self.clear_session_tab_drag(cx);
            return;
        }
        let tabs = self.ordered_tab_sessions();
        if !tabs.iter().any(|session| session.id == target_session_id) {
            self.shell
                .set_status("drop target session no longer exists".to_string());
            self.clear_session_tab_drag(cx);
            return;
        }
        if !tabs.iter().any(|session| session.id == dragged_session_id) {
            self.shell
                .set_status("dragged session no longer exists".to_string());
            self.clear_session_tab_drag(cx);
            return;
        }
        let source_ids = self.tab_tree_session_ids(&dragged_session_id);
        let target_ids = self.tab_tree_session_ids(&target_session_id);
        self.session
            .move_session_group_relative(&source_ids, &target_ids, insert_after);
        self.session.clear_tab_drag();
        self.shell.request_session_tab_scroll_into_view();
        self.shell.set_status(format!(
            "moved tab {} {}",
            if insert_after { "after" } else { "before" },
            short_id(&target_session_id)
        ));
        if self.session.restore_is_complete() {
            self.persist_open_tabs();
        } else if self.terminal_windows_is_multi_leaf() {
            self.persist_terminal_window_layout();
        }
        cx.notify();
    }

    pub(in crate::features) fn reorder_session_to_end(
        &mut self,
        dragged_session_id: String,
        cx: &mut Context<Self>,
    ) {
        let sessions = self.ordered_tab_sessions();
        if !sessions
            .iter()
            .any(|session| session.id == dragged_session_id)
        {
            self.shell
                .set_status("dragged session no longer exists".to_string());
            self.session.clear_tab_drag();
            cx.notify();
            return;
        }
        let source_ids = self.tab_tree_session_ids(&dragged_session_id);
        self.session.move_session_group_to_end(&source_ids);
        self.session.clear_tab_drag();
        self.shell.request_session_tab_scroll_into_view();
        self.shell.set_status("moved tab to end".to_string());
        if self.session.restore_is_complete() {
            self.persist_open_tabs();
        } else if self.terminal_windows_is_multi_leaf() {
            self.persist_terminal_window_layout();
        }
        cx.notify();
    }

    pub(in crate::features) fn update_session_tab_drag(
        &mut self,
        source_id: String,
        target_id: String,
        insert_after: bool,
        cx: &mut Context<Self>,
    ) {
        if self
            .session
            .set_tab_drag_target(source_id, target_id, insert_after)
        {
            cx.notify();
        }
    }

    pub(in crate::features) fn clear_session_tab_drag(&mut self, cx: &mut Context<Self>) {
        if self.session.clear_tab_drag() {
            cx.notify();
        }
    }

    fn tab_mouse_action_enabled(&self, session_id: &str, action: &str) -> bool {
        let tab_root = self.tab_root_for_session(session_id);
        let pane_id = self.active_pane_for_tab_root(&tab_root);
        let policy = self.tab_action_policy_for(&pane_id, &tab_root);
        match action {
            "none" => false,
            "rename_tab" | "copy_tab_name" => true,
            "copy_server_ip" => policy.is_some_and(|policy| policy.support.copy_ssh_host),
            "duplicate_session" => policy.is_some_and(|policy| policy.availability.spawn_session),
            "multiplex_ssh" => policy.is_some_and(|policy| policy.availability.multiplex),
            "reconnect_session" => policy.is_some_and(|policy| policy.availability.reconnect),
            "disconnect_session" => policy.is_some_and(|policy| policy.availability.disconnect),
            "close_tab" => !self.tab_tree_is_locked(&tab_root),
            _ => false,
        }
    }

    fn run_tab_mouse_action(
        &mut self,
        session_id: String,
        action: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_root = self.tab_root_for_session(&session_id);
        if action == "close_tab" {
            self.close_session(tab_root, cx);
            return;
        }

        self.select_session(tab_root.clone(), cx);
        let Some(active_pane_id) = self.session.active_id_owned() else {
            return;
        };
        if self.tab_root_for_session(&active_pane_id) != tab_root {
            return;
        }

        match action.as_str() {
            "none" => {}
            "rename_tab" => self.open_rename_session(tab_root, window, cx),
            "copy_tab_name" => self.copy_session_name(&tab_root, cx),
            "copy_server_ip" => self.copy_active_session_ssh_host(cx),
            "duplicate_session" => self.duplicate_active_session(window, cx),
            "multiplex_ssh" => self.multiplex_active_ssh_session(window, cx),
            "reconnect_session" => self.reconnect_active_session(window, cx),
            "disconnect_session" => self.disconnect_session(active_pane_id, cx),
            _ => {
                self.shell
                    .set_status(format!("unknown tab action '{action}'"));
                cx.notify();
            }
        }
    }
}

pub(in crate::features) const TAB_MOUSE_ACTIONS: [&str; 9] = [
    "none",
    "rename_tab",
    "copy_tab_name",
    "copy_server_ip",
    "duplicate_session",
    "multiplex_ssh",
    "reconnect_session",
    "disconnect_session",
    "close_tab",
];

fn normalize_tab_mouse_action(action: &str) -> &'static str {
    TAB_MOUSE_ACTIONS
        .iter()
        .copied()
        .find(|item| *item == action)
        .unwrap_or("none")
}
