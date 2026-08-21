use rust_i18n::t;

use crate::features::NyaTermApp;
use crate::models::{
    TransferJobEvent, TransferJobKind, TransferJobOutput, TransferJobResult, TransferJobState,
    TransferJobStatus, TransferMoveState,
};
use gpui::{Context, Window};
use nyaterm_core::truncate_preview;
use nyaterm_transport::RemoteFilePath;

use super::super::helpers::{remote_file_name, remote_parent_path};

impl NyaTermApp {
    pub(in crate::features) fn open_transfer_move_dialog(
        &mut self,
        identity: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.forget_text_inputs("transfer.move.");
        let entry = self
            .transfer
            .browser_view()
            .entries
            .iter()
            .find(|entry| entry.matches_identity(&identity))
            .cloned();
        let old_path = entry
            .as_ref()
            .map(|entry| entry.path.clone())
            .unwrap_or(identity);
        let name = remote_file_name(&old_path);
        if old_path.trim().is_empty() || old_path == "/" || name == "." || name == ".." {
            self.shell.set_status(format!("cannot move {old_path}"));
            cx.notify();
            return;
        }
        self.transfer.open_move_dialog(TransferMoveState {
            old_path: old_path.clone(),
            raw_path_token: entry.and_then(|entry| entry.raw_path_token),
            name: name.clone(),
            value: old_path,
        });
        self.shell.set_status("remote move opened".to_string());
        self.open_form_dialog(
            (
                t!("fileExplorer.moveTo").replace("{{name}}", &truncate_preview(&name, 48)),
                384.,
                t!("common.save").to_string(),
                |app, _, cx| app.transfer_move_dialog_content(cx),
                |app, window, cx| app.submit_transfer_move(window, cx),
                |app, cx| app.close_transfer_move_dialog(cx),
            ),
            window,
            cx,
        );
        cx.notify();
    }

    pub(in crate::features) fn close_transfer_move_dialog(&mut self, cx: &mut Context<Self>) {
        self.forget_text_inputs("transfer.move.");
        self.transfer.close_move_dialog();
        self.shell.set_status("remote move cancelled".to_string());
        cx.notify();
    }

    pub(in crate::features) fn submit_transfer_move(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(state) = self.transfer.move_dialog().cloned() else {
            self.shell
                .set_status("no remote move is active".to_string());
            cx.notify();
            return true;
        };
        let new_path = state.value.trim().to_string();
        if new_path.is_empty() {
            self.shell
                .set_status("target path cannot be empty".to_string());
            cx.notify();
            return false;
        }
        if new_path == state.old_path {
            self.transfer.close_move_dialog();
            self.shell.set_status("remote move unchanged".to_string());
            cx.notify();
            return true;
        }
        self.transfer.close_move_dialog();
        self.start_sftp_move_job(
            RemoteFilePath {
                display_path: state.old_path,
                raw_path_token: state.raw_path_token,
            },
            new_path,
            window,
            cx,
        );
        true
    }

    /// Apply an edit from the move dialog's path box.
    pub(in crate::features) fn apply_transfer_move_path(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        if self.transfer.set_move_value(text) {
            cx.notify();
        }
    }

    pub(in crate::features) fn start_sftp_move_job(
        &mut self,
        old_path: RemoteFilePath,
        new_path: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let service = match self.active_remote_file_service() {
            Ok(service) => service,
            Err(error) => {
                self.shell.set_status(error.to_string());
                cx.notify();
                return;
            }
        };
        let old_display_path = old_path.display_path.clone();
        let parent_path = remote_parent_path(&old_display_path);
        let id = self.transfer.next_transfer_job_id("sftp-move");
        self.transfer.enqueue_transfer_job(TransferJobState {
            id: id.clone(),
            session_id: self.session.active_id_owned(),
            kind: TransferJobKind::Move {
                old_path: old_display_path.clone(),
                new_path: new_path.clone(),
                parent_path: parent_path.clone(),
            },
            status: TransferJobStatus::Running,
            detail: format!("Moving {old_display_path}"),
            created_at_ms: TransferJobState::now_ms(),
            display_name: String::new(),
            entries: Vec::new(),
            summary: None,
            progress: None,
            control: None,
        });
        self.shell.set_status(format!(
            "remote move started: {old_display_path} -> {new_path}"
        ));
        let transfer_tx = self.transfer.transfer_event_sender();
        std::thread::spawn(move || {
            let result = service
                .rename_remote_paths(&old_path, &RemoteFilePath::new(&new_path))
                .and_then(|_| service.list_dir(&parent_path))
                .map(|entries| TransferJobOutput::Moved {
                    old_path: old_display_path,
                    new_path,
                    parent_path,
                    entries,
                })
                .map_err(|error| error.to_string());
            let _ = transfer_tx.send(TransferJobResult {
                id,
                event: TransferJobEvent::Finished(result),
            });
        });
        cx.notify();
    }

    pub(in crate::features) fn open_selected_transfer_delete_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let paths = self
            .selected_transfer_entries()
            .into_iter()
            .map(|entry| entry.remote_path())
            .filter(|path| {
                let name = remote_file_name(&path.display_path);
                !path.display_path.trim().is_empty()
                    && path.display_path != "/"
                    && name != "."
                    && name != ".."
            })
            .collect::<Vec<_>>();
        if paths.is_empty() {
            self.shell
                .set_status("mark remote items before deleting".to_string());
            cx.notify();
            return;
        }
        let delete_count = paths.len();
        let title = if delete_count == 1 {
            t!("fileExplorer.sureDelete")
                .replace("{{name}}", &remote_file_name(&paths[0].display_path))
        } else {
            t!("fileExplorer.sureDeleteMultiple").replace("{{count}}", &delete_count.to_string())
        };
        let preview = paths
            .iter()
            .take(6)
            .map(|path| truncate_preview(&remote_file_name(&path.display_path), 72))
            .collect::<Vec<_>>()
            .join("\n");
        let remaining = delete_count.saturating_sub(6);
        let mut detail = t!("fileExplorer.deleteConfirmHint").to_string();
        if delete_count > 1 {
            detail.push_str("\n\n");
            detail.push_str(&preview);
            if remaining > 0 {
                detail.push('\n');
                detail.push_str(
                    &t!("fileExplorer.moreItems").replace("{{count}}", &remaining.to_string()),
                );
            }
        }
        self.shell
            .set_status("remote delete confirmation opened".to_string());
        self.open_confirm_dialog(
            (
                title,
                detail,
                t!("fileExplorer.delete").to_string(),
                true,
                move |app, window, cx| {
                    for remote_path in &paths {
                        app.start_sftp_delete_job(remote_path.clone(), window, cx);
                    }
                    app.shell
                        .set_status(format!("{delete_count} remote delete job(s) started"));
                    cx.notify();
                    true
                },
            ),
            window,
            cx,
        );
        cx.notify();
    }

    pub(in crate::features) fn start_sftp_delete_job(
        &mut self,
        remote_path: RemoteFilePath,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let service = match self.active_remote_file_service() {
            Ok(service) => service,
            Err(error) => {
                self.shell.set_status(error.to_string());
                cx.notify();
                return;
            }
        };
        let remote_display_path = remote_path.display_path.clone();
        let parent_path = remote_parent_path(&remote_display_path);
        let id = self.transfer.next_transfer_job_id("sftp-delete");
        self.transfer.enqueue_transfer_job(TransferJobState {
            id: id.clone(),
            session_id: self.session.active_id_owned(),
            kind: TransferJobKind::Delete {
                remote_path: remote_display_path.clone(),
                parent_path: parent_path.clone(),
            },
            status: TransferJobStatus::Running,
            detail: format!("Deleting {remote_display_path}"),
            created_at_ms: TransferJobState::now_ms(),
            display_name: String::new(),
            entries: Vec::new(),
            summary: None,
            progress: None,
            control: None,
        });
        self.shell
            .set_status(format!("remote delete started: {remote_display_path}"));
        let transfer_tx = self.transfer.transfer_event_sender();
        std::thread::spawn(move || {
            let result = service
                .delete_remote_path(&remote_path)
                .and_then(|_| service.list_dir(&parent_path))
                .map(|entries| TransferJobOutput::Deleted {
                    remote_path: remote_display_path,
                    parent_path,
                    entries,
                })
                .map_err(|error| error.to_string());
            let _ = transfer_tx.send(TransferJobResult {
                id,
                event: TransferJobEvent::Finished(result),
            });
        });
        cx.notify();
    }
}
