use rust_i18n::t;

use std::borrow::Cow;

use gpui::{
    Context, IntoElement, ScrollDelta, ScrollWheelEvent, SharedString, div, prelude::*, px, rgb,
    rgba,
};
use nyaterm_transport::{PROCESS_LIST_UNSUPPORTED_ERROR, RemoteProcess};
use nyaterm_ui::NyaNumberInputOptions;

use crate::features::remote::PROCESS_VIEWPORT_ROWS;
use crate::features::{NyaTermApp, text_inputs::TextInputSetup};
use crate::models::RemoteProcessSortKey;
use crate::widgets::empty_panel;

use super::process::{
    ProcessDetailLabels, ProcessDisplayMode, ProcessTableLabels, ProcessTableRowActions,
    ProcessTableRowPresentation, process_details, process_details_height_px, process_display_mode,
    process_row_height_px, process_sort_button, process_table_row,
};

impl NyaTermApp {
    pub(in crate::features) fn processes_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let process_state = self.remote_ops.process_presentation();
        let palette = self.theme_palette();
        // Built before the view, which reads `self` throughout: creating the
        // box needs it mutably.
        let process_search_input = self
            .search_input_box(
                "remote.process.filter",
                &process_state.search_draft.clone(),
                TextInputSetup::placeholder(t!("processManager.search")),
                cx,
            )
            .into_any_element();
        if self.session.active_ssh_config().is_none() {
            return div()
                .size_full()
                .bg(self.shell_transparent_color(palette.surface))
                .child(empty_panel(t!("processManager.noSession"), palette));
        }
        if !process_state.snapshot_loaded {
            let message = if process_state.pending || !process_state.status.contains("failed") {
                t!("common.loading")
            } else if process_state
                .status
                .contains(PROCESS_LIST_UNSUPPORTED_ERROR)
            {
                t!("processManager.unsupported")
            } else {
                t!("processManager.error")
            };
            return div()
                .size_full()
                .bg(self.shell_transparent_color(palette.surface))
                .child(empty_panel(message, palette));
        }
        let menu_bg = self.shell_surface_color(palette.surface);
        let table_labels = ProcessTableLabels {
            more: t!("common.more"),
            copy_pid: t!("processManager.copyPid"),
            copy_command: t!("processManager.copyCommand"),
            signal_term: t!("processManager.signalTerm"),
            signal_hup: t!("processManager.signalHup"),
            signal_stop: t!("processManager.signalStop"),
            signal_cont: t!("processManager.signalCont"),
            signal_kill: t!("processManager.signalKill"),
        };
        let detail_labels = ProcessDetailLabels {
            cpu: t!("processManager.sortCpu"),
            memory: t!("resourceMonitor.memory"),
            rss: Cow::Borrowed("RSS"),
            elapsed: t!("processManager.elapsed"),
            copy_command: t!("processManager.copyCommand"),
            apply_nice: t!("processManager.applyNice"),
        };
        let mode = process_display_mode(self.shell.right_panel_width());
        // The sort key arrives already constrained to the columns this width can show,
        // and the list already filtered and sorted: `RemoteOpsFeatureState` reconciles
        // both when the data, the query, the sort or the panel width changes. This pass
        // only reads them.
        let filtered_processes = self.remote_ops.derived_processes();

        // Tauri-like virtual list: base row + expanded details height, spacer padding.
        let process_row_px = process_row_height_px(mode);
        let process_details_px = process_details_height_px(mode);
        const PROCESS_OVERSCAN: usize = 8;
        let selected_pid = process_state.selected_pid;
        let row_height = |process: &RemoteProcess| -> f32 {
            if selected_pid == Some(process.pid) {
                process_row_px + process_details_px
            } else {
                process_row_px
            }
        };
        let total_filtered = filtered_processes.len();
        let window_capacity = PROCESS_VIEWPORT_ROWS + PROCESS_OVERSCAN * 2;
        let scroll_row = process_state.list_offset;
        let window_start = scroll_row.saturating_sub(PROCESS_OVERSCAN);
        let window_end = (window_start + window_capacity).min(total_filtered);
        let visible_processes = filtered_processes
            .get(window_start..window_end)
            .unwrap_or(&[])
            .to_vec();
        let pad_top = filtered_processes
            .iter()
            .take(window_start)
            .map(row_height)
            .sum::<f32>();
        let pad_bottom = filtered_processes
            .iter()
            .skip(window_end)
            .map(row_height)
            .sum::<f32>();

        let selected_process = process_state
            .selected_pid
            .and_then(|pid| {
                process_state
                    .items
                    .iter()
                    .find(|process| process.pid == pid)
            })
            .cloned();
        // Built before the rows, which borrow `self`: the nice box is a real
        // input and creating one needs the app mutably.
        let mut nice_input = selected_process.as_ref().map(|process| {
            self.number_input_box(
                format!("remote.process.{}.nice", process.pid),
                &process_state.nice_draft.clone(),
                NyaNumberInputOptions::default().range(-20.0, 19.0),
                cx,
            )
            .into_any_element()
        });

        let mut rows = div().flex().flex_col();
        if filtered_processes.is_empty() {
            rows = rows.child(empty_panel(
                t!("processManager.noMatches"),
                self.theme_palette(),
            ));
        } else {
            if pad_top > 0. {
                rows = rows.child(div().h(px(pad_top)).w_full().flex_none());
            }
            for process in visible_processes.iter() {
                let pid = process.pid;
                let selected = process_state.selected_pid == Some(pid);
                rows = rows.child(
                    process_table_row(
                        ProcessTableRowPresentation {
                            palette,
                            menu_bg,
                            mode,
                            labels: table_labels.clone(),
                            selected,
                            menu_open: process_state.menu_pid == Some(pid),
                        },
                        process,
                        ProcessTableRowActions {
                            on_select: cx.listener(move |this, _, _, cx| {
                                this.remote_ops.close_process_menu();
                                this.toggle_process_selection(pid, cx);
                            }),
                            on_menu: cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.remote_ops.toggle_process_menu(pid);
                                cx.notify();
                            }),
                            on_copy_pid: cx.listener({
                                let value = pid.to_string();
                                move |this, _, _, cx| {
                                    this.remote_ops.close_process_menu();
                                    this.copy_process_text(value.clone(), "pid", cx);
                                }
                            }),
                            on_copy_command: cx.listener({
                                let value = if process.command_line.trim().is_empty() {
                                    process.command.clone()
                                } else {
                                    process.command_line.clone()
                                };
                                move |this, _, _, cx| {
                                    this.remote_ops.close_process_menu();
                                    this.copy_process_text(value.clone(), "command", cx);
                                }
                            }),
                            on_term: cx.listener(move |this, _, window, cx| {
                                this.remote_ops.close_process_menu();
                                this.request_process_signal(pid, "TERM", window, cx);
                            }),
                            on_hup: cx.listener(move |this, _, window, cx| {
                                this.remote_ops.close_process_menu();
                                this.request_process_signal(pid, "HUP", window, cx);
                            }),
                            on_stop: cx.listener(move |this, _, window, cx| {
                                this.remote_ops.close_process_menu();
                                this.request_process_signal(pid, "STOP", window, cx);
                            }),
                            on_cont: cx.listener(move |this, _, window, cx| {
                                this.remote_ops.close_process_menu();
                                this.request_process_signal(pid, "CONT", window, cx);
                            }),
                            on_kill: cx.listener(move |this, _, window, cx| {
                                this.remote_ops.close_process_menu();
                                this.request_process_signal(pid, "KILL", window, cx);
                            }),
                        },
                    )
                    .child(
                        selected_process
                            .as_ref()
                            .filter(|selected_process| selected_process.pid == pid)
                            .map(|selected_process| {
                                process_details(
                                    palette,
                                    selected_process,
                                    mode,
                                    detail_labels.clone(),
                                    nice_input.take(),
                                    cx.listener({
                                        let value =
                                            if selected_process.command_line.trim().is_empty() {
                                                selected_process.command.clone()
                                            } else {
                                                selected_process.command_line.clone()
                                            };
                                        move |this, _, _, cx| {
                                            this.copy_process_text(value.clone(), "command", cx);
                                        }
                                    }),
                                    cx,
                                )
                            })
                            .unwrap_or_else(|| div().into_any_element()),
                    ),
                );
            }
            if pad_bottom > 0. {
                rows = rows.child(div().h(px(pad_bottom)).w_full().flex_none());
            }
        }

        // Tauri ProcessManager shell: dense search toolbar + sort strip + scrollable table.
        let count_label = process_state.items.len().to_string();
        div()
            .flex()
            .flex_col()
            .size_full()
            .relative()
            .overflow_hidden()
            .p(px(10.))
            .gap(px(10.))
            .bg(self.shell_transparent_color(palette.surface))
            .child(
                div()
                    .h(px(32.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(process_search_input))
                    .child(
                        div()
                            .h(px(32.))
                            .px_2()
                            .rounded_md()
                            .border_1()
                            .border_color(rgba((palette.link << 8) | 0x4d))
                            .bg(rgba((palette.link << 8) | 0x1a))
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(palette.link))
                                    .child(count_label),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .flex()
                    .flex_col()
                    .child(
                        // Match Tauri's responsive columns and hide the header entirely in compact mode.
                        div().when(mode != ProcessDisplayMode::Compact, |this| {
                            let cols = match mode {
                                ProcessDisplayMode::Narrow => 4,
                                ProcessDisplayMode::Medium => 5,
                                _ => 6,
                            };
                            this.h(px(32.))
                                .flex_none()
                                .px_2()
                                .border_b_1()
                                .border_color(rgb(palette.border))
                                .grid()
                                .grid_cols(cols)
                                .gap_1()
                                .items_center()
                                .overflow_hidden()
                                .child(process_sort_button(
                                    palette,
                                    "process-sort-command",
                                    t!("processManager.process"),
                                    process_state.sort_key == RemoteProcessSortKey::Command,
                                    process_state.sort_direction,
                                    false,
                                    cx.listener(|this, _, _, cx| {
                                        this.toggle_process_sort(RemoteProcessSortKey::Command, cx);
                                    }),
                                ))
                                .child(process_sort_button(
                                    palette,
                                    "process-sort-pid",
                                    t!("processManager.sortPid"),
                                    process_state.sort_key == RemoteProcessSortKey::Pid,
                                    process_state.sort_direction,
                                    true,
                                    cx.listener(|this, _, _, cx| {
                                        this.toggle_process_sort(RemoteProcessSortKey::Pid, cx);
                                    }),
                                ))
                                .child(process_sort_button(
                                    palette,
                                    "process-sort-cpu",
                                    t!("processManager.sortCpu"),
                                    process_state.sort_key == RemoteProcessSortKey::Cpu,
                                    process_state.sort_direction,
                                    true,
                                    cx.listener(|this, _, _, cx| {
                                        this.toggle_process_sort(RemoteProcessSortKey::Cpu, cx);
                                    }),
                                ))
                                .when(
                                    !matches!(
                                        mode,
                                        ProcessDisplayMode::Narrow | ProcessDisplayMode::Compact
                                    ),
                                    |this| {
                                        this.child(process_sort_button(
                                            palette,
                                            "process-sort-memory",
                                            t!("processManager.sortMemory"),
                                            process_state.sort_key == RemoteProcessSortKey::Memory,
                                            process_state.sort_direction,
                                            true,
                                            cx.listener(|this, _, _, cx| {
                                                this.toggle_process_sort(
                                                    RemoteProcessSortKey::Memory,
                                                    cx,
                                                );
                                            }),
                                        ))
                                    },
                                )
                                .when(mode == ProcessDisplayMode::Wide, |this| {
                                    this.child(process_sort_button(
                                        palette,
                                        "process-sort-user",
                                        t!("processManager.user"),
                                        process_state.sort_key == RemoteProcessSortKey::User,
                                        process_state.sort_direction,
                                        false,
                                        cx.listener(|this, _, _, cx| {
                                            this.toggle_process_sort(
                                                RemoteProcessSortKey::User,
                                                cx,
                                            );
                                        }),
                                    ))
                                })
                                .child(div().w_full())
                        }),
                    )
                    .child(
                        div()
                            .id(SharedString::from("process-list-scroll"))
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            .on_scroll_wheel(cx.listener(
                                move |this, event: &ScrollWheelEvent, _, cx| {
                                    let max_offset = total_filtered
                                        .saturating_sub(PROCESS_VIEWPORT_ROWS.min(total_filtered));
                                    if max_offset == 0 {
                                        return;
                                    }
                                    let delta_rows = match event.delta {
                                        ScrollDelta::Lines(delta) => delta.y,
                                        ScrollDelta::Pixels(delta) => {
                                            f32::from(delta.y) / process_row_px
                                        }
                                    };
                                    // Match GPUI list semantics: scroll_top -= delta.y
                                    let current =
                                        this.remote_ops.process_presentation().list_offset;
                                    let next = (current as f32 - delta_rows)
                                        .round()
                                        .clamp(0., max_offset as f32)
                                        as usize;
                                    if this.remote_ops.set_process_list_offset(next) {
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                },
                            ))
                            .child(rows),
                    ),
            )
    }
}
