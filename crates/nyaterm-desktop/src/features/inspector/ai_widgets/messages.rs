use gpui::{
    App, ClickEvent, ClipboardItem, Context, FontWeight, IntoElement, MouseButton, MouseDownEvent,
    SharedString, Window, div, prelude::*, px, rgb, svg,
};
use nyaterm_core::{AiMessage, AiMessageRole, truncate_preview};

use crate::features::NyaTermApp;
use crate::features::formatting::extract_think_content;
use crate::features::view_widgets::full_window_input_layer;
use crate::features::view_widgets::markdown_content_view;
use crate::models::AiMessageMenuState;

fn ai_message_menu_position(
    x: f32,
    y: f32,
    menu_width: f32,
    menu_height: f32,
    viewport_width: f32,
    viewport_height: f32,
) -> (f32, f32, f32) {
    let margin = 8.;
    let max_height = (viewport_height - margin * 2.).max(64.);
    let height = menu_height.min(max_height);
    let max_x = (viewport_width - menu_width - margin).max(margin);
    let max_y = (viewport_height - height - margin).max(margin);
    (x.clamp(margin, max_x), y.clamp(margin, max_y), max_height)
}

impl NyaTermApp {
    pub(in crate::features) fn close_ai_message_menu(&mut self, cx: &mut Context<Self>) {
        self.ai.close_message_menu();
        cx.notify();
    }

    pub(in crate::features) fn quote_ai_message_text(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.ai.quote_message(text);
        cx.notify();
    }

    pub(in crate::features) fn copy_ai_message_text(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        let value = text.trim().to_string();
        let copied = !value.is_empty();
        if copied {
            cx.write_to_clipboard(ClipboardItem::new_string(value));
        }
        self.ai.finish_copy_message(copied);
        cx.notify();
    }

    pub(in crate::features) fn clear_ai_quote(&mut self, cx: &mut Context<Self>) {
        self.ai.clear_quote();
        cx.notify();
    }

    pub(in crate::features) fn ai_message_context_menu_overlay(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let state = self
            .ai
            .chat_message_menu()
            .cloned()
            .unwrap_or(AiMessageMenuState {
                message_id: String::new(),
                text: String::new(),
                x: px(24.),
                y: px(24.),
            });
        let quote_text = state.text.clone();
        let copy_text = state.text.clone();
        let (viewport_w, viewport_h) = self.shell.viewport_size();
        let (menu_x, menu_y, menu_max_h) = ai_message_menu_position(
            f32::from(state.x),
            f32::from(state.y),
            128.,
            64.,
            viewport_w,
            viewport_h,
        );
        full_window_input_layer("ai-message-context-menu-overlay")
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_ai_message_menu(cx);
            }))
            .child(
                div()
                    .id(SharedString::from("ai-message-context-menu"))
                    .absolute()
                    .top(px(menu_y))
                    .left(px(menu_x))
                    .w(px(128.))
                    .max_h(px(menu_max_h))
                    .overflow_y_scroll()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(self.shell_surface_color(palette.bg))
                    .shadow_lg()
                    .py_1()
                    .flex()
                    .flex_col()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(ai_message_menu_button(
                        palette,
                        "ai-message-menu-quote",
                        "icons/quote.svg",
                        self.tr("ai.quote"),
                        cx.listener(move |this, _, _, cx| {
                            this.quote_ai_message_text(quote_text.clone(), cx);
                        }),
                    ))
                    .child(ai_message_menu_button(
                        palette,
                        "ai-message-menu-copy",
                        "icons/copy.svg",
                        self.tr("ai.copy"),
                        cx.listener(move |this, _, _, cx| {
                            this.copy_ai_message_text(copy_text.clone(), cx);
                        }),
                    )),
            )
    }

    pub(in crate::features) fn ai_message_bubble(
        &self,
        message: &AiMessage,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let is_user = matches!(message.role, AiMessageRole::User);
        let streaming = self
            .ai
            .chat_streaming_assistant_id()
            .is_some_and(|id| id == message.id);
        let role_label = if is_user { "User" } else { "AI" };
        let raw = if message.content.trim().is_empty() {
            String::new()
        } else {
            message.content.clone()
        };
        let (visible, think_reasoning) = extract_think_content(&raw);
        let mut reasoning = message
            .reasoning_content
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if reasoning.is_none() {
            reasoning = think_reasoning;
        }
        let display = if visible.trim().is_empty() {
            if streaming { String::new() } else { visible }
        } else {
            visible
        };
        let menu_text = if display.trim().is_empty() {
            raw.clone()
        } else {
            display.clone()
        };
        let menu_message_id = message.id.clone();

        // Tauri AssistantResponse/User: compact bubbles, softer borders.
        let mut bubble = div()
            .id(SharedString::from(format!("ai-msg-{}", message.id)))
            .rounded_md()
            .border_1()
            .border_color(if is_user {
                rgb(0x1f6feb)
            } else {
                rgb(palette.border)
            })
            .bg(if is_user {
                rgb(palette.hover)
            } else {
                rgb(palette.bg)
            })
            .px_2()
            .py_2()
            .flex()
            .flex_col()
            .gap_1()
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.ai.open_message_menu(AiMessageMenuState {
                        message_id: menu_message_id.clone(),
                        text: menu_text.clone(),
                        x: event.position.x,
                        y: event.position.y,
                    });
                    cx.notify();
                }),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .font_weight(FontWeight(700.))
                    .text_color(rgb(palette.text_muted))
                    .child(role_label),
            );

        if let Some(reasoning) = reasoning {
            bubble = bubble.child(
                div()
                    .rounded_md()
                    .border_1()
                    .border_color(if streaming {
                        rgb(0x1f6feb)
                    } else {
                        rgb(palette.border)
                    })
                    .bg(if streaming {
                        rgb(palette.hover)
                    } else {
                        rgb(palette.bg)
                    })
                    .px_2()
                    .py_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(10.))
                            .font_weight(FontWeight(700.))
                            .text_color(if streaming {
                                rgb(palette.link)
                            } else {
                                rgb(palette.text_muted)
                            })
                            .child(if streaming {
                                self.tr("ai.thinking")
                            } else {
                                self.tr("ai.thoughtComplete")
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(palette.text_muted))
                            .line_height(px(16.))
                            .child(markdown_content_view(
                                palette,
                                &truncate_preview(&reasoning, 1200),
                            )),
                    ),
            );
        } else if streaming && display.trim().is_empty() {
            bubble = bubble.child(
                div()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0x1f6feb))
                    .bg(rgb(palette.hover))
                    .px_2()
                    .py_2()
                    .text_size(px(11.))
                    .text_color(rgb(palette.link))
                    .child(self.tr("ai.thinking")),
            );
        }

        let has_display = !display.trim().is_empty();
        if has_display {
            if is_user {
                bubble = bubble.child(ai_user_pre_wrap_text(palette, &display));
            } else {
                let rendered = truncate_preview(&display, 8000);
                bubble = bubble.child(markdown_content_view(palette, &rendered));
            }
        }

        if !message.command_cards.is_empty() {
            // Tauri renders AICommandCardView inside assistant responses.
            for (card_index, card) in message.command_cards.iter().cloned().enumerate() {
                bubble = bubble.child(Self::ai_command_card_view_for_card(
                    palette,
                    format!("{}-{}", message.id, card_index),
                    card,
                    cx,
                ));
            }
        }
        bubble
    }

    pub(in crate::features) fn ai_assistant_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Tauri AIAssistantPanel: toolbar + scroll transcript + bottom composer.
        // Shared stack already renders PanelHeader; body fills remaining height.
        self.ai_ask_panel(cx)
    }
}

fn ai_user_pre_wrap_text(palette: crate::theme::ThemePalette, text: &str) -> gpui::AnyElement {
    let mut block = div()
        .min_w_0()
        .flex()
        .flex_col()
        .text_size(px(12.))
        .text_color(rgb(palette.text))
        .line_height(px(18.));
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let line_text = if line.is_empty() { " " } else { line }.to_string();
        block = block.child(div().min_w_0().line_height(px(18.)).child(line_text));
    }
    block.into_any_element()
}

fn ai_message_menu_button(
    palette: crate::theme::ThemePalette,
    id: &'static str,
    icon: &'static str,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label: SharedString = label.into();
    div()
        .id(SharedString::from(id))
        .h(px(28.))
        .px_2()
        .flex()
        .items_center()
        .gap_2()
        .rounded_sm()
        .text_size(px(12.))
        .text_color(rgb(palette.text))
        .cursor_pointer()
        .hover(|this| this.bg(rgb(palette.hover)))
        .on_click(on_click)
        .child(
            svg()
                .size(px(14.))
                .flex_none()
                .path(icon)
                .text_color(rgb(palette.text_muted)),
        )
        .child(label)
}

#[cfg(test)]
mod tests {
    use super::ai_message_menu_position;

    #[test]
    fn menu_position_stays_inside_viewport() {
        assert_eq!(
            ai_message_menu_position(1240., 780., 128., 64., 1280., 800.),
            (1144., 728., 784.)
        );
        assert_eq!(
            ai_message_menu_position(240., 180., 128., 64., 200., 120.),
            (64., 48., 104.)
        );
    }
}
