use gpui::{
    Context, FontWeight, IntoElement, KeyDownEvent, MouseButton, SharedString, div, prelude::*, px,
    rgb, rgba, svg,
};
use nyaterm_core::{AiAction, AiMode, truncate_preview};
use nyaterm_ui::NyaScrollable;

use crate::features::formatting::{session_kind_label, short_id};
use crate::features::{NyaTermApp, text_inputs::TextInputSetup};
use crate::models::{AiDetectedErrorState, AiPreparedRequest, NavItem, SettingsTab};
use crate::widgets::{mode_button, small_button};

impl NyaTermApp {
    pub(in crate::features) fn dismiss_ai_detected_error(&mut self, cx: &mut Context<Self>) {
        self.ai.dismiss_detected_error();
        cx.notify();
    }

    pub(in crate::features) fn analyze_ai_detected_error(
        &mut self,
        detected: AiDetectedErrorState,
        cx: &mut Context<Self>,
    ) {
        if self.ai.chat_or_agent_is_running() {
            self.ai
                .set_chat_response_preview("AI request already running");
            self.ai.set_panel_status("AI request already running");
            cx.notify();
            return;
        }
        let mut context = self.ai_terminal_context_for_session(Some(&detected.session_id));
        context.selected_text = detected.output.clone();
        let request = AiPreparedRequest {
            action: AiAction::AnalyzeError,
            context,
            source_label: "Detected terminal error".to_string(),
        };
        self.ai
            .prepare_detected_error_request(request, detected.session_id.clone());
        self.set_ai_prompt_draft("Analyze detected error", cx);
        self.start_ai_ask(cx);
    }

    pub(in crate::features) fn ai_detected_error_banner(
        &mut self,
        detected: AiDetectedErrorState,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let analyze_state = detected.clone();
        div()
            .flex_none()
            .border_b_1()
            .border_color(rgb(palette.border))
            .bg(rgba(0xf59e0b1a))
            .px_3()
            .py_2()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_0()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight(700.))
                            .text_color(rgb(0xd97706))
                            .child(self.tr("ai.errorDetected")),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(palette.text_muted))
                            .child(format!("session {}", short_id(&detected.session_id))),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(small_button(
                        palette,
                        "ai-detected-error-analyze",
                        "Analyze",
                        cx.listener(move |this, _, _, cx| {
                            this.analyze_ai_detected_error(analyze_state.clone(), cx);
                        }),
                    ))
                    .child(small_button(
                        palette,
                        "ai-detected-error-close",
                        "Close",
                        cx.listener(|this, _, _, cx| {
                            this.dismiss_ai_detected_error(cx);
                        }),
                    )),
            )
    }

    pub(in crate::features) fn ai_ask_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.theme_palette();
        let agent_mode = self.ai.settings_config().default_mode == AiMode::Agent;
        let file_action_ready = self
            .ai
            .chat_prepared_request()
            .is_some_and(|request| request.action == AiAction::CustomFileAction);
        let ai_running = self.ai.chat_or_agent_is_running();
        let command_rows = self.ai_command_card_list(cx);
        let agent_steps = self.ai.agent_steps().to_vec();
        let mut agent_step_rows = div();
        if agent_mode || !agent_steps.is_empty() {
            agent_step_rows = agent_step_rows
                .mt_2()
                .border_t_1()
                .border_color(rgb(palette.border))
                .pt_2()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_size(px(10.))
                        .font_weight(FontWeight(700.))
                        .text_color(rgb(palette.text_muted))
                        .child(self.tr("ai.agentSteps")),
                );
            if agent_steps.is_empty() {
                agent_step_rows = agent_step_rows.child(
                    div()
                        .text_xs()
                        .text_color(rgb(palette.text_dimmed))
                        .child(self.tr("ai.agentNoSteps")),
                );
            } else {
                for step in agent_steps.into_iter().rev().take(16).rev() {
                    agent_step_rows = agent_step_rows.child(self.ai_agent_step_card(step, cx));
                }
            }
        }
        let selected_model_id = self.ai_selected_model_id();
        let enabled_models = self.ai_enabled_models();
        let selected_model = selected_model_id.as_deref().and_then(|model_id| {
            enabled_models
                .iter()
                .find(|model| model.id == model_id)
                .cloned()
        });
        let model_choices = self.ai_filtered_model_choices();
        self.ai.clamp_discovery_index(model_choices.len());
        let model_label = selected_model
            .as_ref()
            .map(|model| model.name.clone())
            .unwrap_or_else(|| self.tr("ai.notConfigured").to_string());
        let target_session_ids = self.ai.chat_target_session_ids().to_vec();
        let target_sessions = target_session_ids
            .iter()
            .filter_map(|session_id| {
                self.session.session_info(session_id).map(|session| {
                    (
                        session_id.clone(),
                        self.session.display_name_by_info(&session),
                    )
                })
            })
            .collect::<Vec<_>>();
        let mention_candidates = if self.ai.chat_mention_is_open() {
            self.ai_mention_candidates()
                .into_iter()
                .map(|session| {
                    let label = self.session.display_name_by_info(&session);
                    let kind = session_kind_label(session.kind).to_string();
                    let selected = target_session_ids
                        .iter()
                        .any(|session_id| session_id == &session.id);
                    (session, label, kind, selected)
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        self.ai.clamp_chat_mention_index(mention_candidates.len());
        let mode_label = if agent_mode { "Agent" } else { "Ask" };
        let enabled = self.ai.settings_config().enabled;
        let composer_disabled = ai_running || !enabled;
        let send_disabled = !ai_running
            && (!enabled
                || selected_model.is_none()
                || self.ai.chat_prompt_draft().trim().is_empty());
        // Built before the panel, which reads `self` throughout: creating a box
        // needs it mutably. The prompt wraps, the way Tauri's composer does.
        let prompt_placeholder = if !enabled {
            "Go to Settings to enable AI"
        } else if agent_mode {
            "Describe a task for the agent…"
        } else {
            "Ask about the terminal or generate a command…"
        };
        let prompt_draft = self.ai.chat_prompt_draft().to_string();
        let model_query = self.ai.discovery_query().to_string();
        let history_open = self.ai.history_is_open();
        let execution_menu_open = self.ai.panel_execution_menu_is_open();
        let message_menu_open = self.ai.chat_message_menu().is_some();
        let detected_error = self.ai.panel_detected_error().cloned();
        let quoted_text = self.ai.chat_quote().map(str::to_string);
        let mention_open = self.ai.chat_mention_is_open();
        let mention_index = self.ai.chat_mention_index();
        let discovery_menu_open = self.ai.discovery_menu_is_open();
        let discovery_index = self.ai.discovery_index();
        let prompt_input = self
            .text_input_box(
                "ai.chat.prompt",
                &prompt_draft,
                TextInputSetup::multi_line(prompt_placeholder),
                cx,
            )
            .into_any_element();
        let model_search_input = self
            .search_input_box(
                "ai.model-search",
                &model_query,
                TextInputSetup::placeholder("Search models"),
                cx,
            )
            .into_any_element();

        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(self.shell_transparent_color(palette.surface))
            .relative()
            .when(history_open, |this| {
                this.child(self.ai_history_popover(cx))
            })
            .when(execution_menu_open, |this| {
                this.child(self.ai_execution_mode_menu(cx))
            })
            .when(message_menu_open, |this| {
                this.child(self.ai_message_context_menu_overlay(cx))
            })
            .when_some(detected_error, |this, detected| {
                this.child(self.ai_detected_error_banner(detected, cx))
            })
            .child(
                div()
                    .id(SharedString::from("ai-transcript-scroll"))
                    .flex_1()
                    .min_h_0()
                    .overflow_scrollbar()
                    .px_3()
                    .py_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(self.ai_transcript_body(
                        mode_label,
                        enabled,
                        agent_step_rows,
                        command_rows,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_none()
                    .border_t_1()
                    .border_color(rgb(palette.border))
                    .bg(self.shell_transparent_color(palette.section_header))
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .when_some(quoted_text, |this, quoted_text| {
                        let preview = truncate_preview(quoted_text.trim(), 140);
                        this.child(
                            div()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(palette.link))
                                .bg(rgb(palette.hover))
                                .flex()
                                .items_center()
                                .gap_2()
                                .overflow_hidden()
                                .child(
                                    div()
                                        .w(px(3.))
                                        .h(px(28.))
                                        .flex_none()
                                        .bg(rgb(palette.link)),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .text_size(px(11.))
                                        .text_color(rgb(palette.link))
                                        .child(self.tr("ai.quote")),
                                )
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex_1()
                                        .py_1()
                                        .text_size(px(11.))
                                        .text_color(rgb(palette.text_muted))
                                        .overflow_hidden()
                                        .child(preview),
                                )
                                .child(
                                    div()
                                        .id(SharedString::from("ai-quote-clear"))
                                        .size(px(20.))
                                        .mr_1()
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded_sm()
                                        .text_size(px(12.))
                                        .text_color(rgb(palette.text_muted))
                                        .cursor_pointer()
                                        .hover(|this| {
                                            this.bg(rgb(palette.surface_elevated))
                                                .text_color(rgb(palette.text))
                                        })
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.clear_ai_quote(cx);
                                        }))
                                        .child(
                                            svg()
                                                .size(px(13.))
                                                .path("icons/window/close.svg")
                                                .text_color(rgb(palette.text_muted)),
                                        ),
                                ),
                        )
                    })
                    .when(!target_sessions.is_empty(), |this| {
                        let mut target_row = div().flex().flex_wrap().items_center().gap_1().child(
                            div()
                                .text_size(px(10.))
                                .font_weight(FontWeight(600.))
                                .text_color(rgb(palette.text_muted))
                                .child(format!("{}:", self.tr("ai.targetSession"))),
                        );
                        for (session_id, target_label) in target_sessions {
                            let remove_id = session_id.clone();
                            target_row = target_row.child(
                                div()
                                    .min_w_0()
                                    .max_w(px(220.))
                                    .h(px(20.))
                                    .px_2()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .rounded_full()
                                    .border_1()
                                    .border_color(rgb(palette.link))
                                    .bg(rgb(palette.hover))
                                    .text_size(px(10.))
                                    .font_weight(FontWeight(600.))
                                    .text_color(rgb(palette.link))
                                    .child(
                                        div()
                                            .size(px(6.))
                                            .rounded_full()
                                            .flex_none()
                                            .bg(rgb(palette.link)),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .child(truncate_preview(&target_label, 32)),
                                    )
                                    .child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "ai-target-remove-{session_id}"
                                            )))
                                            .size(px(14.))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_full()
                                            .text_size(px(10.))
                                            .cursor_pointer()
                                            .hover(|this| {
                                                this.bg(rgb(palette.surface_elevated))
                                                    .text_color(rgb(palette.danger))
                                            })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.remove_ai_target_session(
                                                    remove_id.clone(),
                                                    cx,
                                                );
                                            }))
                                            .child(
                                                svg()
                                                    .size(px(11.))
                                                    .path("icons/window/close.svg")
                                                    .text_color(rgb(palette.text_muted)),
                                            ),
                                    ),
                            );
                        }
                        this.child(target_row)
                    })
                    .when(mention_open, |this| {
                        let selected_index = mention_index;
                        let mut popover = div()
                            .max_h(px(192.))
                            .overflow_hidden()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(palette.border))
                            .bg(self.shell_surface_color(palette.surface))
                            .shadow_lg()
                            .flex()
                            .flex_col()
                            .p_1();
                        if mention_candidates.is_empty() {
                            popover = popover.child(
                                div()
                                    .h(px(44.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_xs()
                                    .text_color(rgb(palette.text_muted))
                                    .child(self.tr("ai.noSessions")),
                            );
                        } else {
                            for (index, (session, label, kind, selected)) in
                                mention_candidates.into_iter().enumerate().take(8)
                            {
                                let focused = index == selected_index;
                                popover = popover.child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "ai-mention-session-{}",
                                            session.id
                                        )))
                                        .h(px(30.))
                                        .px_2()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .rounded_sm()
                                        .bg(if focused || selected {
                                            rgb(palette.hover)
                                        } else {
                                            rgba(0x00000000)
                                        })
                                        .cursor_pointer()
                                        .hover(|this| this.bg(rgb(palette.hover)))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.ai.set_chat_mention_index(index);
                                            this.select_ai_mention_candidate(cx);
                                        }))
                                        .child(div().size(px(7.)).rounded_full().flex_none().bg(
                                            if selected {
                                                rgb(palette.link)
                                            } else {
                                                rgb(palette.text_dimmed)
                                            },
                                        ))
                                        .child(
                                            div()
                                                .min_w_0()
                                                .flex_1()
                                                .overflow_hidden()
                                                .text_size(px(11.))
                                                .font_weight(FontWeight(600.))
                                                .text_color(rgb(palette.text))
                                                .child(truncate_preview(&label, 34)),
                                        )
                                        .child(
                                            div()
                                                .flex_none()
                                                .text_size(px(10.))
                                                .text_color(rgb(palette.text_muted))
                                                .child(kind),
                                        ),
                                );
                            }
                        }
                        this.child(popover)
                    })
                    .child(
                        div()
                            // A flex row, so the box has a width to fill: the
                            // composer's own children stretch, a block's do not.
                            .w_full()
                            .flex()
                            .when(composer_disabled, |this| this.opacity(0.56))
                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                                this.handle_ai_prompt_key_down(event, cx);
                            }))
                            .child(div().min_w_0().flex_1().child(prompt_input)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .h(px(28.))
                                            .flex()
                                            .items_center()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(rgb(palette.border))
                                            .bg(rgb(palette.input))
                                            .p(px(1.))
                                            .gap_0()
                                            .child(mode_button(
                                                "ai-mode-ask",
                                                "Ask",
                                                !agent_mode,
                                                self.theme_palette(),
                                                cx.listener(|this, _, _, cx| {
                                                    this.set_ai_mode(AiMode::Ask, cx);
                                                }),
                                            ))
                                            .child(mode_button(
                                                "ai-mode-agent",
                                                "Agent",
                                                agent_mode,
                                                self.theme_palette(),
                                                cx.listener(|this, _, _, cx| {
                                                    this.set_ai_mode(AiMode::Agent, cx);
                                                }),
                                            )),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex_1()
                                            .relative()
                                            .child(
                                                div()
                                                    .id(SharedString::from("ai-model-selector"))
                                                    .min_w_0()
                                                    .h(px(28.))
                                                    .px_2()
                                                    .flex()
                                                    .items_center()
                                                    .justify_between()
                                                    .gap_2()
                                                    .rounded_md()
                                                    .border_1()
                                                    .border_color(rgb(palette.border))
                                                    .bg(rgb(palette.input))
                                                    .text_size(px(11.))
                                                    .text_color(rgb(palette.text_muted))
                                                    .overflow_hidden()
                                                    .cursor_pointer()
                                                    .hover(|this| {
                                                        this.border_color(rgb(palette.link))
                                                            .text_color(rgb(palette.text))
                                                    })
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        let selected_index =
                                                            this.ai_selected_model_index();
                                                        let opening = this
                                                            .ai
                                                            .toggle_discovery_menu(selected_index);
                                                        if opening {
                                                            this.reset_text_input(
                                                                "ai.model-search",
                                                                "",
                                                                cx,
                                                            );
                                                            let field = this.text_input(
                                                                "ai.model-search",
                                                                "",
                                                                TextInputSetup::placeholder(
                                                                    "Search models",
                                                                ),
                                                                cx,
                                                            );
                                                            window.focus(
                                                                &field.read(cx).focus_handle(),
                                                                cx,
                                                            );
                                                        }
                                                        cx.notify();
                                                    }))
                                                    .child(
                                                        div()
                                                            .min_w_0()
                                                            .overflow_hidden()
                                                            .child(truncate_preview(
                                                                &model_label,
                                                                24,
                                                            )),
                                                    )
                                                    .child(
                                                        svg()
                                                            .size(px(14.))
                                                            .flex_none()
                                                            .path("icons/chevron-down.svg")
                                                            .text_color(rgb(palette.text_dimmed)),
                                                    ),
                                            )
                                            .when(discovery_menu_open, |this| {
                                                let selected_id = selected_model_id.clone();
                                                let focused_index = discovery_index;
                                                let mut menu = div()
                                                    .absolute()
                                                    .left_0()
                                                    .bottom(px(34.))
                                                    .w(px(320.))
                                                    .max_h(px(280.))
                                                    .overflow_hidden()
                                                    .rounded_md()
                                                    .border_1()
                                                    .border_color(rgb(palette.border))
                                                    .bg(self.shell_surface_color(palette.surface))
                                                    .shadow_lg()
                                                    .p_1()
                                                    .flex()
                                                    .flex_col()
                                                    .on_mouse_down(MouseButton::Left, |_, _, _| {});
                                                if enabled_models.is_empty() {
                                                    menu = menu
                                                        .child(
                                                            div()
                                                                .px_2()
                                                                .py_2()
                                                                .text_size(px(11.))
                                                                .text_color(rgb(
                                                                    palette.text_muted,
                                                                ))
                                                                .child(self.tr("ai.noEnabledModels")),
                                                        )
                                                        .child(
                                                            div()
                                                                .id(SharedString::from(
                                                                    "ai-model-open-settings",
                                                                ))
                                                                .h(px(28.))
                                                                .px_2()
                                                                .flex()
                                                                .items_center()
                                                                .rounded_sm()
                                                                .cursor_pointer()
                                                                .text_size(px(11.))
                                                                .text_color(rgb(palette.link))
                                                                .hover(|this| {
                                                                    this.bg(rgb(palette.hover))
                                                                })
                                                                .on_click(cx.listener(
                                                                    |this, _, _, cx| {
                                                                        this.ai
                                                                            .close_discovery_menu();
                                                                        this.shell.set_settings_active_tab(SettingsTab::AiModels);
                                                                        this.open_page(
                                                                            NavItem::Settings,
                                                                            cx,
                                                                        );
                                                                    },
                                                                ))
                                                                .child(self.tr("ai.models")),
                                                        );
                                                } else {
                                                    menu = menu.child(
                                                        div()
                                                            .mb_1()
                                                            .on_key_down(cx.listener(
                                                                |this,
                                                                 event: &KeyDownEvent,
                                                                 _,
                                                                 cx| {
                                                                    this
                                                                        .handle_ai_model_search_key_down(
                                                                            event, cx,
                                                                        );
                                                                },
                                                            ))
                                                            .child(model_search_input),
                                                    );
                                                    if model_choices.is_empty() {
                                                        menu = menu.child(
                                                            div()
                                                                .h(px(52.))
                                                                .flex()
                                                                .items_center()
                                                                .justify_center()
                                                                .text_size(px(11.))
                                                                .text_color(rgb(
                                                                    palette.text_muted,
                                                                ))
                                                                .child(self.tr("ai.noModelMatches")),
                                                        );
                                                    } else {
                                                        let mut rows = div()
                                                            .id(SharedString::from(
                                                                "ai-model-choice-list",
                                                            ))
                                                            .max_h(px(220.))
                                                            .overflow_y_scrollbar()
                                                            .flex()
                                                            .flex_col();
                                                        for (index, (model, provider_label)) in
                                                            model_choices.into_iter().enumerate()
                                                        {
                                                            let model_id = model.id.clone();
                                                            let is_selected =
                                                                selected_id.as_deref()
                                                                    == Some(model.id.as_str());
                                                            let focused = index == focused_index;
                                                            rows = rows.child(
                                                                div()
                                                                    .id(SharedString::from(
                                                                        format!(
                                                                            "ai-model-choice-{}",
                                                                            model.id
                                                                        ),
                                                                    ))
                                                                    .h(px(34.))
                                                                    .px_2()
                                                                    .flex()
                                                                    .items_center()
                                                                    .gap_2()
                                                                    .rounded_sm()
                                                                    .bg(if focused || is_selected {
                                                                        rgb(palette.hover)
                                                                    } else {
                                                                        rgba(0x00000000)
                                                                    })
                                                                    .cursor_pointer()
                                                                    .hover(|this| {
                                                                        this.bg(rgb(palette.hover))
                                                                    })
                                                                    .on_click(cx.listener(
                                                                        move |this, _, _, cx| {
                                                                            this.ai
                                                                                .set_discovery_index(index);
                                                                            this.ai
                                                                                .close_discovery_menu();
                                                                            this.set_ai_default_model(
                                                                                model_id.clone(),
                                                                                cx,
                                                                            );
                                                                        },
                                                                    ))
                                                                    .child(
                                                                        div()
                                                                            .size(px(14.))
                                                                            .flex_none()
                                                                            .flex()
                                                                            .items_center()
                                                                            .justify_center()
                                                                            .text_color(rgb(
                                                                                palette.link,
                                                                            ))
                                                                            .when(is_selected, |this| {
                                                                                this.child(
                                                                                    svg()
                                                                                        .size(px(13.))
                                                                                        .path("icons/check.svg")
                                                                                        .text_color(rgb(palette.link)),
                                                                                )
                                                                            }),
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .min_w_0()
                                                                            .flex_1()
                                                                            .overflow_hidden()
                                                                            .text_size(px(11.))
                                                                            .font_weight(
                                                                                FontWeight(600.),
                                                                            )
                                                                            .text_color(rgb(
                                                                                palette.text,
                                                                            ))
                                                                            .child(
                                                                                truncate_preview(
                                                                                    &model.name,
                                                                                    32,
                                                                                ),
                                                                            ),
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .flex_none()
                                                                            .max_w(px(120.))
                                                                            .overflow_hidden()
                                                                            .text_size(px(10.))
                                                                            .text_color(rgb(
                                                                                palette.text_muted,
                                                                            ))
                                                                            .child(
                                                                                truncate_preview(
                                                                                    &provider_label,
                                                                                    24,
                                                                                ),
                                                                            ),
                                                                    ),
                                                            );
                                                        }
                                                        menu = menu.child(rows);
                                                    }
                                                }
                                                this.child(menu)
                                            }),
                                    ),
                            )
                            .child(ai_send_button(palette, ai_running, send_disabled, cx)),
                    )
                    .when(file_action_ready, |this| {
                        this.child(
                            div()
                                .text_size(px(10.))
                                .text_color(rgb(palette.warning))
                                .child(self.tr("ai.fileActionReady")),
                        )
                    }),
            )
    }
}

fn ai_send_button(
    palette: crate::theme::ThemePalette,
    running: bool,
    disabled: bool,
    cx: &mut Context<NyaTermApp>,
) -> impl IntoElement {
    let icon = if running {
        "icons/ai/stop.svg"
    } else {
        "icons/ai/send.svg"
    };
    div()
        .id(SharedString::from("ai-ask-run"))
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .text_color(if disabled {
            rgb(palette.text_dimmed)
        } else {
            rgb(palette.text_muted)
        })
        .opacity(if disabled { 0.48 } else { 1.0 })
        .when(!disabled, |this| {
            this.cursor_pointer().hover(|this| {
                this.bg(rgb(palette.surface_elevated))
                    .text_color(rgb(palette.text))
            })
        })
        .child(
            svg()
                .size(px(16.))
                .flex_none()
                .path(icon)
                .text_color(if disabled {
                    rgb(palette.text_dimmed)
                } else {
                    rgb(palette.text_muted)
                }),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            if this.ai.chat_or_agent_is_running() {
                this.cancel_ai_chat(cx);
            } else if !disabled {
                this.start_ai_ask(cx);
            }
        }))
}
