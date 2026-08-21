use gpui::{Context, IntoElement, SharedString, div, prelude::*, px, rgb};
use nyaterm_ui::{NyaNumberInputOptions, NyaSelectOption};

use crate::features::{NyaTermApp, text_inputs::TextInputSetup};
use nyaterm_ui::NyaTooltip;

use super::{
    settings_form_row, settings_form_section, settings_switch, settings_switch_with_enabled,
};

impl NyaTermApp {
    pub(in crate::features) fn security_settings_section(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let master_password = self.settings.master_password();
        let master_password_draft = master_password.draft.to_string();
        let master_password_enabled = master_password.enabled;
        let master_password_input = self
            .text_input_box(
                "settings.security.master-password",
                &master_password_draft,
                TextInputSetup::masked(),
                cx,
            )
            .into_any_element();
        let master_password_switch_enabled = !self.cloud_sync.settings().enabled;
        let has_stored_master_password = self.settings.summary().has_master_password;
        let idle_minutes = self.settings.summary().idle_lock_minutes;
        let host_key_policy = match self.settings.summary().host_key_policy.as_str() {
            "strict" | "reject" => "strict",
            "accept" | "accept_new" => "accept",
            _ => "prompt",
        };

        let master_section_label = self.tr("settings.masterPasswordSection");
        let master_switch_label = self.tr("settings.masterPasswordSwitch");
        let master_switch_desc = self.tr("settings.masterPasswordSwitchDesc");
        let master_locked_desc = self.tr("settings.masterPasswordLockedByCloudSync");
        let master_set_label = self.tr("settings.masterPasswordIsSet");
        let master_input_label = self.tr(if has_stored_master_password {
            "settings.masterPasswordNew"
        } else {
            "settings.masterPassword"
        });
        let master_input_desc = self.tr("settings.masterPasswordDesc");
        let session_security_label = self.tr("settings.sessionSecurity");
        let screen_lock_label = self.tr("settings.enableScreenLock");
        let screen_lock_desc = self.tr("settings.enableScreenLockDesc");
        let idle_lock_label = self.tr("settings.idleLockMinutes");
        let idle_lock_desc = self.tr("settings.idleLockMinutesDesc");
        let minutes_label = self.tr("common.minutes");
        let host_key_label = self.tr("settings.hostKeyPolicy");
        let host_key_desc = self.tr("settings.hostKeyPolicyDesc");

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(settings_form_section(
                palette,
                Some(master_section_label),
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(settings_form_row(
                        palette,
                        master_switch_label,
                        Some(SharedString::from(master_switch_desc)),
                        div()
                            .id("settings-master-password-switch-wrap")
                            .when(!master_password_switch_enabled, |this| {
                                this.tooltip(move |window, cx| {
                                    NyaTooltip::new(master_locked_desc.clone()).build(window, cx)
                                })
                            })
                            .child(settings_switch_with_enabled(
                                palette,
                                "settings-master-password-enabled",
                                master_password_enabled,
                                master_password_switch_enabled,
                                cx.listener(|this, _, _, cx| {
                                    this.toggle_settings_master_password(cx);
                                }),
                            )),
                    ))
                    .when(
                        has_stored_master_password && master_password_draft.is_empty(),
                        |this| {
                            this.child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(palette.text_muted))
                                    .child(master_set_label),
                            )
                        },
                    )
                    .child(settings_form_row(
                        palette,
                        master_input_label,
                        Some(SharedString::from(master_input_desc)),
                        div()
                            // The row's control slot is content-sized, and a box
                            // with nothing typed in it has no content.
                            .w(px(260.))
                            .flex()
                            .opacity(if master_password_enabled { 1.0 } else { 0.45 })
                            .child(div().min_w_0().flex_1().child(master_password_input)),
                    )),
            ))
            .child(settings_form_section(
                palette,
                Some(session_security_label),
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(settings_form_row(
                        palette,
                        screen_lock_label,
                        Some(SharedString::from(screen_lock_desc)),
                        settings_switch(
                            palette,
                            "settings-screen-lock-enabled",
                            self.settings.summary().enable_screen_lock,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_screen_lock_enabled(cx);
                            }),
                        ),
                    ))
                    .when(self.settings.summary().enable_screen_lock, |this| {
                        this.child(settings_form_row(
                            palette,
                            idle_lock_label,
                            Some(SharedString::from(idle_lock_desc)),
                            self.number_input_box(
                                "settings.number.idle-lock-minutes",
                                idle_minutes.to_string().as_str(),
                                NyaNumberInputOptions::default()
                                    .range(0.0, 1440.0)
                                    .step(1.0)
                                    .suffix(minutes_label),
                                cx,
                            ),
                        ))
                    }),
            ))
            .child(settings_form_section(
                palette,
                None,
                None,
                self.settings_select_field(
                    "settings.security.host-key-policy",
                    host_key_label,
                    Some(SharedString::from(host_key_desc)),
                    vec![
                        NyaSelectOption::new("strict", self.tr("settings.hostKeyStrict")),
                        NyaSelectOption::new("prompt", self.tr("settings.hostKeyPrompt")),
                        NyaSelectOption::new("accept", self.tr("settings.hostKeyAccept")),
                    ],
                    host_key_policy,
                    false,
                    cx,
                ),
            ))
    }
}
