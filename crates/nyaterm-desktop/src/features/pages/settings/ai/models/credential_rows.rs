use gpui::{
    Context, FontWeight, IntoElement, KeyDownEvent, SharedString, div, prelude::*, px, rgb,
};

use crate::features::{NyaTermApp, text_inputs::TextInputSetup};
use crate::widgets::small_button;

use super::super::super::settings_switch;

impl NyaTermApp {
    pub(super) fn ai_credential_rows(
        &mut self,
        palette: crate::theme::ThemePalette,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let credential_secret_drafts = self.ai.settings_credential_secret_drafts().clone();
        let profile_name_label = self.tr("ai.profileName");
        let base_url_label = self.tr("ai.baseUrl");
        let delete_label = self.tr("common.delete");
        let save_label = self.tr("common.save");

        // Cloned up front so the fold can borrow `self` mutably: each row builds
        // three real inputs, and creating one needs the app.
        let credentials = self.ai.settings_config().provider_credentials.clone();
        credentials
            .into_iter()
            .fold(div().flex().flex_col().gap_4(), |rows, credential| {
                let credential_id = credential.id.clone();
                let credential_id_toggle = credential.id.clone();
                let credential_id_delete = credential.id.clone();
                let credential_id_save = credential.id.clone();
                let is_builtin = matches!(
                    credential.id.as_str(),
                    "openai"
                        | "anthropic"
                        | "gemini"
                        | "deepseek"
                        | "ollama"
                        | "xai"
                        | "cohere"
                        | "mimo"
                        | "zai"
                        | "groq"
                );
                let secret_draft = credential_secret_drafts
                    .get(&credential.id)
                    .cloned()
                    .unwrap_or_default();
                let name_input = self
                    .text_input_box(
                        format!("ai.credential.{}.name", credential.id),
                        &credential.name,
                        TextInputSetup::placeholder(profile_name_label.clone()),
                        cx,
                    )
                    .into_any_element();
                let base_url_input = self
                    .text_input_box(
                        format!("ai.credential.{}.base-url", credential.id),
                        credential.base_url.as_deref().unwrap_or(""),
                        TextInputSetup::placeholder(base_url_label.clone()),
                        cx,
                    )
                    .into_any_element();
                let api_key_input = self
                    .text_input_box(
                        format!("ai.credential.{}.api-key", credential.id),
                        &secret_draft,
                        TextInputSetup::masked(),
                        cx,
                    )
                    .into_any_element();

                rows.child(
                    div()
                        .id(SharedString::from(format!(
                            "ai-cred-card-{}",
                            credential.id
                        )))
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(palette.border))
                        .bg(rgb(palette.bg))
                        .p_4()
                        .flex()
                        .flex_col()
                        .gap_4()
                        // Enter saves the row; the boxes own everything else.
                        .on_key_down({
                            let credential_id = credential.id.clone();
                            cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                                if event.keystroke.key.as_str() == "enter" {
                                    cx.stop_propagation();
                                    this.persist_ai_credential_edits(&credential_id, cx);
                                }
                            })
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex_1()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight(500.))
                                        .text_color(rgb(palette.text))
                                        .overflow_hidden()
                                        .child(credential.name.clone()),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(settings_switch(
                                            palette,
                                            format!("ai-cred-list-enabled-{}", credential.id),
                                            credential.enabled,
                                            cx.listener(move |this, _, _, cx| {
                                                this.toggle_ai_credential_enabled(
                                                    credential_id_toggle.clone(),
                                                    cx,
                                                );
                                            }),
                                        ))
                                        .when(!is_builtin, |this| {
                                            this.child(small_button(
                                                palette,
                                                format!("ai-cred-delete-{}", credential.id),
                                                delete_label.clone(),
                                                cx.listener(move |this, _, _, cx| {
                                                    this.remove_ai_credential(
                                                        credential_id_delete.clone(),
                                                        cx,
                                                    );
                                                }),
                                            ))
                                        }),
                                ),
                        )
                        .when(!is_builtin, |body| {
                            body.child(
                                div()
                                    .grid()
                                    .grid_cols(2)
                                    .gap_2()
                                    .child(name_input)
                                    .child(base_url_input),
                            )
                        })
                        .child(api_key_input)
                        .child(div().flex().justify_end().child(small_button(
                            palette,
                            format!("ai-cred-save-{}", credential_id),
                            save_label.clone(),
                            cx.listener(move |this, _, _, cx| {
                                this.persist_ai_credential_edits(&credential_id_save, cx);
                            }),
                        ))),
                )
            })
    }
}
