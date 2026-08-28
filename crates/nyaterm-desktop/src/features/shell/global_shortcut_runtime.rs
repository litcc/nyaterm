use gpui::{Context, KeyDownEvent, Window};

use crate::features::NyaTermApp;
use crate::models::{BottomPanelMode, NavItem, StartupCommandAction};
use crate::shortcuts::shortcut_matches;

impl NyaTermApp {
    pub(in crate::features) fn handle_global_shortcut(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.security.screen_locked() {
            return false;
        }

        if event.keystroke.key.as_str() == "escape" {
            if self.translation.dialog_is_open() {
                self.close_translation_dialog(window, cx);
                return true;
            }
        }

        // Dismiss strip menus with Escape (Tauri dropdown dismiss).
        if event.keystroke.key.as_str() == "escape"
            && (self.shell.chrome.open_tabs_menu_open || self.shell.chrome.new_session_menu_open)
        {
            self.close_open_tabs_menu(cx);
            self.close_new_session_menu(cx);
            return true;
        }

        if event.keystroke.key.as_str() == "escape" && self.close_last_floating_panel(cx) {
            return true;
        }

        let keybindings = self.settings.summary().keybindings.clone();
        if shortcut_matches(event, "terminal.copy", &keybindings) {
            self.copy_terminal_selection_or_visible(cx);
            return true;
        }
        if shortcut_matches(event, "terminal.paste", &keybindings) {
            self.paste_from_clipboard(window, cx);
            return true;
        }
        if shortcut_matches(event, "terminal.pasteSelected", &keybindings)
            && let Some(text) = self.selected_terminal_text()
        {
            self.paste_terminal_text(text, window, cx);
            return true;
        }
        if shortcut_matches(event, "terminal.find", &keybindings) {
            self.open_terminal_search(window, cx);
            return true;
        }
        if shortcut_matches(event, "terminal.clear", &keybindings) {
            self.clear_terminal(cx);
            return true;
        }
        if shortcut_matches(event, "terminal.manageSyncGroups", &keybindings) {
            self.open_sync_groups(window, cx);
            return true;
        }
        if shortcut_matches(event, "terminal.selectAll", &keybindings) {
            self.select_all_terminal(cx);
            return true;
        }
        if shortcut_matches(event, "terminal.recording.toggle", &keybindings) {
            self.toggle_active_session_recording(cx);
            return true;
        }
        if shortcut_matches(event, "tab.newSession", &keybindings) {
            self.open_connection_editor(None, None, false, window, cx);
            return true;
        }
        if shortcut_matches(event, "tab.temporarySshLink", &keybindings) {
            self.open_temporary_ssh_link_dialog(window, cx);
            return true;
        }
        if shortcut_matches(event, "tab.quickSwitch", &keybindings) {
            self.open_quick_switch(window, cx);
            return true;
        }
        if shortcut_matches(event, "tab.newLocalTerminal", &keybindings) {
            self.start_local_session(window, cx);
            return true;
        }
        if shortcut_matches(event, "tab.close", &keybindings) {
            self.close_active_session(cx);
            return true;
        }
        if shortcut_matches(event, "tab.next", &keybindings) {
            self.select_relative_session(1, cx);
            return true;
        }
        if shortcut_matches(event, "tab.prev", &keybindings) {
            self.select_relative_session(-1, cx);
            return true;
        }
        if shortcut_matches(event, "tab.switchTo", &keybindings) {
            let key = event
                .keystroke
                .key_char
                .as_deref()
                .unwrap_or(event.keystroke.key.as_str());
            if let Ok(tab_number) = key.parse::<usize>() {
                if tab_number == 9 {
                    let last_index = self.session.ordered_sessions().len().saturating_sub(1);
                    self.select_session_index(last_index, cx);
                } else {
                    self.select_session_index(tab_number.saturating_sub(1), cx);
                }
                return true;
            }
        }
        if shortcut_matches(event, "tab.duplicateSession", &keybindings) {
            self.duplicate_active_session(window, cx);
            return true;
        }
        if shortcut_matches(event, "tab.multiplexSsh", &keybindings) {
            self.multiplex_active_ssh_session(window, cx);
            return true;
        }
        if shortcut_matches(event, "tab.duplicateSessionWithCommand", &keybindings) {
            self.open_startup_command_dialog(window, cx);
            return true;
        }
        if shortcut_matches(event, "tab.multiplexSshWithCommand", &keybindings) {
            self.open_startup_command_dialog_for(StartupCommandAction::Multiplex, window, cx);
            return true;
        }
        if shortcut_matches(event, "view.openSettings", &keybindings) {
            self.open_page(NavItem::Settings, cx);
            self.shell.set_status("settings opened".to_string());
            cx.notify();
            return true;
        }
        if shortcut_matches(event, "view.openChat", &keybindings) {
            self.ensure_panel_open(NavItem::AiAssistant);
            window.focus(self.ai.chat_focus(), cx);
            self.shell.set_status("AI panel focused".to_string());
            cx.notify();
            return true;
        }
        if shortcut_matches(event, "view.showAllCommands", &keybindings) {
            self.set_bottom_panel_mode(BottomPanelMode::QuickCommands);
            self.shell.set_status("quick commands opened".to_string());
            cx.notify();
            return true;
        }
        if shortcut_matches(event, "savedConnections.copySelected", &keybindings) {
            self.copy_selected_connections(cx);
            return true;
        }
        if shortcut_matches(event, "fileExplorer.rename", &keybindings) {
            self.open_transfer_rename_dialog(window, cx);
            return true;
        }
        if shortcut_matches(event, "view.toggleLeftSidebar", &keybindings) {
            self.toggle_left_sidebar(cx);
            return true;
        }
        if shortcut_matches(event, "view.toggleRightSidebar", &keybindings) {
            self.toggle_right_inspector(cx);
            return true;
        }
        if shortcut_matches(event, "view.zoomIn", &keybindings) {
            self.zoom_terminal_in(cx);
            return true;
        }
        if shortcut_matches(event, "view.zoomOut", &keybindings) {
            self.zoom_terminal_out(cx);
            return true;
        }
        if shortcut_matches(event, "view.resetZoom", &keybindings) {
            self.reset_terminal_font_size(cx);
            return true;
        }
        if shortcut_matches(event, "special.lockScreen", &keybindings) {
            self.lock_app(window, cx);
            return true;
        }

        false
    }
}
