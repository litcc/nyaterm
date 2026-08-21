use rust_i18n::t;

use gpui::{Context, IntoElement, SharedString, div, prelude::*};
use nyaterm_ui::NyaNumberInputOptions;

use crate::features::{NyaTermApp, text_inputs::TextInputSetup};

use super::super::{
    settings_form_row, settings_form_section, settings_input_control, settings_switch,
};

impl NyaTermApp {
    pub(in crate::features::pages::settings) fn transfer_advanced_settings_section(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let permissions = self
            .settings
            .summary()
            .transfer_default_file_permissions
            .clone();
        let permissions_input = self
            .text_input_box(
                "settings.transfer.default-permissions",
                &permissions,
                TextInputSetup::placeholder("644"),
                cx,
            )
            .into_any_element();

        div().flex().flex_col().gap_3().child(settings_form_section(
            palette,
            None,
            None,
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(settings_form_row(
                    palette,
                    t!("settings.downloadConcurrentTasks"),
                    Some(SharedString::from(t!(
                        "settings.downloadConcurrentTasksDesc"
                    ))),
                    self.number_input_box(
                        "settings.number.transfer-download-threads",
                        self.settings
                            .summary()
                            .transfer_download_threads
                            .to_string()
                            .as_str(),
                        NyaNumberInputOptions::default().range(1.0, 10.0).step(1.0),
                        cx,
                    ),
                ))
                .child(settings_form_row(
                    palette,
                    t!("settings.uploadConcurrentTasks"),
                    Some(SharedString::from(t!("settings.uploadConcurrentTasksDesc"))),
                    self.number_input_box(
                        "settings.number.transfer-upload-threads",
                        self.settings
                            .summary()
                            .transfer_upload_threads
                            .to_string()
                            .as_str(),
                        NyaNumberInputOptions::default().range(1.0, 10.0).step(1.0),
                        cx,
                    ),
                ))
                .child(settings_form_row(
                    palette,
                    t!("settings.maxTransferRetries"),
                    Some(SharedString::from(t!("settings.maxTransferRetriesDesc"))),
                    self.number_input_box(
                        "settings.number.transfer-max-retries",
                        self.settings
                            .summary()
                            .transfer_max_retries
                            .to_string()
                            .as_str(),
                        NyaNumberInputOptions::default().range(0.0, 10.0).step(1.0),
                        cx,
                    ),
                ))
                .child(settings_form_row(
                    palette,
                    t!("settings.transferBufferSize"),
                    Some(SharedString::from(t!("settings.transferBufferSizeDesc"))),
                    self.number_input_box(
                        "settings.number.transfer-buffer-size",
                        self.settings
                            .summary()
                            .transfer_buffer_size
                            .to_string()
                            .as_str(),
                        NyaNumberInputOptions::default().range(8.0, 256.0).step(8.0),
                        cx,
                    ),
                ))
                .child(settings_form_row(
                    palette,
                    t!("settings.preserveTimestamps"),
                    Some(SharedString::from(t!("settings.preserveTimestampsDesc"))),
                    settings_switch(
                        palette,
                        "settings-transfer-preserve-timestamps",
                        self.settings.summary().transfer_preserve_timestamps,
                        cx.listener(|this, _, _, cx| {
                            this.toggle_transfer_preserve_timestamps(cx);
                        }),
                    ),
                ))
                .child(settings_form_row(
                    palette,
                    t!("settings.resumeBrokenTransfer"),
                    Some(SharedString::from(t!("settings.resumeBrokenTransferDesc"))),
                    settings_switch(
                        palette,
                        "settings-transfer-resume-broken",
                        self.settings.summary().transfer_resume_broken_transfer,
                        cx.listener(|this, _, _, cx| {
                            this.toggle_transfer_resume_broken(cx);
                        }),
                    ),
                ))
                .child(settings_form_row(
                    palette,
                    t!("settings.defaultFilePermissions"),
                    Some(SharedString::from(t!(
                        "settings.defaultFilePermissionsDesc"
                    ))),
                    settings_input_control(260., permissions_input),
                )),
        ))
    }
}
