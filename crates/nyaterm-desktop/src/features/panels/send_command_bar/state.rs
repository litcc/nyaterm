use std::borrow::Cow;

use nyaterm_core::truncate_preview;
use nyaterm_transport::SessionKind;

use super::super::send_command_hex_preview;
use crate::features::{NyaTermApp, panels::SendCommandPresentationState};
use crate::send_command::{SendCommandDataType, SendCommandMode};

pub(super) struct SendCommandBarViewState {
    pub(super) send: SendCommandPresentationState,
    pub(super) palette: crate::theme::ThemePalette,
    pub(super) group_targets: Vec<(String, String, usize)>,
    pub(super) target_kind: Cow<'static, str>,
    pub(super) is_serial_text_line: bool,
    pub(super) validation_error: bool,
    pub(super) preview: String,
    pub(super) input_hint: Cow<'static, str>,
    pub(super) is_sending: bool,
    pub(super) progress_ratio: f32,
    pub(super) progress_label: String,
}

impl NyaTermApp {
    pub(super) fn send_command_bar_view_state(&self) -> SendCommandBarViewState {
        let send = self.send_command.presentation();
        let palette = self.theme_palette();
        let active_kind = self.active_session_kind();
        let group_targets = self.send_command_group_target_options();
        let target_kind = match active_kind {
            Some(SessionKind::Serial) => self.tr("serialSend.serialData"),
            Some(SessionKind::RawTcp) => Cow::Borrowed("Raw TCP"),
            Some(SessionKind::Telnet) => Cow::Borrowed("Telnet"),
            Some(SessionKind::Ssh | SessionKind::LocalPty) => self.tr("serialSend.shellCommand"),
            Some(SessionKind::Rdp | SessionKind::Vnc) => self.tr("serialSend.unavailable"),
            None => self.tr("serialSend.unavailable"),
        };
        let is_serial_text_line = matches!(active_kind, Some(SessionKind::Serial))
            && send.data_type == SendCommandDataType::Text
            && send.mode == SendCommandMode::Line;
        let unit_result = self.build_send_command_units(&send.draft, active_kind);
        let (validation_error, unit_count, byte_count) = match &unit_result {
            Ok(units) => {
                let bytes = units.iter().map(Vec::len).sum::<usize>();
                (false, units.len(), bytes)
            }
            Err(_) => (true, 0usize, 0usize),
        };
        let preview = if send.data_type == SendCommandDataType::Hex {
            send_command_hex_preview(&send.draft)
        } else {
            truncate_preview(&send.draft.replace('\n', "\\n"), 96)
        };
        let input_hint = if send.data_type == SendCommandDataType::Hex {
            self.tr("serialSend.hexPlaceholder")
        } else {
            self.tr("serialSend.textPlaceholder")
        };
        let _ = (unit_count, byte_count);
        let is_sending = send.sending;
        let infinite_progress = is_sending && send.rounds == 0;
        let progress_total = send.total.max(1);
        let progress_completed = if infinite_progress {
            send.completed
        } else {
            send.completed.min(progress_total)
        };
        let progress_ratio = if infinite_progress {
            // Indeterminate-ish pulse from completed units.
            (((progress_completed % 20) as f32) / 20.0).clamp(0.08, 0.95)
        } else {
            progress_completed as f32 / progress_total as f32
        };
        let progress_label = if is_sending {
            if infinite_progress {
                let round = self
                    .tr("serialSend.shellProgressInfinite")
                    .replace("{{current}}", &send.round.max(1).to_string());
                let units = self
                    .tr("serialSend.shellProgressUnits")
                    .replace("{{completed}}", &progress_completed.to_string())
                    .replace("{{total}}", "∞");
                format!("{round} · {units}")
            } else {
                let units = self
                    .tr("serialSend.shellProgressUnits")
                    .replace("{{completed}}", &progress_completed.to_string())
                    .replace("{{total}}", &send.total.to_string());
                let round = self
                    .tr("serialSend.shellProgressRound")
                    .replace("{{current}}", &send.round.max(1).to_string())
                    .replace("{{total}}", &send.rounds.max(1).to_string());
                format!("{units} · {round}")
            }
        } else {
            String::new()
        };

        SendCommandBarViewState {
            send,
            palette,
            group_targets,
            target_kind,
            is_serial_text_line,
            validation_error,
            preview,
            input_hint,
            is_sending,
            progress_ratio,
            progress_label,
        }
    }
}
