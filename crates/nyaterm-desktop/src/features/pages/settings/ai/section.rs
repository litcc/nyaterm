use rust_i18n::t;

use gpui::{AnyElement, Context, IntoElement, SharedString, div, prelude::*, px, rgb};
use nyaterm_core::RiskLevel;
use nyaterm_ui::NyaSelectOption;

use crate::features::{pages::settings::panel::SettingsPanel, text_inputs::TextInputSetup};
use crate::models::AiInputField;
use crate::theme::ThemePalette;

use super::super::{
    settings_form_row, settings_form_section, settings_input_control, settings_switch,
};

impl SettingsPanel {
    pub(in crate::features) fn ai_input(
        &mut self,
        _id: &'static str,
        label: impl Into<SharedString>,
        value: String,
        field: AiInputField,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label: SharedString = label.into();
        let _ = (value, cx);
        self.existing_text_input_field(format!("ai.input.{}", field.input_key()), label, false)
    }

    /// Every AI settings input its section draws, with the value it seeds from.
    fn ai_input_specs(&self) -> Vec<(AiInputField, String)> {
        let config = self.ai.settings_config();
        vec![(
            AiInputField::RequestUserAgent,
            config.request_user_agent.clone(),
        )]
    }

    pub(in crate::features) fn ensure_ai_settings_inputs(&mut self, cx: &mut Context<Self>) {
        for (field, value) in self.ai_input_specs() {
            let setup = if field == AiInputField::ApiKey {
                TextInputSetup::masked()
            } else {
                TextInputSetup::default()
            };
            self.ensure_text_input(format!("ai.input.{}", field.input_key()), &value, setup, cx);
        }
    }

    pub(in crate::features) fn ai_settings_section(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let selected_risk =
            ai_risk_value(&self.ai.settings_config().agent_smart_auto_execute_max_risk);
        let risk_options = [
            ("low", "ai.riskLow"),
            ("medium", "ai.riskMedium"),
            ("high", "ai.riskHigh"),
            ("critical", "ai.riskCritical"),
        ]
        .into_iter()
        .map(|(value, label)| NyaSelectOption::new(value, t!(label)))
        .collect();

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(settings_form_section(
                palette,
                Some(t!("ai.general")),
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(settings_form_row(
                        palette,
                        t!("ai.enabled"),
                        None,
                        settings_switch(
                            palette,
                            "ai-enabled",
                            self.ai.settings_config().enabled,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_ai_enabled(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.redaction"),
                        None,
                        settings_switch(
                            palette,
                            "ai-redaction-toggle",
                            self.ai.settings_config().redaction_enabled,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_ai_redaction(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.allowSave"),
                        None,
                        settings_switch(
                            palette,
                            "ai-save-command-toggle",
                            self.ai.settings_config().allow_save_command,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_ai_allow_save_command(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.recordHistory"),
                        None,
                        settings_switch(
                            palette,
                            "ai-history-toggle",
                            self.ai.settings_config().record_history,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_ai_record_history(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.requestUserAgent"),
                        Some(SharedString::from(t!("ai.requestUserAgentDesc"))),
                        settings_input_control(
                            300.,
                            self.ai_input(
                                "ai-request-user-agent",
                                t!("ai.requestUserAgent"),
                                self.ai.settings_config().request_user_agent.clone(),
                                AiInputField::RequestUserAgent,
                                cx,
                            ),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.contextLineLimit"),
                        None,
                        self.existing_number_input_box("ai.number.context-line-limit"),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.timeoutMs"),
                        None,
                        self.existing_number_input_box("ai.number.timeout-ms"),
                    )),
            ))
            .child(settings_form_section(
                palette,
                Some(t!("ai.agentSettings")),
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(settings_form_row(
                        palette,
                        t!("ai.smartAutoExecuteMaxRisk"),
                        Some(SharedString::from(t!("ai.smartAutoExecuteMaxRiskDesc"))),
                        self.select_control(
                            "ai-smart-risk",
                            risk_options,
                            Some(selected_risk.to_string()),
                            false,
                            cx,
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.agentMaxSteps"),
                        None,
                        self.existing_number_input_box("ai.number.agent-steps"),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.agentStepTimeout"),
                        None,
                        self.existing_number_input_box("ai.number.agent-step-timeout-ms"),
                    ))
                    .child(settings_form_row(
                        palette,
                        t!("ai.terminalOutputLines"),
                        None,
                        self.existing_number_input_box("ai.number.terminal-output-lines"),
                    ))
                    .child(ai_help_text(palette, t!("ai.agentMaxStepsDesc")))
                    .child(ai_help_text(palette, t!("ai.terminalOutputLinesDesc"))),
            ))
    }
}

fn ai_help_text(palette: ThemePalette, text: impl Into<SharedString>) -> impl IntoElement {
    let text: SharedString = text.into();
    div()
        .text_size(px(11.))
        .text_color(rgb(palette.text_muted))
        .child(text)
}

fn ai_risk_value(risk: &RiskLevel) -> &'static str {
    match risk {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
        RiskLevel::Critical => "critical",
    }
}
