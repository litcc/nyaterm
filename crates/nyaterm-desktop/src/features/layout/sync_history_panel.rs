use gpui::{
    ClipboardItem, Context, FontWeight, IntoElement, SharedString, div, prelude::*, px, rgb, svg,
};
use nyaterm_core::{CloudConflictKind, truncate_preview};

use crate::features::NyaTermApp;
use crate::features::formatting::{
    cloud_sync_status_dot_color, cloud_sync_status_text_color, configured_cloud_sync_provider,
    format_cloud_provider, format_duration_ms,
};
use crate::features::view_widgets::{
    CloudSyncHistoryRowLabels, cloud_sync_history_row, dialog_action_button,
};
use crate::widgets::small_button;
use nyaterm_ui::{NyaScrollable, NyaTooltip};

impl NyaTermApp {
    pub(in crate::features) fn sync_backup_history_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        // Tauri SyncBackupHistoryPanel:
        // shared PanelHeader + status strip + optional conflict card + dense history list.
        let provider = configured_cloud_sync_provider(self.cloud_sync.settings());
        let provider_label = format_cloud_provider(&provider);
        let enabled = self.cloud_sync.settings().enabled;
        let state = if !enabled {
            "disabled"
        } else if self.cloud_sync.conflict().is_some() {
            "conflict"
        } else if self
            .cloud_sync
            .status()
            .to_ascii_lowercase()
            .contains("fail")
        {
            "failed"
        } else if self
            .cloud_sync
            .status()
            .to_ascii_lowercase()
            .contains("push")
            || self
                .cloud_sync
                .status()
                .to_ascii_lowercase()
                .contains("pull")
            || self
                .cloud_sync
                .status()
                .to_ascii_lowercase()
                .contains("running")
        {
            "running"
        } else if self
            .cloud_sync
            .status()
            .to_ascii_lowercase()
            .contains("success")
            || self
                .cloud_sync
                .status()
                .to_ascii_lowercase()
                .contains("synced")
            || self
                .cloud_sync
                .status()
                .to_ascii_lowercase()
                .contains("ready")
        {
            "success"
        } else {
            "idle"
        };
        let state_label = match state {
            "disabled" => self.tr("settings.syncState.disabled"),
            "conflict" => self.tr("settings.syncState.conflict"),
            "failed" => self.tr("settings.syncState.failed"),
            "running" => self.tr("settings.syncState.running"),
            "success" => self.tr("settings.syncState.success"),
            _ => self.tr("settings.syncState.idle"),
        };
        let status_message = self.cloud_sync.status().to_string();
        let history = self.cloud_sync.history().to_vec();
        let expanded = self.cloud_sync.history_expanded().clone();
        let conflict = self.cloud_sync.conflict().cloned();
        let sync_action_enabled = enabled && !self.cloud_sync.job_running();

        let mut rows = div().flex().flex_col();
        if history.is_empty() {
            rows = rows.child(
                div()
                    .py_6()
                    .px_3()
                    .text_center()
                    .text_size(px(11.))
                    .text_color(rgb(palette.text_dimmed))
                    .child(self.tr("settings.historyNoEntries")),
            );
        } else {
            for entry in history {
                let entry_id = entry.id.clone();
                let is_open = expanded.contains(&entry_id);
                let copy_message = entry.message.clone();
                let kind_label = self.tr(match entry.kind.as_str() {
                    "sync" => "settings.historyKindSync",
                    "backup" => "settings.historyKindBackup",
                    _ => "settings.historyKindSync",
                });
                let status_label = self.tr(match entry.status.as_str() {
                    "success" => "settings.syncState.success",
                    "failed" => "settings.syncState.failed",
                    "conflict" => "settings.syncState.conflict",
                    "running" => "settings.syncState.running",
                    _ => "settings.syncState.idle",
                });
                let trigger_label = self
                    .tr("settings.historyTrigger")
                    .replace("{{value}}", &entry.trigger);
                let provider = entry
                    .provider
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .map(format_cloud_provider)
                    .unwrap_or_else(|| "-".to_string());
                let provider_label = self
                    .tr("settings.historyProvider")
                    .replace("{{value}}", &provider);
                let duration =
                    format_duration_ms(entry.duration_ms).unwrap_or_else(|| "-".to_string());
                let duration_label = self
                    .tr("settings.historyDuration")
                    .replace("{{value}}", &duration);
                rows = rows.child(cloud_sync_history_row(
                    palette,
                    entry,
                    CloudSyncHistoryRowLabels {
                        kind: kind_label.to_string(),
                        status: status_label.to_string(),
                        trigger: trigger_label,
                        provider: provider_label,
                        duration: duration_label,
                        revision: self.tr("settings.historyRevision"),
                        view_details: self.tr("settings.historyViewDetails"),
                        hide_details: self.tr("settings.historyHideDetails"),
                        copy_message: self.tr("settings.historyCopyMessage"),
                    },
                    is_open,
                    cx.listener(move |this, _, _, cx| {
                        this.toggle_cloud_sync_history_details(&entry_id, cx);
                    }),
                    cx.listener(move |this, _, _, cx| {
                        if copy_message.trim().is_empty() {
                            this.shell
                                .set_status("history entry has no message".to_string());
                        } else {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy_message.clone()));
                            this.shell
                                .set_status("sync history message copied".to_string());
                        }
                        cx.notify();
                    }),
                ));
            }
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(self.shell_transparent_color(palette.surface))
            .child(
                div()
                    .flex_none()
                    .px_3()
                    .py(px(10.))
                    .border_b_1()
                    .border_color(rgb(palette.border))
                    .bg(self.shell_transparent_color(palette.surface))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .min_w_0()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .size(px(8.))
                                    .rounded_full()
                                    .flex_none()
                                    .bg(cloud_sync_status_dot_color(palette, state)),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(palette.text_muted))
                                    .child(self.tr("settings.historyCurrentState")),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight(600.))
                                    .text_color(cloud_sync_status_text_color(palette, state))
                                    .overflow_hidden()
                                    .child(state_label),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(palette.border))
                                    .child("·"),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .text_size(px(11.))
                                    .text_color(rgb(palette.text_muted))
                                    .overflow_hidden()
                                    .child(provider_label),
                            )
                            .child(sync_history_action_button(
                                palette,
                                "sync-history-push-now",
                                "icons/fe/upload.svg",
                                self.tr("settings.syncPushNow"),
                                sync_action_enabled,
                                cx.listener(move |this, _, window, cx| {
                                    if !this.cloud_sync.settings().enabled
                                        || this.cloud_sync.job_running()
                                    {
                                        this.shell.set_status(
                                            "cloud sync is disabled or already running".to_string(),
                                        );
                                        cx.notify();
                                        return;
                                    }
                                    this.prompt_provider_cloud_sync_push(window, cx);
                                }),
                            ))
                            .child(sync_history_action_button(
                                palette,
                                "sync-history-pull-now",
                                "icons/fe/download.svg",
                                self.tr("settings.syncPullNow"),
                                sync_action_enabled,
                                cx.listener(move |this, _, window, cx| {
                                    if !this.cloud_sync.settings().enabled
                                        || this.cloud_sync.job_running()
                                    {
                                        this.shell.set_status(
                                            "cloud sync is disabled or already running".to_string(),
                                        );
                                        cx.notify();
                                        return;
                                    }
                                    this.prompt_provider_cloud_sync_pull(window, cx);
                                }),
                            )),
                    )
                    .when(
                        !status_message.trim().is_empty() && conflict.is_none(),
                        |this| {
                            this.child(
                                div()
                                    .pl_4()
                                    .text_size(px(12.))
                                    .line_height(px(18.))
                                    .text_color(rgb(palette.text_muted))
                                    .child(truncate_preview(&status_message, 140)),
                            )
                        },
                    ),
            )
            .when_some(conflict, |this, conflict| {
                let preview = conflict.preview;
                let remote_inconsistent = preview.kind == CloudConflictKind::RemoteInconsistent;
                let recovery_revision = preview.recovery_revision.clone();
                this.child(
                    div()
                        .flex_none()
                        .m_2()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(palette.warning))
                        .bg(rgb(palette.input))
                        .child(
                            div()
                                .px_3()
                                .py_2()
                                .border_b_1()
                                .border_color(rgb(palette.warning))
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .font_weight(FontWeight(700.))
                                        .text_color(rgb(palette.warning))
                                        .child(if remote_inconsistent {
                                            self.tr("settings.syncRemoteIncompleteTitle")
                                        } else {
                                            self.tr("settings.syncConflictTitle")
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .px_3()
                                .py_2()
                                .text_size(px(11.))
                                .text_color(rgb(palette.text))
                                .child(preview.message.clone()),
                        )
                        .child(
                            div()
                                .px_3()
                                .pb_2()
                                .grid()
                                .grid_cols(1)
                                .gap_2()
                                .child(
                                    div()
                                        .rounded_md()
                                        .border_1()
                                        .border_color(rgb(palette.border))
                                        .bg(rgb(palette.input))
                                        .px_2()
                                        .py_1()
                                        .child(
                                            div()
                                                .text_size(px(10.))
                                                .text_color(rgb(palette.text_muted))
                                                .child(self.tr("settings.providerLabel")),
                                        )
                                        .child(
                                            div()
                                                .mt_0()
                                                .font_family(
                                                    crate::features::shell::gpui_code_font_family(),
                                                )
                                                .text_size(px(11.))
                                                .text_color(rgb(palette.text))
                                                .child(format_cloud_provider(&preview.provider)),
                                        ),
                                )
                                .when_some(recovery_revision, |this, revision| {
                                    this.child(
                                        div()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(rgb(palette.border))
                                            .bg(rgb(palette.input))
                                            .px_2()
                                            .py_1()
                                            .child(
                                                div()
                                                    .text_size(px(10.))
                                                    .text_color(rgb(palette.text_muted))
                                                    .child(
                                                        self.tr("settings.currentRemoteSnapshot"),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .font_family(
                                                        crate::features::shell::gpui_code_font_family(),
                                                    )
                                                    .text_size(px(11.))
                                                    .text_color(rgb(palette.text))
                                                    .child(truncate_preview(&revision, 16)),
                                            ),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .px_3()
                                .pb_3()
                                .flex()
                                .gap_2()
                                .child(if remote_inconsistent {
                                    small_button(
                                        palette,
                                        "sync-panel-recover-current",
                                        self.tr("settings.useCurrentRemoteSnapshot"),
                                        cx.listener({
                                            let provider_action = conflict.provider_action;
                                            move |this, _, window, cx| {
                                                this.prompt_cloud_sync_recover_current(
                                                    provider_action,
                                                    window,
                                                    cx,
                                                );
                                            }
                                        }),
                                    )
                                    .into_any_element()
                                } else {
                                    small_button(
                                        palette,
                                        "sync-panel-force-pull",
                                        self.tr("settings.downloadRemoteVersion"),
                                        cx.listener({
                                            let provider_action = conflict.provider_action;
                                            move |this, _, window, cx| {
                                                this.prompt_cloud_sync_force_pull(
                                                    provider_action,
                                                    window,
                                                    cx,
                                                );
                                            }
                                        }),
                                    )
                                    .into_any_element()
                                })
                                .child(dialog_action_button(
                                    palette,
                                    "sync-panel-force-push",
                                    self.tr("settings.uploadLocalVersion"),
                                    false,
                                    cx.listener({
                                        let provider_action = conflict.provider_action;
                                        move |this, _, window, cx| {
                                            this.prompt_cloud_sync_force_push(
                                                provider_action,
                                                window,
                                                cx,
                                            );
                                        }
                                    }),
                                )),
                        ),
                )
            })
            .child(
                div()
                    .id(SharedString::from("sync-backup-history-list"))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(rows),
            )
    }
}

fn sync_history_action_button(
    palette: crate::theme::ThemePalette,
    id: impl Into<String>,
    icon_path: &'static str,
    tooltip: &'static str,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(SharedString::from(id.into()))
        .size(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .opacity(if enabled { 1.0 } else { 0.45 })
        .cursor_pointer()
        .tooltip(move |window, cx| NyaTooltip::new(tooltip).build(window, cx))
        .hover(|this| {
            if enabled {
                this.bg(rgb(palette.surface_elevated))
            } else {
                this
            }
        })
        .child(
            svg()
                .size(px(14.))
                .flex_none()
                .path(icon_path)
                .text_color(rgb(palette.text_muted)),
        )
        .on_click(on_click)
}
