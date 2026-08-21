use gpui::{AnyElement, Context, IntoElement, SharedString, div, prelude::*, px, rgb};
use nyaterm_core::RiskLevel;
use nyaterm_ui::{NyaNumberInputOptions, NyaSelectOption};

use crate::features::{NyaTermApp, text_inputs::TextInputSetup};
use crate::models::AiInputField;
use crate::theme::ThemePalette;

use super::super::{
    settings_form_row, settings_form_section, settings_input_control, settings_switch,
};

impl NyaTermApp {
    pub(in crate::features) fn ai_input(
        &mut self,
        _id: &'static str,
        label: impl Into<SharedString>,
        value: String,
        field: AiInputField,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label: SharedString = label.into();
        let setup = if field == AiInputField::ApiKey {
            TextInputSetup::masked()
        } else {
            TextInputSetup::default()
        };
        self.text_input_field(
            format!("ai.input.{}", field.input_key()),
            label,
            &value,
            setup,
            cx,
        )
        .into_any_element()
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
        .map(|(value, label)| NyaSelectOption::new(value, self.tr(label)))
        .collect();

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(settings_form_section(
                palette,
                Some(self.tr("ai.general")),
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(settings_form_row(
                        palette,
                        self.tr("ai.enabled"),
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
                        self.tr("ai.redaction"),
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
                        self.tr("ai.allowSave"),
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
                        self.tr("ai.recordHistory"),
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
                        self.tr("ai.requestUserAgent"),
                        Some(SharedString::from(self.tr("ai.requestUserAgentDesc"))),
                        settings_input_control(
                            300.,
                            self.ai_input(
                                "ai-request-user-agent",
                                self.tr("ai.requestUserAgent"),
                                self.ai.settings_config().request_user_agent.clone(),
                                AiInputField::RequestUserAgent,
                                cx,
                            ),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        self.tr("ai.contextLineLimit"),
                        None,
                        self.number_input_box(
                            "ai.number.context-line-limit",
                            self.ai
                                .settings_config()
                                .context_line_limit
                                .to_string()
                                .as_str(),
                            NyaNumberInputOptions::default()
                                .range(50.0, 500.0)
                                .step(50.0),
                            cx,
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        self.tr("ai.timeoutMs"),
                        None,
                        self.number_input_box(
                            "ai.number.timeout-ms",
                            self.ai.settings_config().timeout_ms.to_string().as_str(),
                            NyaNumberInputOptions::default()
                                .range(5_000.0, 300_000.0)
                                .step(1_000.0),
                            cx,
                        ),
                    )),
            ))
            .child(settings_form_section(
                palette,
                Some(self.tr("ai.agentSettings")),
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(settings_form_row(
                        palette,
                        self.tr("ai.smartAutoExecuteMaxRisk"),
                        Some(SharedString::from(
                            self.tr("ai.smartAutoExecuteMaxRiskDesc"),
                        )),
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
                        self.tr("ai.agentMaxSteps"),
                        None,
                        self.number_input_box(
                            "ai.number.agent-steps",
                            self.ai
                                .settings_config()
                                .max_agent_steps
                                .unwrap_or(10)
                                .to_string()
                                .as_str(),
                            NyaNumberInputOptions::default().range(1.0, 50.0).step(1.0),
                            cx,
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        self.tr("ai.agentStepTimeout"),
                        None,
                        self.number_input_box(
                            "ai.number.agent-step-timeout-ms",
                            self.ai
                                .settings_config()
                                .agent_step_timeout_ms
                                .unwrap_or(30_000)
                                .to_string()
                                .as_str(),
                            NyaNumberInputOptions::default()
                                .range(5_000.0, 120_000.0)
                                .step(1_000.0),
                            cx,
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        self.tr("ai.terminalOutputLines"),
                        None,
                        self.number_input_box(
                            "ai.number.terminal-output-lines",
                            self.ai
                                .settings_config()
                                .terminal_output_lines
                                .to_string()
                                .as_str(),
                            NyaNumberInputOptions::default().range(0.0, 100.0).step(1.0),
                            cx,
                        ),
                    ))
                    .child(ai_help_text(palette, self.tr("ai.agentMaxStepsDesc")))
                    .child(ai_help_text(palette, self.tr("ai.terminalOutputLinesDesc"))),
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
