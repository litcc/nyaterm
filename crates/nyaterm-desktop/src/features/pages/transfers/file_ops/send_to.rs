use std::sync::Arc;

use rust_i18n::t;

use gpui::{Context, Window};
use nyaterm_transport::{
    FileCopyRequest, FileTransferEndpoint, RemoteFilePath, RemoteFileService, SftpFileEntry,
};

use crate::features::NyaTermApp;
use crate::models::{
    TransferJobEvent, TransferJobKind, TransferJobOutput, TransferJobResult, TransferJobState,
    TransferJobStatus,
};

use super::super::context_menu_policy::{
    SendToCandidate, send_to_candidate_is_eligible, send_to_destination_path,
    send_to_target_directory,
};
use super::super::helpers::{remote_file_name, remote_parent_path};

/// A session the current selection can be sent to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::features::pages::transfers) struct TransferSendToTarget {
    pub(in crate::features::pages::transfers) session_id: String,
    /// Display name shown as the submenu entry label.
    pub(in crate::features::pages::transfers) label: String,
    /// SSH address shown as the entry meta (e.g. `ssh -p 22 user@host`).
    pub(in crate::features::pages::transfers) meta: Option<String>,
}

impl NyaTermApp {
    /// Eligible "Send to" targets for the active browser session.
    ///
    /// A target is any *other* session that is connected (not disconnected) and
    /// exposes an SSH config that the file browser can use. The source session is
    /// always excluded. Ordered like the session tabs so the menu is stable.
    pub(in crate::features::pages::transfers) fn transfer_send_to_targets(
        &self,
    ) -> Vec<TransferSendToTarget> {
        let Some(source_session_id) = self.session.active_id() else {
            return Vec::new();
        };
        self.session
            .ordered_sessions()
            .into_iter()
            .filter_map(|session| {
                let has_ssh_config = self
                    .session
                    .metadata(&session.id)
                    .is_some_and(|metadata| metadata.ssh_config.is_some());
                let candidate = SendToCandidate {
                    session_id: session.id.clone(),
                    has_ssh_config,
                    is_disconnected: self.session.is_disconnected(&session.id),
                };
                send_to_candidate_is_eligible(source_session_id, &candidate).then(|| {
                    TransferSendToTarget {
                        label: self.session.display_name_by_info(&session),
                        meta: self.session.ssh_address(&session.id),
                        session_id: session.id,
                    }
                })
            })
            .collect()
    }

    /// Resolve the target session's destination directory *synchronously* from its
    /// browser cache when present. `None` means the background job must resolve the
    /// target session's home/cwd instead — the source session's path is never used.
    fn transfer_send_to_cached_target_dir(&self, target_session_id: &str) -> Option<String> {
        let cache = self.transfer.browser_session_cache(target_session_id)?;
        let dir = send_to_target_directory(Some(cache.current_path.as_str()), None);
        (dir != ".").then_some(dir)
    }

    /// Copy the current selection set to another connected SSH session.
    pub(in crate::features::pages::transfers) fn start_send_selected_transfers_to_session(
        &mut self,
        target_session_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entries = self.selected_transfer_entries();
        if entries.is_empty() {
            self.shell
                .set_status(t!("fileTransfer.statusMarkBeforeSending").to_string());
            cx.notify();
            return;
        }

        let source_service = match self.active_remote_file_service() {
            Ok(service) => Arc::new(service),
            Err(error) => {
                self.shell.set_status(error.to_string());
                cx.notify();
                return;
            }
        };

        let Some(target_config) = self
            .session
            .metadata(&target_session_id)
            .and_then(|metadata| metadata.ssh_config.clone())
        else {
            self.shell
                .set_status(t!("fileTransfer.statusSendTargetUnavailable").to_string());
            cx.notify();
            return;
        };
        let target_service =
            match self.remote_file_service_for_session(&target_session_id, target_config) {
                Ok(service) => Arc::new(service),
                Err(error) => {
                    self.shell.set_status(error.to_string());
                    cx.notify();
                    return;
                }
            };

        let cached_target_dir = self.transfer_send_to_cached_target_dir(&target_session_id);
        let source_session_id = self.session.active_id_owned();
        let total = entries.len();

        let context = SendToJobContext {
            source_session_id,
            source_service,
            target_session_id,
            target_service,
            cached_target_dir,
        };
        for entry in entries {
            self.enqueue_send_to_job(&context, entry, cx);
        }
        self.shell
            .set_status(t!("fileTransfer.statusSendJobsStarted", count = total).to_string());
        cx.notify();
    }

    fn enqueue_send_to_job(
        &mut self,
        context: &SendToJobContext,
        entry: SftpFileEntry,
        cx: &mut Context<Self>,
    ) {
        let id = self.transfer.next_transfer_job_id("sftp-send");
        let entry_name = remote_file_name(&entry.path);
        let source_path = entry.path.clone();
        // Best-effort provisional target for the visible row; the worker resolves
        // the real directory (cache or the target session's home) before copying.
        let provisional_dir = context
            .cached_target_dir
            .clone()
            .unwrap_or_else(|| "~".to_string());
        let provisional_target = send_to_destination_path(&provisional_dir, &entry_name);
        let provisional_parent = remote_parent_path(&provisional_target);

        self.transfer.enqueue_transfer_job(TransferJobState {
            id: id.clone(),
            // Scope the row to the *source* session so it shows in that session's
            // queue; the target session is recorded in the job kind.
            session_id: context.source_session_id.clone(),
            kind: TransferJobKind::SendTo {
                source_path: source_path.clone(),
                target_session_id: context.target_session_id.clone(),
                target_path: provisional_target.clone(),
                target_parent_path: provisional_parent,
            },
            status: TransferJobStatus::Running,
            detail: t!("fileTransfer.detailSending", path = source_path.clone()).to_string(),
            created_at_ms: TransferJobState::now_ms(),
            display_name: String::new(),
            entries: Vec::new(),
            summary: None,
            progress: None,
            control: None,
        });

        let source_remote = entry.remote_path();
        let source_service = context.source_service.clone();
        let target_service = context.target_service.clone();
        let target_session_id = context.target_session_id.clone();
        let cached_target_dir = context.cached_target_dir.clone();
        let transfer_tx = self.transfer.transfer_event_sender();
        self.submit_transfer_blocking_job(
            "sftp-send-to",
            id.clone(),
            transfer_tx.clone(),
            move || {
                let result = run_send_to(
                    &source_service,
                    source_remote,
                    &target_service,
                    cached_target_dir.as_deref(),
                    &entry_name,
                    target_session_id,
                );
                let _ = transfer_tx.unbounded_send(TransferJobResult {
                    id,
                    event: TransferJobEvent::Finished(result),
                });
            },
        );
        cx.notify();
    }
}

/// Shared inputs for every per-entry "Send to" job in one send action.
struct SendToJobContext {
    source_session_id: Option<String>,
    source_service: Arc<RemoteFileService>,
    target_session_id: String,
    target_service: Arc<RemoteFileService>,
    cached_target_dir: Option<String>,
}

/// Copy one entry to the target session and list the destination directory.
///
/// Backend selection is left to `FileCopyRequest`: SFTP-to-SFTP copies directly,
/// mixed backends stage through a local temp directory.
fn run_send_to(
    source_service: &Arc<RemoteFileService>,
    source_path: RemoteFilePath,
    target_service: &Arc<RemoteFileService>,
    cached_target_dir: Option<&str>,
    entry_name: &str,
    target_session_id: String,
) -> Result<TransferJobOutput, String> {
    // Resolve the destination directory from the target session only: the cache
    // when present, otherwise the target session's home. Never the source path.
    let target_dir = match cached_target_dir {
        Some(dir) => dir.to_string(),
        None => {
            let home = target_service.home_dir().map_err(|error| {
                t!(
                    "fileTransfer.errorResolveTargetHome",
                    error = error.to_string()
                )
                .to_string()
            })?;
            send_to_target_directory(None, Some(home.as_str()))
        }
    };
    let target_path = send_to_destination_path(&target_dir, entry_name);
    let target_parent_path = remote_parent_path(&target_path);

    let request = FileCopyRequest {
        source: FileTransferEndpoint::Remote {
            service: source_service.clone(),
            path: source_path.clone(),
        },
        destination: FileTransferEndpoint::Remote {
            service: target_service.clone(),
            path: RemoteFilePath::new(target_path.clone()),
        },
    };
    let summary = request
        .execute()
        .map_err(|error| t!("fileTransfer.errorSend", error = error.to_string()).to_string())?;

    // A best-effort refresh of the destination listing; a failure here does not
    // fail the copy, it just leaves the target cache untouched.
    let entries = target_service
        .list_dir(&target_parent_path)
        .unwrap_or_default();

    Ok(TransferJobOutput::Sent {
        source_path: source_path.display_path,
        target_session_id,
        target_path,
        target_parent_path,
        bytes: summary.bytes,
        used_local_staging: summary.used_local_staging,
        entries,
    })
}
