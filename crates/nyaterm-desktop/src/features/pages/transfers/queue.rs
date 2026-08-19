use gpui::{
    Context, InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, div, px, rgb,
};
use nyaterm_core::truncate_preview;

use crate::features::{
    NyaTermApp, shell::gpui_code_font_family, view_widgets::panel_header_with_actions,
};
use crate::models::{TransferJobState, TransferJobStatus};
use nyaterm_ui::{NyaScrollable, NyaTooltip};

use super::helpers::{TransferJobRowLabels, queue_action_button, transfer_job_row};

impl NyaTermApp {
    pub(super) fn transfer_queue_view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.theme_palette();
        let active_session_id = self.session.active_id();
        let visible_jobs = self
            .transfer
            .transfer_jobs()
            .iter()
            .filter(|job| job.is_visible_for_session(active_session_id))
            .cloned()
            .collect::<Vec<_>>();

        let has_running = visible_jobs
            .iter()
            .any(|job| job.status == TransferJobStatus::Running && job.control.is_some());
        let has_paused = visible_jobs
            .iter()
            .any(|job| job.status == TransferJobStatus::Paused && job.control.is_some());
        let has_active = visible_jobs.iter().any(|job| {
            job.control.is_some()
                && matches!(
                    job.status,
                    TransferJobStatus::Running | TransferJobStatus::Paused
                )
        });
        let has_completed = visible_jobs
            .iter()
            .any(|job| job.status == TransferJobStatus::Completed);
        let has_stopped = visible_jobs.iter().any(|job| {
            !matches!(
                job.status,
                TransferJobStatus::Running
                    | TransferJobStatus::Paused
                    | TransferJobStatus::Cancelling
            )
        });
        let download_path = self
            .resolved_transfer_download_dir()
            .map(|path| truncate_preview(&path.display().to_string(), 64))
            .unwrap_or_else(|| format!("{}: -", self.tr("fileTransfer.downloadPath")));

        let mut list = div().flex().flex_col();
        if self.session.active_id().is_none() {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px_3()
                    .py_6()
                    .text_size(px(11.))
                    .text_color(rgb(palette.text_dimmed))
                    .child(self.tr("fileExplorer.connectToSession")),
            );
        } else if visible_jobs.is_empty() {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px_3()
                    .py_6()
                    .text_size(px(11.))
                    .text_color(rgb(palette.text_dimmed))
                    .child(self.tr("fileTransfer.noTransfers")),
            );
        } else {
            list = list.gap(px(2.)).p_1();
            let row_labels = TransferJobRowLabels {
                transferring: self.tr("fileTransfer.transferring").to_string(),
                paused: self.tr("fileTransfer.paused").to_string(),
                cancelling: self.tr("fileTransfer.cancelling").to_string(),
                cancelled: self.tr("fileTransfer.cancelled").to_string(),
                completed: self.tr("fileTransfer.completed").to_string(),
                failed: self.tr("fileTransfer.failed").to_string(),
                streaming: self.tr("fileTransfer.streaming").to_string(),
                unknown_size: self.tr("fileTransfer.unknownSize").to_string(),
            };
            for job in ordered_transfer_jobs(&visible_jobs) {
                let directory_progress = job.progress.as_ref().and_then(|progress| {
                    progress
                        .item_count_completed
                        .zip(progress.item_count_total)
                        .map(|(completed, total)| {
                            self.tr("fileTransfer.directoryProgress")
                                .replace("{{completed}}", &completed.to_string())
                                .replace("{{total}}", &total.to_string())
                        })
                });
                list = list.child(transfer_job_row(
                    palette,
                    job,
                    directory_progress,
                    row_labels.clone(),
                    self.transfer.browser_view().selected_remote_path.clone(),
                    self.transfer.selected_transfer_job_id().map(str::to_string),
                    cx,
                ));
            }
        }
        div()
            .id(SharedString::from("transfer-queue-panel"))
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(self.shell_transparent_color(palette.surface))
            .track_focus(self.transfer.queue_focus())
            .on_click(cx.listener(|this, _, window, cx| {
                window.focus(this.transfer.queue_focus(), cx);
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.handle_transfer_queue_key_down(event, window, cx);
            }))
            .child(panel_header_with_actions(
                self.tr("panel.fileTransfer"),
                "",
                palette,
                self.shell_transparent_color(palette.section_header),
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(queue_action_button(
                            palette,
                            "transfer-pause-all",
                            "icons/transfer/pause.svg",
                            self.tr("fileTransfer.pauseAll"),
                            has_running,
                            cx.listener(|this, _, _, cx| {
                                this.pause_all_transfer_jobs(cx);
                            }),
                        ))
                        .child(queue_action_button(
                            palette,
                            "transfer-resume-all",
                            "icons/transfer/play.svg",
                            self.tr("fileTransfer.resumeAll"),
                            has_paused,
                            cx.listener(|this, _, _, cx| {
                                this.resume_all_transfer_jobs(cx);
                            }),
                        ))
                        .child(queue_action_button(
                            palette,
                            "transfer-cancel-all",
                            "icons/transfer/stop.svg",
                            self.tr("fileTransfer.cancelAll"),
                            has_active,
                            cx.listener(|this, _, _, cx| {
                                this.cancel_all_transfer_jobs(cx);
                            }),
                        ))
                        .child(queue_action_button(
                            palette,
                            "transfer-clear-completed",
                            "icons/transfer/playlist-remove.svg",
                            self.tr("fileTransfer.clearCompleted"),
                            has_completed,
                            cx.listener(|this, _, _, cx| {
                                this.clear_completed_transfer_jobs(cx);
                            }),
                        ))
                        .child(queue_action_button(
                            palette,
                            "transfer-clear-stopped",
                            "icons/transfer/clear-all.svg",
                            self.tr("fileTransfer.clearAll"),
                            has_stopped,
                            cx.listener(|this, _, _, cx| {
                                this.clear_stopped_transfer_jobs(cx);
                            }),
                        ))
                        .into_any_element(),
                ),
            ))
            .child(
                div()
                    .id(SharedString::from("transfer-queue-scroll"))
                    .flex_1()
                    .min_h_0()
                    .overflow_scrollbar()
                    .child(list),
            )
            .child(
                div()
                    .h(px(26.))
                    .px_2()
                    .border_t_1()
                    .border_color(rgb(palette.border))
                    .bg(self.shell_transparent_color(palette.surface))
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .id(SharedString::from("transfer-download-path-footer"))
                            .min_w_0()
                            .flex_1()
                            .font_family(gpui_code_font_family())
                            .text_size(px(11.))
                            .text_color(rgb(palette.text_muted))
                            .cursor_pointer()
                            .hover(|this| this.text_color(rgb(palette.text)))
                            .tooltip({
                                let label = self.tr("fileTransfer.downloadPath").to_string();
                                move |window, cx| NyaTooltip::new(label.clone()).build(window, cx)
                            })
                            .child(download_path)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reveal_transfer_download_dir(cx);
                            })),
                    ),
            )
    }
}

fn ordered_transfer_jobs(jobs: &[TransferJobState]) -> Vec<TransferJobState> {
    let mut indexed_jobs = jobs.iter().cloned().enumerate().collect::<Vec<_>>();
    indexed_jobs.sort_by(|(left_index, left), (right_index, right)| {
        transfer_job_display_rank(left.status)
            .cmp(&transfer_job_display_rank(right.status))
            .then_with(|| right_index.cmp(left_index))
    });
    indexed_jobs.into_iter().map(|(_, job)| job).collect()
}

fn transfer_job_display_rank(status: TransferJobStatus) -> u8 {
    match status {
        TransferJobStatus::Running | TransferJobStatus::Cancelling => 0,
        TransferJobStatus::Paused => 1,
        TransferJobStatus::Cancelled | TransferJobStatus::Completed | TransferJobStatus::Failed => {
            2
        }
    }
}
