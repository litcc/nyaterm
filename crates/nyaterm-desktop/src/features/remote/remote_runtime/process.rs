use rust_i18n::t;

use gpui::{ClipboardItem, Context, Window};
use nyaterm_transport::SshProcessService;

use crate::features::NyaTermApp;
use crate::features::runtime_jobs::{ProcessJobOutput, ProcessJobResult};
use crate::models::{DockerTab, RemoteProcessSortKey};

const PROCESS_EVENT_DRAIN_LIMIT: usize = 8;

impl NyaTermApp {
    pub(in crate::features) fn set_docker_tab(&mut self, tab: DockerTab, cx: &mut Context<Self>) {
        self.remote_ops.set_docker_tab(tab);
        cx.notify();
    }

    pub(in crate::features) fn toggle_docker_tab_menu(&mut self, cx: &mut Context<Self>) {
        self.remote_ops.toggle_docker_tab_menu();
        cx.notify();
    }

    /// Apply an edit from the Docker filter box.
    pub(in crate::features) fn apply_docker_search(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.remote_ops.apply_docker_search(text);
        cx.notify();
    }

    /// Apply an edit from the process filter box.
    pub(in crate::features) fn apply_process_search(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.remote_ops.apply_process_search(text);
        cx.notify();
    }

    pub(in crate::features) fn toggle_process_sort(
        &mut self,
        key: RemoteProcessSortKey,
        cx: &mut Context<Self>,
    ) {
        self.remote_ops.toggle_process_sort(key);
        cx.notify();
    }

    pub(in crate::features) fn toggle_process_selection(
        &mut self,
        pid: u32,
        cx: &mut Context<Self>,
    ) {
        self.remote_ops.toggle_process_selection(pid);
        cx.notify();
    }

    /// Apply an edit from the nice value box.
    ///
    /// A nice value is a small signed number, so the draft keeps a leading
    /// minus and up to three digits and drops everything else.
    pub(in crate::features) fn apply_process_nice_input(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.remote_ops.apply_process_nice_input(text);
        cx.notify();
    }

    pub(in crate::features) fn apply_process_nice_draft(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((pid, nice)) = self.remote_ops.validated_process_nice_draft() else {
            cx.notify();
            return;
        };
        self.renice_process(pid, nice, window, cx);
    }

    pub(in crate::features) fn copy_process_text(
        &mut self,
        value: String,
        label: &'static str,
        cx: &mut Context<Self>,
    ) {
        cx.write_to_clipboard(ClipboardItem::new_string(value));
        self.remote_ops
            .set_process_status(format!("copied process {label}"));
        self.shell
            .set_status(self.remote_ops.process_status().to_string());
        cx.notify();
    }

    pub(in crate::features) fn copy_docker_text(
        &mut self,
        value: String,
        label: &str,
        cx: &mut Context<Self>,
    ) {
        cx.write_to_clipboard(ClipboardItem::new_string(value));
        self.remote_ops
            .set_docker_status(format!("copied Docker {label}"));
        self.shell
            .set_status(self.remote_ops.docker_status().to_string());
        cx.notify();
    }

    pub(in crate::features) fn request_process_signal(
        &mut self,
        pid: u32,
        signal: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if signal != "KILL" {
            self.signal_process(pid, signal, window, cx);
            return;
        }
        let description = t!(
            "processManager.confirmSignalDesc",
            signal = signal,
            pid = pid,
            command = format!("kill -{signal} -- {pid}")
        )
        .to_string();
        self.open_confirm_dialog(
            (
                t!("processManager.confirmSignalTitle").to_string(),
                description,
                t!("common.confirm").to_string(),
                true,
                move |app, window, cx| {
                    app.signal_process(pid, signal, window, cx);
                    true
                },
            ),
            window,
            cx,
        );
    }

    pub(in crate::features) fn refresh_processes(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(config) = self.session.active_ssh_config_owned() else {
            self.remote_ops
                .set_process_status("start an SSH session before listing processes");
            self.shell
                .set_status(self.remote_ops.process_status().to_string());
            cx.notify();
            return;
        };
        let Some(job_session_id) = self.session.active_id_owned() else {
            self.remote_ops
                .set_process_status("start an SSH session before listing processes");
            cx.notify();
            return;
        };
        if self.remote_ops.process_is_pending_for(&job_session_id) {
            self.remote_ops
                .set_process_status("process operation already running");
            cx.notify();
            return;
        }

        let ticket = self.remote_ops.begin_process_job(job_session_id.clone());
        self.remote_ops.close_process_menu();
        self.remote_ops.mark_process_refresh_started();
        self.remote_ops
            .set_process_status("listing remote processes");
        std::thread::spawn(move || {
            let result = SshProcessService::new(config)
                .list_processes()
                .map(ProcessJobOutput::Listed)
                .map_err(|error| error.to_string());
            let _ = ticket.tx.send(ProcessJobResult {
                job_id: ticket.job_id,
                session_id: job_session_id,
                result,
            });
        });
        cx.notify();
    }

    pub(in crate::features) fn signal_process(
        &mut self,
        pid: u32,
        signal: &'static str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(config) = self.session.active_ssh_config_owned() else {
            self.remote_ops
                .set_process_status("start an SSH session before signalling processes");
            self.shell
                .set_status(self.remote_ops.process_status().to_string());
            cx.notify();
            return;
        };
        let Some(job_session_id) = self.session.active_id_owned() else {
            self.remote_ops
                .set_process_status("start an SSH session before signalling processes");
            cx.notify();
            return;
        };
        if self.remote_ops.process_is_pending_for(&job_session_id) {
            self.remote_ops
                .set_process_status("process operation already running");
            cx.notify();
            return;
        }

        let ticket = self.remote_ops.begin_process_job(job_session_id.clone());
        self.remote_ops
            .set_process_status(format!("sending {signal} to pid {pid}"));
        std::thread::spawn(move || {
            let result = (|| {
                let service = SshProcessService::new(config);
                service.signal_process(pid, signal)?;
                let processes = service.list_processes()?;
                Ok(ProcessJobOutput::Signalled {
                    pid,
                    signal: signal.to_string(),
                    processes,
                })
            })()
            .map_err(|error: anyhow::Error| error.to_string());
            let _ = ticket.tx.send(ProcessJobResult {
                job_id: ticket.job_id,
                session_id: job_session_id,
                result,
            });
        });
        cx.notify();
    }

    pub(in crate::features) fn renice_process(
        &mut self,
        pid: u32,
        nice: i32,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(config) = self.session.active_ssh_config_owned() else {
            self.remote_ops
                .set_process_status("start an SSH session before renicing processes");
            self.shell
                .set_status(self.remote_ops.process_status().to_string());
            cx.notify();
            return;
        };
        let Some(job_session_id) = self.session.active_id_owned() else {
            self.remote_ops
                .set_process_status("start an SSH session before renicing processes");
            cx.notify();
            return;
        };
        if self.remote_ops.process_is_pending_for(&job_session_id) {
            self.remote_ops
                .set_process_status("process operation already running");
            cx.notify();
            return;
        }

        let ticket = self.remote_ops.begin_process_job(job_session_id.clone());
        self.remote_ops
            .set_process_status(format!("renicing pid {pid} to {nice}"));
        std::thread::spawn(move || {
            let result = (|| {
                let service = SshProcessService::new(config);
                service.renice_process(pid, nice)?;
                let processes = service.list_processes()?;
                Ok(ProcessJobOutput::Reniced {
                    pid,
                    nice,
                    processes,
                })
            })()
            .map_err(|error: anyhow::Error| error.to_string());
            let _ = ticket.tx.send(ProcessJobResult {
                job_id: ticket.job_id,
                session_id: job_session_id,
                result,
            });
        });
        cx.notify();
    }

    pub(in crate::features) fn drain_process_events(&mut self) -> bool {
        let mut dirty = false;
        for _ in 0..PROCESS_EVENT_DRAIN_LIMIT {
            let Some(event) = self.remote_ops.next_process_event() else {
                break;
            };
            if !self
                .remote_ops
                .complete_process_event(event.job_id, &event.session_id)
            {
                continue;
            }
            dirty = true;
            if self.session.active_id() != Some(event.session_id.as_str()) {
                continue;
            }
            let was_list_refresh = self.remote_ops.process_status() == "listing remote processes";
            match event.result {
                Ok(ProcessJobOutput::Listed(processes)) => {
                    self.remote_ops.reset_process_refresh_failures();
                    self.remote_ops.set_process_status(format!(
                        "loaded {} remote process(es)",
                        processes.len()
                    ));
                    self.shell
                        .set_status(self.remote_ops.process_status().to_string());
                    self.remote_ops.apply_processes(processes);
                }
                Ok(ProcessJobOutput::Signalled {
                    pid,
                    signal,
                    processes,
                }) => {
                    self.remote_ops
                        .set_process_status(format!("sent {signal} to pid {pid}"));
                    self.shell
                        .set_status(self.remote_ops.process_status().to_string());
                    self.remote_ops.apply_processes(processes);
                }
                Ok(ProcessJobOutput::Reniced {
                    pid,
                    nice,
                    processes,
                }) => {
                    self.remote_ops
                        .set_process_status(format!("reniced pid {pid} to {nice}"));
                    self.shell
                        .set_status(self.remote_ops.process_status().to_string());
                    self.remote_ops.apply_processes(processes);
                }
                Err(error) => {
                    if was_list_refresh {
                        let terminal =
                            error.contains(nyaterm_transport::PROCESS_LIST_UNSUPPORTED_ERROR);
                        if self.remote_ops.record_process_refresh_failure(terminal) >= 3 {
                            self.remote_ops.clear_process_data();
                        }
                    }
                    self.remote_ops
                        .set_process_status(format!("process operation failed: {error}"));
                    self.shell
                        .set_status(self.remote_ops.process_status().to_string());
                }
            }
        }
        dirty
    }
}
