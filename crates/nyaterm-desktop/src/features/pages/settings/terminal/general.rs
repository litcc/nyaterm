use std::borrow::Cow;

use gpui::{
    App, ClickEvent, Context, FontWeight, IntoElement, SharedString, Window, div, prelude::*, px,
    rgb,
};
use nyaterm_ui::{NyaNumberInputOptions, NyaSelectOption};

use crate::features::{NyaTermApp, text_inputs::TextInputSetup};
use crate::theme::ThemePalette;

use super::super::{
    settings_form_row, settings_form_section, settings_switch, settings_switch_with_enabled,
};

impl NyaTermApp {
    pub(in crate::features) fn terminal_general_settings_section(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = self.theme_palette();
        let x11_display_input = self
            .text_input_box(
                "settings.terminal.x11-display",
                &self.settings.summary().x11_display.clone(),
                TextInputSetup::placeholder(self.tr("settings.x11DisplayPlaceholder")),
                cx,
            )
            .into_any_element();
        let timestamp_format_input = self
            .text_input_box(
                "settings.terminal.timestamp-format",
                &self.settings.summary().terminal_timestamp_format.clone(),
                TextInputSetup::placeholder("[HH:mm:ss]"),
                cx,
            )
            .into_any_element();
        let action_links_enabled = self.settings.summary().terminal_action_links_enabled;

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(settings_form_section(
                palette,
                None,
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(settings_form_row(
                        palette,
                        self.tr("settings.scrollbackLines"),
                        Some(SharedString::from(self.tr("settings.scrollbackLinesDesc"))),
                        self.number_input_box(
                            "settings.number.terminal-scrollback-lines",
                            self.settings
                                .summary()
                                .terminal_scrollback_lines
                                .to_string()
                                .as_str(),
                            NyaNumberInputOptions::default()
                                .range(100.0, 100_000.0)
                                .step(100.0),
                            cx,
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        self.tr("settings.keepAliveMode"),
                        Some(SharedString::from(
                            self.tr("settings.keepAliveModeCompatibleDescription"),
                        )),
                        self.settings_select_control(
                            "settings.terminal.keep-alive-mode",
                            vec![
                                NyaSelectOption::new(
                                    "compatible",
                                    self.tr("settings.keepAliveModeCompatible"),
                                ),
                                NyaSelectOption::new(
                                    "strict",
                                    self.tr("settings.keepAliveModeStrict"),
                                ),
                                NyaSelectOption::new(
                                    "disabled",
                                    self.tr("settings.keepAliveModeDisabled"),
                                ),
                            ],
                            self.settings.summary().terminal_keep_alive_mode.clone(),
                            false,
                            cx,
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        self.tr("settings.keepAliveInterval"),
                        Some(SharedString::from(
                            self.tr("settings.keepAliveIntervalDesc"),
                        )),
                        self.number_input_box(
                            "settings.number.terminal-keep-alive-interval",
                            self.settings
                                .summary()
                                .terminal_keep_alive_interval
                                .to_string()
                                .as_str(),
                            NyaNumberInputOptions::default().range(0.0, 600.0).step(5.0),
                            cx,
                        ),
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(terminal_settings_field_meta(
                                palette,
                                self.tr("settings.x11Display"),
                                self.tr("settings.x11DisplayDesc"),
                            ))
                            .child(div().w_full().max_w(px(520.)).child(x11_display_input)),
                    )
                    .child(settings_form_row(
                        palette,
                        self.tr("settings.hardwareAcceleration"),
                        Some(SharedString::from(
                            self.tr("settings.hardwareAccelerationDesc"),
                        )),
                        settings_switch(
                            palette,
                            "terminal-hardware-acceleration",
                            self.settings.summary().terminal_hardware_acceleration,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_terminal_hardware_acceleration(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        self.tr("settings.lowLatencyMode"),
                        Some(SharedString::from(self.tr("settings.lowLatencyModeDesc"))),
                        settings_switch(
                            palette,
                            "terminal-low-latency-mode",
                            self.settings.summary().terminal_low_latency_mode,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_terminal_low_latency_mode(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        self.tr("settings.zebraStripes"),
                        Some(SharedString::from(self.tr("settings.zebraStripesDesc"))),
                        settings_switch(
                            palette,
                            "terminal-zebra-stripes",
                            self.settings.summary().terminal_zebra_stripes_enabled,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_terminal_zebra_stripes(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        self.tr("settings.showWorkspacePadding"),
                        Some(SharedString::from(
                            self.tr("settings.showWorkspacePaddingDesc"),
                        )),
                        settings_switch(
                            palette,
                            "terminal-workspace-padding",
                            self.settings.summary().terminal_show_workspace_padding,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_terminal_workspace_padding(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        self.tr("settings.showLineNumbers"),
                        Some(SharedString::from(self.tr("settings.showLineNumbersDesc"))),
                        settings_switch(
                            palette,
                            "terminal-line-numbers",
                            self.settings.summary().terminal_show_line_numbers,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_terminal_line_numbers(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        self.tr("settings.showTimestamps"),
                        Some(SharedString::from(self.tr("settings.showTimestampsDesc"))),
                        settings_switch(
                            palette,
                            "terminal-timestamps",
                            self.settings.summary().terminal_show_timestamps,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_terminal_timestamps(cx);
                            }),
                        ),
                    ))
                    .when(self.settings.summary().terminal_show_timestamps, |this| {
                        this.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(terminal_settings_field_meta(
                                    palette,
                                    self.tr("settings.timestampFormat"),
                                    self.tr("settings.timestampFormatDesc"),
                                ))
                                .child(
                                    div().w_full().max_w(px(520.)).child(timestamp_format_input),
                                ),
                        )
                    })
                    .child(settings_form_row(
                        palette,
                        self.tr("terminal.showMultiLinePasteDialog"),
                        Some(SharedString::from(
                            self.tr("terminal.showMultiLinePasteDialogDesc"),
                        )),
                        settings_switch(
                            palette,
                            "terminal-multi-line-paste",
                            self.settings
                                .summary()
                                .terminal_show_multi_line_paste_dialog,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_multi_line_paste_dialog(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        self.tr("terminal.pasteImageAsPath"),
                        Some(SharedString::from(self.tr("terminal.pasteImageAsPathDesc"))),
                        settings_switch(
                            palette,
                            "terminal-paste-image-path",
                            self.settings.summary().terminal_paste_image_as_path,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_paste_image_as_path(cx);
                            }),
                        ),
                    ))
                    .child(settings_form_row(
                        palette,
                        self.tr("settings.showRemoteStats"),
                        Some(SharedString::from(self.tr("settings.showRemoteStatsDesc"))),
                        settings_switch(
                            palette,
                            "terminal-remote-stats",
                            self.settings.summary().ui_show_remote_stats,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_remote_stats_panel(cx);
                            }),
                        ),
                    ))
                    .when(self.settings.summary().ui_show_remote_stats, |this| {
                        this.child(settings_form_row(
                            palette,
                            self.tr("settings.remoteStatsInterval"),
                            Some(SharedString::from(
                                self.tr("settings.remoteStatsIntervalDesc"),
                            )),
                            self.number_input_box(
                                "settings.number.remote-stats-interval",
                                self.settings
                                    .summary()
                                    .ui_remote_stats_interval
                                    .to_string()
                                    .as_str(),
                                NyaNumberInputOptions::default().range(1.0, 60.0).step(1.0),
                                cx,
                            ),
                        ))
                    })
                    .child(settings_form_row(
                        palette,
                        self.tr("settings.showGpuMonitor"),
                        Some(SharedString::from(self.tr("settings.showGpuMonitorDesc"))),
                        settings_switch(
                            palette,
                            "terminal-gpu-monitor",
                            self.settings.summary().ui_show_gpu_monitor,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_gpu_monitor_panel(cx);
                            }),
                        ),
                    ))
                    .when(self.settings.summary().ui_show_gpu_monitor, |this| {
                        this.child(settings_form_row(
                            palette,
                            self.tr("settings.gpuMonitorInterval"),
                            Some(SharedString::from(
                                self.tr("settings.gpuMonitorIntervalDesc"),
                            )),
                            self.number_input_box(
                                "settings.number.gpu-monitor-interval",
                                self.settings
                                    .summary()
                                    .ui_gpu_monitor_interval
                                    .to_string()
                                    .as_str(),
                                NyaNumberInputOptions::default().range(3.0, 120.0).step(1.0),
                                cx,
                            ),
                        ))
                    })
                    .child(settings_form_row(
                        palette,
                        self.tr("settings.showAscendNpuMonitor"),
                        Some(SharedString::from(
                            self.tr("settings.showAscendNpuMonitorDesc"),
                        )),
                        settings_switch(
                            palette,
                            "terminal-ascend-npu-monitor",
                            self.settings.summary().ui_show_ascend_npu_monitor,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_ascend_npu_monitor_panel(cx);
                            }),
                        ),
                    ))
                    .when(self.settings.summary().ui_show_ascend_npu_monitor, |this| {
                        this.child(settings_form_row(
                            palette,
                            self.tr("settings.ascendNpuMonitorInterval"),
                            Some(SharedString::from(
                                self.tr("settings.ascendNpuMonitorIntervalDesc"),
                            )),
                            self.number_input_box(
                                "settings.number.ascend-npu-monitor-interval",
                                self.settings
                                    .summary()
                                    .ui_ascend_npu_monitor_interval
                                    .to_string()
                                    .as_str(),
                                NyaNumberInputOptions::default().range(3.0, 120.0).step(1.0),
                                cx,
                            ),
                        ))
                    })
                    .child(settings_form_row(
                        palette,
                        self.tr("settings.showProcessManager"),
                        Some(SharedString::from(
                            self.tr("settings.showProcessManagerDesc"),
                        )),
                        settings_switch(
                            palette,
                            "terminal-process-manager",
                            self.settings.summary().ui_show_process_manager,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_process_manager_panel(cx);
                            }),
                        ),
                    ))
                    .when(self.settings.summary().ui_show_process_manager, |this| {
                        this.child(settings_form_row(
                            palette,
                            self.tr("settings.processManagerInterval"),
                            Some(SharedString::from(
                                self.tr("settings.processManagerIntervalDesc"),
                            )),
                            self.number_input_box(
                                "settings.number.process-manager-interval",
                                self.settings
                                    .summary()
                                    .ui_process_manager_interval
                                    .to_string()
                                    .as_str(),
                                NyaNumberInputOptions::default().range(3.0, 120.0).step(1.0),
                                cx,
                            ),
                        ))
                    })
                    .child(settings_form_row(
                        palette,
                        self.tr("settings.showDockerManager"),
                        Some(SharedString::from(
                            self.tr("settings.showDockerManagerDesc"),
                        )),
                        settings_switch(
                            palette,
                            "terminal-docker-manager",
                            self.settings.summary().ui_show_docker_manager,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_docker_manager_panel(cx);
                            }),
                        ),
                    ))
                    .when(self.settings.summary().ui_show_docker_manager, |this| {
                        this.child(settings_form_row(
                            palette,
                            self.tr("settings.dockerManagerInterval"),
                            Some(SharedString::from(
                                self.tr("settings.dockerManagerIntervalDesc"),
                            )),
                            self.number_input_box(
                                "settings.number.docker-manager-interval",
                                self.settings
                                    .summary()
                                    .ui_docker_manager_interval
                                    .to_string()
                                    .as_str(),
                                NyaNumberInputOptions::default().range(3.0, 120.0).step(1.0),
                                cx,
                            ),
                        ))
                    }),
            ))
            .child(settings_form_section(
                palette,
                None,
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(settings_form_row(
                        palette,
                        self.tr("settings.actionLinks"),
                        Some(SharedString::from(self.tr("settings.actionLinksDesc"))),
                        settings_switch(
                            palette,
                            "terminal-action-links",
                            action_links_enabled,
                            cx.listener(|this, _, _, cx| {
                                this.toggle_terminal_action_links(cx);
                            }),
                        ),
                    ))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight(500.))
                            .text_color(rgb(palette.text))
                            .child(self.tr("settings.actionLinksMatchers")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(terminal_action_matcher_row(
                                palette,
                                TerminalActionMatcherPresentation {
                                    id: "terminal-action-links-ipv4",
                                    label: self.tr("settings.actionLinksMatcherIpv4"),
                                    example: Cow::Borrowed("192.168.1.1"),
                                    description: self.tr("settings.actionLinksMatcherIpv4Desc"),
                                    checked: self
                                        .settings
                                        .summary()
                                        .terminal_action_links_matchers
                                        .ipv4,
                                    enabled: action_links_enabled,
                                },
                                cx.listener(|this, _, _, cx| {
                                    this.toggle_terminal_action_links_matcher("ipv4", cx);
                                }),
                            ))
                            .child(terminal_action_matcher_row(
                                palette,
                                TerminalActionMatcherPresentation {
                                    id: "terminal-action-links-host-port",
                                    label: self.tr("settings.actionLinksMatcherHostPort"),
                                    example: Cow::Borrowed("localhost:8080"),
                                    description: self.tr("settings.actionLinksMatcherHostPortDesc"),
                                    checked: self
                                        .settings
                                        .summary()
                                        .terminal_action_links_matchers
                                        .host_port,
                                    enabled: action_links_enabled,
                                },
                                cx.listener(|this, _, _, cx| {
                                    this.toggle_terminal_action_links_matcher("host_port", cx);
                                }),
                            ))
                            .child(terminal_action_matcher_row(
                                palette,
                                TerminalActionMatcherPresentation {
                                    id: "terminal-action-links-archive",
                                    label: self.tr("settings.actionLinksMatcherArchive"),
                                    example: Cow::Borrowed("backup.tar.gz"),
                                    description: self.tr("settings.actionLinksMatcherArchiveDesc"),
                                    checked: self
                                        .settings
                                        .summary()
                                        .terminal_action_links_matchers
                                        .archive,
                                    enabled: action_links_enabled,
                                },
                                cx.listener(|this, _, _, cx| {
                                    this.toggle_terminal_action_links_matcher("archive", cx);
                                }),
                            )),
                    ),
            ))
            .child(self.keyword_highlights_settings_section(cx))
    }
}

fn terminal_settings_field_meta(
    palette: ThemePalette,
    label: impl Into<SharedString>,
    desc: impl Into<SharedString>,
) -> impl IntoElement {
    let label: SharedString = label.into();
    let desc: SharedString = desc.into();
    div()
        .min_w_0()
        .child(
            div()
                .text_size(px(13.))
                .font_weight(FontWeight(500.))
                .text_color(rgb(palette.text))
                .child(label),
        )
        .child(
            div()
                .mt_1()
                .text_size(px(11.))
                .text_color(rgb(palette.text_dimmed))
                .child(desc),
        )
}

fn terminal_action_matcher_row(
    palette: ThemePalette,
    presentation: TerminalActionMatcherPresentation,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let TerminalActionMatcherPresentation {
        id,
        label,
        example,
        description,
        checked,
        enabled,
    } = presentation;
    div()
        .rounded_md()
        .border_1()
        .border_color(rgb(palette.border))
        .bg(rgb(palette.input))
        .px_3()
        .py_2()
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .child(
            div()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(12.))
                                .font_weight(FontWeight(500.))
                                .text_color(rgb(palette.text))
                                .child(label),
                        )
                        .child(
                            div()
                                .px_2()
                                .py(px(1.))
                                .rounded_sm()
                                .bg(rgb(palette.surface_elevated))
                                .font_family(crate::features::shell::gpui_code_font_family())
                                .text_size(px(10.))
                                .text_color(rgb(palette.text_muted))
                                .child(example),
                        ),
                )
                .child(
                    div()
                        .mt_1()
                        .text_size(px(11.))
                        .text_color(rgb(palette.text_dimmed))
                        .child(description),
                ),
        )
        .child(settings_switch_with_enabled(
            palette, id, checked, enabled, on_click,
        ))
}

struct TerminalActionMatcherPresentation {
    id: &'static str,
    label: Cow<'static, str>,
    example: Cow<'static, str>,
    description: Cow<'static, str>,
    checked: bool,
    enabled: bool,
}
