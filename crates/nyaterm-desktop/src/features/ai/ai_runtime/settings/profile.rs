use gpui::Context;
use nyaterm_core::{AgentCommandExecutionMode, AiMode, RiskLevel};

use crate::features::NyaTermApp;

impl NyaTermApp {
    pub(in crate::features) fn toggle_ai_enabled(&mut self, cx: &mut Context<Self>) {
        self.ai.toggle_settings_enabled();
        self.persist_ai_settings_now(cx);
    }

    pub(in crate::features) fn set_ai_mode(&mut self, mode: AiMode, cx: &mut Context<Self>) {
        self.ai.set_settings_mode(mode);
        self.persist_ai_settings_now(cx);
    }

    pub(in crate::features) fn set_ai_command_mode(
        &mut self,
        mode: AgentCommandExecutionMode,
        cx: &mut Context<Self>,
    ) {
        let before = self.ai_header_presentation();
        self.ai.set_settings_command_mode(mode);
        self.persist_ai_settings_now(cx);
        self.notify_root_if_ai_header_changed(before, cx);
    }

    pub(in crate::features) fn toggle_ai_background_execution(&mut self, cx: &mut Context<Self>) {
        self.ai.toggle_settings_background_execution();
        self.persist_ai_settings_now(cx);
    }

    pub(in crate::features) fn toggle_ai_redaction(&mut self, cx: &mut Context<Self>) {
        self.ai.toggle_settings_redaction();
        self.persist_ai_settings_now(cx);
    }

    pub(in crate::features) fn toggle_ai_allow_save_command(&mut self, cx: &mut Context<Self>) {
        self.ai.toggle_settings_allow_save_command();
        self.persist_ai_settings_now(cx);
    }

    pub(in crate::features) fn toggle_ai_record_history(&mut self, cx: &mut Context<Self>) {
        self.ai.toggle_settings_record_history();
        self.persist_ai_settings_now(cx);
    }

    pub(in crate::features) fn set_ai_context_line_limit(
        &mut self,
        value: u32,
        cx: &mut Context<Self>,
    ) {
        self.ai.set_settings_context_line_limit(value);
        self.persist_ai_settings_now(cx);
    }

    pub(in crate::features) fn set_ai_timeout_ms(&mut self, value: u64, cx: &mut Context<Self>) {
        self.ai.set_settings_timeout_ms(value);
        self.persist_ai_settings_now(cx);
    }

    pub(in crate::features) fn set_ai_agent_steps(&mut self, value: u16, cx: &mut Context<Self>) {
        self.ai.set_settings_agent_steps(value);
        self.persist_ai_settings_now(cx);
    }

    pub(in crate::features) fn set_ai_agent_step_timeout_ms(
        &mut self,
        value: u64,
        cx: &mut Context<Self>,
    ) {
        self.ai.set_settings_agent_step_timeout_ms(value);
        self.persist_ai_settings_now(cx);
    }

    pub(in crate::features) fn set_ai_terminal_output_lines(
        &mut self,
        value: u16,
        cx: &mut Context<Self>,
    ) {
        self.ai.set_settings_terminal_output_lines(value);
        self.persist_ai_settings_now(cx);
    }

    pub(in crate::features) fn set_ai_file_size_mb(&mut self, value: u64, cx: &mut Context<Self>) {
        self.ai.set_settings_file_size_mb(value);
        self.persist_ai_settings_now(cx);
    }

    pub(in crate::features) fn update_ai_smart_auto_execute_max_risk(
        &mut self,
        risk: RiskLevel,
        cx: &mut Context<Self>,
    ) {
        self.ai.set_settings_smart_auto_execute_max_risk(risk);
        self.persist_ai_settings_now(cx);
    }
}
