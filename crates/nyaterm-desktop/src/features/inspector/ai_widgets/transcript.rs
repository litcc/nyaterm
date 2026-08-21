use rust_i18n::t;

use gpui::{Context, FontWeight, IntoElement, SharedString, div, prelude::*, px, rgb, svg};

use crate::features::NyaTermApp;
use crate::models::{NavItem, SettingsTab};

impl NyaTermApp {
    pub(in crate::features) fn ai_transcript_body(
        &self,
        mode_label: &'static str,
        enabled: bool,
        agent_step_rows: impl IntoElement,
        command_rows: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut body = div().flex().flex_col().gap_2();
        if self.ai.chat_messages().is_empty() {
            body = body.child(self.ai_empty_transcript(mode_label, enabled, cx));
        } else {
            for message in self.ai.chat_messages() {
                body = body.child(self.ai_message_bubble(message, cx));
            }
        }
        body.child(agent_step_rows).child(command_rows)
    }

    pub(in crate::features) fn ai_empty_transcript(
        &self,
        _mode_label: &'static str,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let palette = self.theme_palette();
        let has_model = !self.ai.settings_model_draft().trim().is_empty()
            || self
                .ai
                .settings_config()
                .default_model_id
                .as_ref()
                .is_some_and(|id| !id.trim().is_empty());
        if !enabled {
            return div()
                .min_h(px(192.))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_3()
                .px_3()
                .child(
                    svg()
                        .size(px(36.))
                        .path("icons/ai.svg")
                        .text_color(rgb(palette.text_muted)),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(palette.text_muted))
                        .child(t!("ai.goToSettingsToEnable")),
                )
                .into_any_element();
        }
        if !has_model {
            return div()
                .min_h(px(240.))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_3()
                .px_4()
                .child(
                    div()
                        .size(px(48.))
                        .rounded_full()
                        .border_1()
                        .border_color(rgb(0x9e6a03))
                        .bg(rgb(0x3d2e00))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(crate::features::view_widgets::mono_icon(
                            "icons/warning.svg",
                            rgb(palette.warning).into(),
                            22.,
                        )),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight(700.))
                        .text_color(rgb(palette.text))
                        .child(t!("ai.setupTitle")),
                )
                .child(self.ai_setup_step("1", t!("ai.setupStep1")))
                .child(self.ai_setup_step("2", t!("ai.setupStep2")))
                .child(
                    div()
                        .id(SharedString::from("ai-empty-open-settings-setup"))
                        .mt_1()
                        .h(px(30.))
                        .px_3()
                        .rounded_md()
                        .bg(rgb(palette.success))
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_size(px(12.))
                        .font_weight(FontWeight(600.))
                        .text_color(rgb(0xffffff))
                        .cursor_pointer()
                        .hover(|this| this.bg(rgb(0x2ea043)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.shell.set_settings_active_tab(SettingsTab::AiGeneral);
                            this.open_page(NavItem::Settings, cx);
                        }))
                        .child(t!("ai.setupAction")),
                )
                .into_any_element();
        }
        div()
            .min_h(px(180.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .px_3()
            .child(
                svg()
                    .size(px(40.))
                    .path("icons/ai.svg")
                    .text_color(rgb(palette.text_muted)),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(palette.text_muted))
                    .child(t!("ai.empty")),
            )
            .into_any_element()
    }

    pub(in crate::features) fn ai_setup_step(
        &self,
        index: &'static str,
        label: impl Into<SharedString>,
    ) -> impl IntoElement {
        let label: SharedString = label.into();
        let palette = self.theme_palette();
        div()
            .w_full()
            .max_w(px(280.))
            .rounded_md()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.bg))
            .px_3()
            .py_2()
            .flex()
            .items_start()
            .gap_2()
            .child(
                div()
                    .size(px(18.))
                    .rounded_full()
                    .bg(rgb(palette.hover))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(10.))
                    .font_weight(FontWeight(800.))
                    .text_color(rgb(palette.link))
                    .child(index),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(palette.text))
                    .child(label),
            )
    }
}
