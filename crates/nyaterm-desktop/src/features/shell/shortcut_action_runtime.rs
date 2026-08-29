use gpui::{Context, Div, InteractiveElement, Stateful};

use crate::features::NyaTermApp;
use crate::models::{BottomPanelMode, NavItem, StartupCommandAction};
use crate::shortcuts::{
    CloseTab, CopySelectedConnections, DuplicateSession, DuplicateSessionWithCommand, LockScreen,
    ManageSyncGroups, MultiplexSsh, MultiplexSshWithCommand, NewLocalTerminal, NewSession, NextTab,
    OpenChat, OpenSettings, PreviousTab, QuickSwitch, RenameFile, ResetZoom, ShowAllCommands,
    ShowCommandSuggestions, SwitchToTab, TemporarySshLink, TerminalClear, TerminalCopy,
    TerminalFind, TerminalPaste, TerminalPasteSelected, TerminalSelectAll, ToggleLeftSidebar,
    ToggleRecording, ToggleRightSidebar, ZoomIn, ZoomOut,
};

impl NyaTermApp {
    pub(in crate::features) fn with_shortcut_action_handlers(
        &mut self,
        root: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        root.on_action(cx.listener(|this, _: &TerminalCopy, _, cx| {
            this.copy_terminal_selection_or_visible(cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &TerminalPaste, window, cx| {
            this.paste_from_clipboard(window, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &TerminalPasteSelected, window, cx| {
            if let Some(text) = this.selected_terminal_text() {
                this.paste_terminal_text(text, window, cx);
            }
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &TerminalFind, window, cx| {
            this.open_terminal_search(window, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &TerminalClear, _, cx| {
            this.clear_terminal(cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &TerminalSelectAll, _, cx| {
            this.select_all_terminal(cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &ManageSyncGroups, window, cx| {
            this.open_sync_groups(window, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &ShowCommandSuggestions, _, cx| {
            this.show_manual_command_suggestions(cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &ToggleRecording, _, cx| {
            this.toggle_active_session_recording(cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &NewSession, window, cx| {
            this.open_connection_editor(None, None, false, window, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &TemporarySshLink, window, cx| {
            this.open_temporary_ssh_link_dialog(window, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &QuickSwitch, window, cx| {
            this.open_quick_switch(window, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &NewLocalTerminal, window, cx| {
            this.start_local_session(window, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &CloseTab, _, cx| {
            this.close_active_session(cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &NextTab, _, cx| {
            this.select_relative_session(1, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &PreviousTab, _, cx| {
            this.select_relative_session(-1, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, action: &SwitchToTab, _, cx| {
            let index = if action.index == 9 {
                this.session.ordered_sessions().len().saturating_sub(1)
            } else {
                action.index.saturating_sub(1)
            };
            this.select_session_index(index, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &DuplicateSession, window, cx| {
            this.duplicate_active_session(window, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &MultiplexSsh, window, cx| {
            this.multiplex_active_ssh_session(window, cx);
            cx.stop_propagation();
        }))
        .on_action(
            cx.listener(|this, _: &DuplicateSessionWithCommand, window, cx| {
                this.open_startup_command_dialog(window, cx);
                cx.stop_propagation();
            }),
        )
        .on_action(
            cx.listener(|this, _: &MultiplexSshWithCommand, window, cx| {
                this.open_startup_command_dialog_for(StartupCommandAction::Multiplex, window, cx);
                cx.stop_propagation();
            }),
        )
        .on_action(cx.listener(|this, _: &ToggleLeftSidebar, _, cx| {
            this.toggle_left_sidebar(cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &ToggleRightSidebar, _, cx| {
            this.toggle_right_inspector(cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &ZoomIn, _, cx| {
            this.zoom_terminal_in(cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &ZoomOut, _, cx| {
            this.zoom_terminal_out(cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &ResetZoom, _, cx| {
            this.reset_terminal_font_size(cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &OpenSettings, _, cx| {
            this.open_page(NavItem::Settings, cx);
            this.shell.set_status("settings opened".to_string());
            cx.notify();
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &OpenChat, window, cx| {
            this.ensure_panel_open(NavItem::AiAssistant);
            window.focus(this.ai.chat_focus(), cx);
            this.shell.set_status("AI panel focused".to_string());
            cx.notify();
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &ShowAllCommands, _, cx| {
            this.set_bottom_panel_mode(BottomPanelMode::QuickCommands);
            this.shell.set_status("quick commands opened".to_string());
            cx.notify();
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &RenameFile, window, cx| {
            if this.selected_transfer_entries().len() == 1
                && this.session.active_ssh_file_browser_config().is_some()
                && !this.transfer.rename_dialog_is_open()
            {
                this.open_transfer_rename_dialog(window, cx);
            }
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &CopySelectedConnections, _, cx| {
            this.copy_selected_connections(cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(|this, _: &LockScreen, window, cx| {
            this.lock_app(window, cx);
            cx.stop_propagation();
        }))
    }
}
