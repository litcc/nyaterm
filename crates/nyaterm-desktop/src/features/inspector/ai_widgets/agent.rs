use rust_i18n::t;

use gpui::{Context, FontWeight, IntoElement, SharedString, div, prelude::*, px, rgb, svg};
use nyaterm_core::truncate_preview;

use crate::features::NyaTermApp;
use crate::features::formatting::ai_agent_step_status_style;
use crate::features::runtime_jobs::{AiAgentStepStatus, AiAgentStepView};
use crate::features::shell::gpui_code_font_family;
use crate::features::view_widgets::markdown_content_view;
use crate::widgets::status_pill;

impl NyaTermApp {
    pub(in crate::features) fn ai_agent_step_card(
        &self,
        step: AiAgentStepView,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        // Tauri AgentStepView: collapsible thought, left-accent command shell, optional output.
        let (label, fg, bg) = ai_agent_step_status_style(step.status);
        let border = match step.status {
            AiAgentStepStatus::Completed => rgb(palette.success),
            AiAgentStepStatus::Failed | AiAgentStepStatus::Cancelled => rgb(palette.danger),
            AiAgentStepStatus::Running | AiAgentStepStatus::Tool => rgb(palette.link),
            AiAgentStepStatus::NeedsApproval => rgb(palette.warning),
            AiAgentStepStatus::Planning => rgb(palette.text_muted),
        };
        let step_index = step.step_index;
        let thought_open = self.ai.agent_thought_is_expanded(step_index);
        let output_open = self.ai.agent_output_is_expanded(step_index);
        let thought = step
            .thought
            .clone()
            .filter(|value| !value.trim().is_empty());
        let command = step
            .command
            .clone()
            .or_else(|| {
                if step.detail.trim().is_empty()
                    || thought.as_ref().is_some_and(|t| t == &step.detail)
                {
                    None
                } else {
                    Some(step.detail.clone())
                }
            })
            .filter(|value| !value.trim().is_empty());
        let observation = step
            .observation
            .clone()
            .filter(|value| !value.trim().is_empty());
        let has_thought = thought.is_some();
        let thought_label = if has_thought {
            if thought_open {
                "Hide thought"
            } else {
                "Show thought"
            }
        } else if matches!(
            step.status,
            AiAgentStepStatus::Completed | AiAgentStepStatus::Planning
        ) {
            "Step"
        } else {
            ""
        };

        let mut card = div()
            .id(SharedString::from(format!("ai-agent-step-{step_index}")))
            .flex()
            .flex_col()
            .gap_1()
            .pb_2()
            .child(
                div()
                    .id(SharedString::from(format!(
                        "ai-agent-step-thought-toggle-{step_index}"
                    )))
                    .flex()
                    .items_center()
                    .gap_1()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_ai_agent_thought_expanded(step_index, cx);
                    }))
                    .child(
                        svg()
                            .size(px(13.))
                            .flex_none()
                            .path(if thought_open {
                                "icons/chevron-down.svg"
                            } else {
                                "icons/fe/forward.svg"
                            })
                            .text_color(rgb(palette.text_muted)),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_weight(FontWeight(700.))
                            .text_color(rgb(palette.text))
                            .child(format!("#{}", step.step_index.saturating_add(1))),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .text_size(px(11.))
                            .text_color(rgb(palette.text_muted))
                            .overflow_hidden()
                            .child(if thought_label.is_empty() {
                                truncate_preview(&step.title, 36)
                            } else {
                                format!("{} · {}", thought_label, truncate_preview(&step.title, 28))
                            }),
                    )
                    .child(status_pill(label, rgb(fg), rgb(bg))),
            );

        if thought_open && let Some(thought_text) = thought.clone() {
            card = card.child(
                div()
                    .ml_4()
                    .text_size(px(11.))
                    .text_color(rgb(palette.text_muted))
                    .line_height(px(16.))
                    .child(markdown_content_view(
                        palette,
                        &truncate_preview(&thought_text, 800),
                    )),
            );
        }

        if let Some(command_text) = command {
            let mut shell = div()
                .ml_1()
                .rounded_md()
                .border_1()
                .border_color(rgb(palette.border))
                .border_l_2()
                .border_color(border)
                .bg(rgb(palette.bg))
                .overflow_hidden()
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .border_b_1()
                        .border_color(rgb(palette.surface_elevated))
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(10.))
                                .font_weight(FontWeight(700.))
                                .text_color(rgb(palette.text_muted))
                                .child("SHELL"),
                        )
                        .child(
                            div()
                                .ml_auto()
                                .text_size(px(10.))
                                .text_color(rgb(palette.text_dimmed))
                                .child(truncate_preview(&step.title, 24)),
                        ),
                )
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .font_family(gpui_code_font_family())
                        .text_size(px(11.))
                        .text_color(rgb(palette.text))
                        .line_height(px(16.))
                        .child(truncate_preview(&command_text, 600)),
                );

            if let Some(obs) = observation.clone() {
                shell = shell.child(
                    div()
                        .id(SharedString::from(format!(
                            "ai-agent-step-output-toggle-{step_index}"
                        )))
                        .px_2()
                        .py_1()
                        .border_t_1()
                        .border_color(rgb(palette.surface_elevated))
                        .flex()
                        .items_center()
                        .gap_1()
                        .cursor_pointer()
                        .hover(|this| this.bg(rgb(palette.surface)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_ai_agent_output_expanded(step_index, cx);
                        }))
                        .child(
                            svg()
                                .size(px(13.))
                                .flex_none()
                                .path(if output_open {
                                    "icons/chevron-down.svg"
                                } else {
                                    "icons/fe/forward.svg"
                                })
                                .text_color(rgb(palette.text_muted)),
                        )
                        .child(
                            div()
                                .text_size(px(10.))
                                .text_color(rgb(palette.text_muted))
                                .child(if output_open {
                                    "Hide output"
                                } else {
                                    "Show output"
                                }),
                        ),
                );
                if output_open {
                    shell = shell.child(
                        div()
                            .px_2()
                            .py_1()
                            .max_h(px(120.))
                            .overflow_hidden()
                            .font_family(gpui_code_font_family())
                            .text_size(px(10.))
                            .text_color(rgb(palette.text_muted))
                            .line_height(px(14.))
                            .child(truncate_preview(&obs, 1200)),
                    );
                }
            } else if matches!(
                step.status,
                AiAgentStepStatus::Running | AiAgentStepStatus::Tool
            ) {
                shell = shell.child(
                    div()
                        .px_2()
                        .py_1()
                        .border_t_1()
                        .border_color(rgb(palette.surface_elevated))
                        .text_size(px(10.))
                        .text_color(rgb(palette.link))
                        .child(t!("ai.agentExecuting")),
                );
            }

            card = card.child(shell);
        } else if let Some(obs) = observation {
            card = card.child(
                div()
                    .ml_4()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(rgb(palette.bg))
                    .px_2()
                    .py_1()
                    .font_family(gpui_code_font_family())
                    .text_size(px(10.))
                    .text_color(rgb(palette.text_muted))
                    .child(truncate_preview(&obs, 400)),
            );
        }

        card
    }
}
