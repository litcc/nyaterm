use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use futures::StreamExt as _;
use gpui::{Context, Window};
use nyaterm_core::{AiAction, truncate_preview};
use nyaterm_transport::{
    SFTP_TRANSFER_CANCELLED, SftpFileEntry, SftpFileType, SftpTransferProgress,
};
use nyaterm_ui::NyaDialogWindowExt as _;

use crate::features::NyaTermApp;
use crate::features::formatting::format_permissions_octal;
use crate::models::{
    AiPreparedRequest, NavItem, TransferBrowserChildrenMenuStatus, TransferBrowserPathMenuKind,
    TransferBrowserPathMenuState, TransferExternalSyncPromptState, TransferJobEvent,
    TransferJobKind, TransferJobOutput, TransferJobResult, TransferJobStatus,
};

type ExternalEditorSyncStart = (Option<String>, String, String, Option<String>, PathBuf);

use super::state::{TransferEditorSaveOutcome, TransferFeatureState};
use super::transfer_widgets::{format_file_size, format_transfer_progress};

#[derive(Clone)]
struct TransferBrowserEventSnapshot {
    remote_path: String,
    browser_path: String,
    home_dir: String,
    home_dir_pending: bool,
    entries: Vec<SftpFileEntry>,
    loading: bool,
    error: Option<String>,
    status: String,
    history: VecDeque<String>,
    history_index: usize,
    visited_history: VecDeque<String>,
    selected_path: Option<String>,
    selected_paths: HashSet<String>,
}

/// Save/restore of everything a browser event may rewind.
///
/// Taking `&TransferFeatureState` rather than `&NyaTermApp` means a snapshot
/// can only capture and replay transfer state, which is what the callers
/// intend. The captured fields are unchanged.
impl TransferFeatureState {
    fn browser_event_snapshot(&self) -> TransferBrowserEventSnapshot {
        TransferBrowserEventSnapshot {
            remote_path: self.remote_path().to_string(),
            browser_path: self.browser.path.clone(),
            home_dir: self.browser.home_dir.clone(),
            home_dir_pending: self.browser.home_dir_pending,
            entries: self.browser.entries.clone(),
            loading: self.browser.loading,
            error: self.browser.error.clone(),
            status: self.browser.status.clone(),
            history: self.browser.history.clone(),
            history_index: self.browser.history_index,
            visited_history: self.browser.visited_history.clone(),
            selected_path: self.browser.selected_remote_path.clone(),
            selected_paths: self.browser.selected_remote_paths.clone(),
        }
    }

    fn restore_browser_event_snapshot(&mut self, snapshot: TransferBrowserEventSnapshot) {
        self.set_remote_path(snapshot.remote_path);
        self.browser.path = snapshot.browser_path;
        self.browser.home_dir = snapshot.home_dir;
        self.browser.home_dir_pending = snapshot.home_dir_pending;
        self.browser.entries = snapshot.entries;
        self.browser.loading = snapshot.loading;
        self.browser.error = snapshot.error;
        self.browser.status = snapshot.status;
        self.browser.history = snapshot.history;
        self.browser.history_index = snapshot.history_index;
        self.browser.visited_history = snapshot.visited_history;
        self.browser.selected_remote_path = snapshot.selected_path;
        self.browser.selected_remote_paths = snapshot.selected_paths;
    }
}

impl NyaTermApp {
    fn load_transfer_browser_event_session(&mut self, session_id: &str) {
        let Some(cache) = self.transfer.browser.session_cache.get(session_id).cloned() else {
            self.transfer.set_remote_path(".");
            self.transfer.browser.path = ".".to_string();
            self.transfer.browser.home_dir.clear();
            self.transfer.browser.home_dir_pending = false;
            self.transfer.browser.entries.clear();
            self.transfer.browser.loading = false;
            self.transfer.browser.error = None;
            self.transfer.browser.status.clear();
            self.transfer.browser.history.clear();
            self.transfer.browser.history_index = 0;
            self.transfer.browser.visited_history.clear();
            self.transfer.browser.selected_remote_path = None;
            self.transfer.browser.selected_remote_paths.clear();
            self.transfer.browser.path_menu = None;
            return;
        };

        self.transfer.set_remote_path(cache.current_path.clone());
        self.transfer.browser.path = cache.current_path;
        self.transfer.browser.home_dir = cache.home_dir;
        self.transfer.browser.home_dir_pending = false;
        self.transfer.browser.entries = cache.entries;
        self.transfer.browser.loading = false;
        self.transfer.browser.error = None;
        self.transfer.browser.status.clear();
        self.transfer.browser.history = cache.history;
        self.transfer.browser.history_index = cache.history_index;
        self.transfer.browser.visited_history = cache.visited_history;
        self.transfer.browser.selected_remote_path = None;
        self.transfer.browser.selected_remote_paths.clear();
        self.transfer.browser.path_menu = None;
    }

    /// Deliver transfer job events -- start, progress, finish -- as they arrive.
    ///
    /// Started once at window open. Before this the runtime tick polled
    /// `try_recv_transfer_event`, so a progress update waited for the next tick
    /// and `runtime_quiet_tick_allowed` had to carry a `transfer` term to keep
    /// that wait short.
    ///
    /// `update_in` rather than `update`: the handler opens dialogs and external
    /// editor windows, so it needs the `Window` the app entity is rendered in.
    pub(in crate::features) fn start_transfer_event_drain(&mut self, cx: &mut Context<Self>) {
        let Some(mut rx) = self.transfer.take_transfer_event_receiver() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            while let Some(event) = rx.next().await {
                if this
                    .update_in(cx, |this, window, cx| {
                        if this.apply_transfer_event(event, window, cx) {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    /// Apply one job event, reporting whether the UI needs a repaint.
    fn apply_transfer_event(
        &mut self,
        event: TransferJobResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((job_index, mut job)) = self.transfer.take_transfer_job_for_event(&event.id)
        else {
            // The job was cancelled or removed while its worker was still running.
            return false;
        };
        let mut dirty = false;
        let event_id = event.id.clone();
        let job_session_id = job.session_id.clone();
        let navigation_job_key = matches!(
            &job.kind,
            TransferJobKind::ListDir { .. } | TransferJobKind::SyncCwd
        )
        .then(|| job_session_id.clone().unwrap_or_default());
        if transfer_navigation_job_is_stale(
            &self.transfer.browser.navigation_jobs,
            navigation_job_key.as_deref(),
            &event_id,
        ) {
            self.transfer.browser.pending_navigations.remove(&event_id);
            return false;
        }
        dirty |= transfer_event_needs_ui_refresh(
            self.session.active_id(),
            job_session_id.as_deref(),
            &event.event,
        );
        let inactive_browser_snapshot = job_session_id
            .as_deref()
            .filter(|session_id| self.session.active_id() != Some(*session_id))
            .filter(|_| transfer_event_needs_browser_context(&job.kind, &event.event))
            .map(|session_id| {
                let snapshot = self.transfer.browser_event_snapshot();
                self.load_transfer_browser_event_session(session_id);
                snapshot
            });
        let mut external_sync_to_start: Option<ExternalEditorSyncStart> = None;
        let mut external_sync_prompt_to_open: Option<String> = None;
        let mut zmodem_upload_after_probe: Option<(String, Vec<PathBuf>)> = None;
        let mut open_after_create: Option<SftpFileEntry> = None;
        let mut browser_navigation_rollback = None;
        let mut browser_listing_completed = false;
        let mut remote_path_to_set = None;
        let mut sync_properties_inputs = false;
        let mut forget_properties_inputs = false;
        let event_finished = matches!(&event.event, TransferJobEvent::Finished(_));
        let event_failed = matches!(&event.event, TransferJobEvent::Finished(Err(_)));
        let cleanup_internal_job = event_finished
            && !job.is_user_transfer()
            && (!matches!(&job.kind, TransferJobKind::OpenExternal { .. }) || event_failed);
        match event.event {
            TransferJobEvent::Started { detail } => {
                job.status = TransferJobStatus::Running;
                job.detail = detail;
                job.progress = None;
                job.summary = None;
            }
            TransferJobEvent::ExternalModified {
                remote_path,
                raw_path_token,
                local_path,
            } => {
                job.detail = format!("External edit changed {}", local_path.display());
                let watch_key = format!("{remote_path}\n{}", local_path.display());
                if self.transfer.external_sync_always_uploads(&watch_key) {
                    external_sync_to_start = Some((
                        job_session_id.clone(),
                        job.id.clone(),
                        remote_path.clone(),
                        raw_path_token.clone(),
                        local_path.clone(),
                    ));
                } else if let Some(session_id) = job_session_id.clone() {
                    let prompt_id = job.id.clone();
                    self.transfer.insert_external_sync_prompt(
                        prompt_id.clone(),
                        TransferExternalSyncPromptState {
                            session_id: Some(session_id),
                            job_id: job.id.clone(),
                            remote_path: remote_path.clone(),
                            raw_path_token,
                            local_path: local_path.clone(),
                        },
                    );
                    external_sync_prompt_to_open = Some(prompt_id);
                    self.shell
                        .set_status(format!("external edit changed: {}", local_path.display()));
                }
            }
            TransferJobEvent::Progress(progress) => {
                if job.status == TransferJobStatus::Running {
                    job.detail = format_transfer_progress(&progress);
                }
                job.progress = Some(progress);
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::Entries(entries))) => {
                browser_listing_completed = true;
                let (listed_path, select_after) = match &job.kind {
                    TransferJobKind::ListDir {
                        remote_path,
                        select_after,
                    } => (remote_path.clone(), select_after.clone()),
                    _ => (self.transfer.browser.path.clone(), None),
                };
                job.status = TransferJobStatus::Completed;
                job.detail = format!("{} item(s)", entries.len());
                self.transfer.browser.path = listed_path;
                self.transfer.browser.entries = entries.clone();
                self.transfer.browser.loading = false;
                self.transfer.browser.error = None;
                self.transfer.browser.status = job.detail.clone();
                self.transfer
                    .browser
                    .selected_remote_paths
                    .retain(|identity| {
                        entries.iter().any(|entry| entry.matches_identity(identity))
                    });
                if let Some(select_after) = select_after
                    && entries.iter().any(|entry| entry.path == select_after)
                {
                    self.transfer.browser.selected_remote_path = Some(select_after.clone());
                    self.transfer.browser.selected_remote_paths.clear();
                    self.transfer
                        .browser
                        .selected_remote_paths
                        .insert(select_after.clone());
                    remote_path_to_set = Some(select_after);
                }
                job.entries = entries;
                job.summary = None;
                job.progress = None;
                job.control = None;
                self.shell
                    .set_status(format!("remote file list completed: {}", job.detail));
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::ChildEntries {
                remote_path,
                mut entries,
            })) => {
                entries.retain(|entry| {
                    entry.is_directory()
                        && entry.name != "."
                        && entry.name != ".."
                        && (self.settings.summary().ui_file_explorer_show_hidden_files
                            || !entry.name.starts_with('.'))
                });
                entries.sort_by(|left, right| {
                    left.name
                        .to_lowercase()
                        .cmp(&right.name.to_lowercase())
                        .then_with(|| left.name.cmp(&right.name))
                });
                job.status = TransferJobStatus::Completed;
                job.detail = if entries.len() == 1 {
                    "1 child directory".to_string()
                } else {
                    format!("{} child directories", entries.len())
                };
                job.entries = entries.clone();
                job.summary = None;
                job.progress = None;
                job.control = None;

                if job_session_id.as_deref() == self.session.active_id()
                    && let Some(TransferBrowserPathMenuState {
                        session_id,
                        kind:
                            TransferBrowserPathMenuKind::Children {
                                path,
                                request_id,
                                status,
                                ..
                            },
                        ..
                    }) = self.transfer.browser.path_menu.as_mut()
                    && *session_id == job_session_id
                    && path == &remote_path
                    && request_id.as_deref() == Some(event_id.as_str())
                {
                    *request_id = None;
                    *status = TransferBrowserChildrenMenuStatus::Ready(entries);
                }
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::HomeDir(home_dir))) => {
                job.status = TransferJobStatus::Completed;
                job.detail = format!("Home {home_dir}");
                self.transfer.browser.home_dir = home_dir.clone();
                self.transfer.browser.home_dir_pending = false;
                self.transfer.browser.loading = false;
                self.transfer.browser.error = None;
                self.transfer.browser.status =
                    format!("remote home resolved: {}", truncate_preview(&home_dir, 72));
                job.entries.clear();
                job.summary = None;
                job.progress = None;
                job.control = None;
                self.shell.set_status("remote home resolved".to_string());
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::CwdSynced {
                remote_path,
                entries,
            })) => {
                job.status = TransferJobStatus::Completed;
                job.detail = format!("Synced cwd {remote_path}");
                remote_path_to_set = Some(remote_path.clone());
                self.transfer.browser.list_scroll = gpui::UniformListScrollHandle::new();
                self.transfer.browser.horizontal_scroll = gpui::ScrollHandle::new();
                self.transfer.browser.path = remote_path;
                self.transfer.browser.entries = entries.clone();
                self.transfer.browser.loading = false;
                self.transfer.browser.error = None;
                self.transfer.browser.status =
                    format!("remote exec cwd · {} item(s)", entries.len());
                self.transfer.browser.selected_remote_path = None;
                self.transfer.browser.selected_remote_paths.clear();
                job.entries = entries;
                job.summary = None;
                job.progress = None;
                job.control = None;
                self.shell
                    .set_status("remote cwd sync completed".to_string());
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::Renamed {
                old_path,
                new_path,
                parent_path,
                entries,
            })) => {
                job.status = TransferJobStatus::Completed;
                job.detail = format!("Renamed {old_path} -> {new_path}");
                self.transfer.browser.path = parent_path.clone();
                self.transfer.browser.entries = entries.clone();
                self.transfer.browser.status = format!("{} item(s)", entries.len());
                job.entries = entries;
                job.summary = None;
                job.progress = None;
                job.control = None;
                self.transfer.browser.selected_remote_path = Some(new_path.clone());
                self.transfer
                    .browser
                    .selected_remote_paths
                    .remove(&old_path);
                self.transfer
                    .browser
                    .selected_remote_paths
                    .insert(new_path.clone());
                self.transfer.set_remote_path(new_path.clone());
                self.shell.set_status(format!(
                    "remote rename completed in {parent_path}: {new_path}"
                ));
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::Moved {
                old_path,
                new_path,
                parent_path,
                entries,
            })) => {
                job.status = TransferJobStatus::Completed;
                job.detail = format!("Moved {old_path} -> {new_path}");
                self.transfer.browser.path = parent_path.clone();
                self.transfer.browser.entries = entries.clone();
                self.transfer.browser.status = format!("{} item(s)", entries.len());
                job.entries = entries;
                job.summary = None;
                job.progress = None;
                job.control = None;
                self.transfer.browser.selected_remote_path = Some(new_path.clone());
                self.transfer
                    .browser
                    .selected_remote_paths
                    .remove(&old_path);
                self.transfer
                    .browser
                    .selected_remote_paths
                    .insert(new_path.clone());
                self.transfer.set_remote_path(new_path.clone());
                self.shell.set_status(format!(
                    "remote move completed from {parent_path}: {new_path}"
                ));
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::Deleted {
                remote_path,
                parent_path,
                entries,
            })) => {
                job.status = TransferJobStatus::Completed;
                job.detail = format!("Deleted {remote_path}");
                self.transfer.browser.path = parent_path.clone();
                self.transfer.browser.entries = entries.clone();
                self.transfer.browser.status = format!("{} item(s)", entries.len());
                job.entries = entries;
                job.summary = None;
                job.progress = None;
                job.control = None;
                if self.transfer.browser.selected_remote_path.as_deref()
                    == Some(remote_path.as_str())
                {
                    self.transfer.browser.selected_remote_path = None;
                }
                self.transfer
                    .browser
                    .selected_remote_paths
                    .remove(&remote_path);
                self.shell.set_status(format!(
                    "remote delete completed in {parent_path}: {remote_path}"
                ));
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::CreatedDirectory {
                remote_path,
                parent_path,
                entries,
                open_after_create,
            })) => {
                job.status = TransferJobStatus::Completed;
                job.detail = format!("Created {remote_path}");
                self.transfer.browser.path = if open_after_create {
                    remote_path.clone()
                } else {
                    parent_path.clone()
                };
                self.transfer.browser.entries = entries.clone();
                self.transfer.browser.status = format!("{} item(s)", entries.len());
                job.entries = entries;
                job.summary = None;
                job.progress = None;
                job.control = None;
                self.transfer.browser.selected_remote_path = Some(remote_path.clone());
                self.transfer.browser.selected_remote_paths.clear();
                self.transfer
                    .browser
                    .selected_remote_paths
                    .insert(remote_path.clone());
                self.transfer.set_remote_path(remote_path.clone());
                self.shell.set_status(format!(
                    "remote directory created in {parent_path}: {remote_path}"
                ));
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::CreatedFile {
                remote_path,
                parent_path,
                entries,
                open_after_create: should_open,
            })) => {
                job.status = TransferJobStatus::Completed;
                job.detail = format!("Created {remote_path}");
                self.transfer.browser.path = parent_path.clone();
                self.transfer.browser.entries = entries.clone();
                self.transfer.browser.status = format!("{} item(s)", entries.len());
                job.entries = entries.clone();
                job.summary = None;
                job.progress = None;
                job.control = None;
                self.transfer.browser.selected_remote_path = Some(remote_path.clone());
                self.transfer.browser.selected_remote_paths.clear();
                self.transfer
                    .browser
                    .selected_remote_paths
                    .insert(remote_path.clone());
                self.transfer.set_remote_path(remote_path.clone());
                self.shell.set_status(format!(
                    "remote file created in {parent_path}: {remote_path}"
                ));
                if should_open && job_session_id.as_deref() == self.session.active_id() {
                    open_after_create = Some(
                        entries
                            .iter()
                            .find(|entry| entry.path == remote_path)
                            .cloned()
                            .unwrap_or_else(|| SftpFileEntry {
                                name: remote_path
                                    .rsplit('/')
                                    .next()
                                    .unwrap_or(remote_path.as_str())
                                    .to_string(),
                                path: remote_path.clone(),
                                file_type: SftpFileType::File,
                                size: Some(0),
                                permissions: None,
                                owner: String::new(),
                                group: String::new(),
                                modified_at: None,
                                raw_path_token: None,
                                symlink_target_is_directory: false,
                            }),
                    );
                }
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::CreatedSymlink {
                link_path,
                target_path,
                parent_path,
                entries,
            })) => {
                job.status = TransferJobStatus::Completed;
                job.detail = format!("Linked {link_path} -> {target_path}");
                self.transfer.browser.path = parent_path.clone();
                self.transfer.browser.entries = entries.clone();
                self.transfer.browser.status = format!("{} item(s)", entries.len());
                job.entries = entries;
                job.summary = None;
                job.progress = None;
                job.control = None;
                self.transfer.browser.selected_remote_path = Some(link_path.clone());
                self.transfer.browser.selected_remote_paths.clear();
                self.transfer
                    .browser
                    .selected_remote_paths
                    .insert(link_path.clone());
                self.transfer.set_remote_path(link_path.clone());
                self.shell.set_status(format!(
                    "remote symlink created in {parent_path}: {link_path}"
                ));
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::PropertiesLoaded {
                remote_path,
                properties,
            })) => {
                job.status = TransferJobStatus::Completed;
                job.detail = format!("Loaded properties for {remote_path}");
                job.summary = None;
                job.progress = None;
                job.control = None;
                let mode_value = properties
                    .permissions
                    .map(format_permissions_octal)
                    .unwrap_or_else(|| "0644".to_string());
                let owner_value = if properties.owner.is_empty() {
                    properties
                        .uid
                        .map(|uid| uid.to_string())
                        .unwrap_or_default()
                } else {
                    properties.owner.clone()
                };
                let group_value = if properties.group.is_empty() {
                    properties
                        .gid
                        .map(|gid| gid.to_string())
                        .unwrap_or_default()
                } else {
                    properties.group.clone()
                };
                sync_properties_inputs = self.transfer.complete_properties_load(
                    job_session_id.as_deref(),
                    &remote_path,
                    properties,
                    mode_value,
                    owner_value,
                    group_value,
                );
                self.transfer.browser.status = format!("properties loaded for {remote_path}");
                self.shell
                    .set_status(format!("remote properties loaded: {remote_path}"));
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::PropertiesUpdated {
                remote_path,
                parent_path,
                properties,
                entries,
            })) => {
                job.status = TransferJobStatus::Completed;
                job.detail = format!("Updated properties for {remote_path}");
                self.transfer.browser.path = parent_path.clone();
                self.transfer.browser.entries = entries.clone();
                self.transfer.browser.status = format!("{} item(s)", entries.len());
                job.entries = entries;
                job.summary = None;
                job.progress = None;
                job.control = None;
                self.transfer.browser.selected_remote_path = Some(remote_path.clone());
                self.transfer.browser.selected_remote_paths.clear();
                self.transfer
                    .browser
                    .selected_remote_paths
                    .insert(remote_path.clone());
                self.transfer.set_remote_path(remote_path.clone());
                if self.transfer.complete_properties_update(
                    job_session_id.as_deref(),
                    &remote_path,
                    properties,
                ) {
                    forget_properties_inputs = true;
                    window.close_nya_dialog(cx);
                }
                self.shell.set_status(format!(
                    "remote properties updated in {parent_path}: {remote_path}"
                ));
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::EditorLoaded {
                tab_id,
                remote_path,
                file,
            })) => {
                job.status = TransferJobStatus::Completed;
                job.detail = format!("Opened {remote_path}");
                job.summary = None;
                job.progress = None;
                job.control = None;
                self.transfer.complete_editor_load_tab(&tab_id, file);
                self.transfer.browser.status = format!("opened text file {remote_path}");
                self.shell
                    .set_status(format!("remote text file opened: {remote_path}"));
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::AiFileActionLoaded {
                remote_path,
                action_id,
                action_name,
                prompt,
                file,
            })) => {
                job.status = TransferJobStatus::Completed;
                job.detail = format!("Prepared AI action {action_name} for {remote_path}");
                job.summary = None;
                job.progress = None;
                job.control = None;

                if job_session_id.as_deref() == self.session.active_id() {
                    let mut context = self.ai_terminal_context();
                    context.selected_text = file.content;
                    context.cwd = Some(transfer_event_remote_parent_path(&remote_path));
                    let request = AiPreparedRequest {
                        action: AiAction::CustomFileAction,
                        context,
                        source_label: format!("{action_name} · {remote_path}"),
                    };
                    self.set_ai_prompt_draft(prompt, cx);
                    self.ai.prepare_external_request(
                        request,
                        format!(
                            "Loaded {} byte(s) from {remote_path} for AI action {action_name}",
                            file.size
                        ),
                        format!("AI file action ready: {action_name} ({action_id})"),
                        true,
                    );
                    self.ensure_panel_open(NavItem::AiAssistant);
                    self.transfer.browser.status = format!("AI action ready for {remote_path}");
                    self.shell.set_status(format!(
                        "AI assistant opened for remote file: {remote_path}"
                    ));
                }
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::EditorSaved {
                tab_id,
                remote_path,
                result,
            })) => {
                job.status = TransferJobStatus::Completed;
                job.detail = format!("Saved {remote_path}");
                job.summary = None;
                job.progress = None;
                job.control = None;
                if let Some(outcome) = self.transfer.complete_editor_save_tab(&tab_id, result) {
                    self.shell.set_status(match outcome {
                        TransferEditorSaveOutcome::Saved => {
                            format!("remote text file saved: {remote_path}")
                        }
                        TransferEditorSaveOutcome::Conflict => {
                            format!("remote text save conflict: {remote_path}")
                        }
                        TransferEditorSaveOutcome::SavedAndClosed => {
                            format!("remote text file saved and closed: {remote_path}")
                        }
                    });
                }
                self.transfer.browser.status =
                    format!("text editor save finished for {remote_path}");
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::ExternalOpened {
                remote_path,
                local_path,
            })) => {
                job.status = TransferJobStatus::Completed;
                job.detail = format!("Opened {}", local_path.display());
                job.summary = None;
                job.progress = None;
                job.control = None;
                self.transfer.browser.status = format!("opened external {remote_path}");
                self.shell.set_status(format!(
                    "remote file opened externally: {}",
                    local_path.display()
                ));
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::Summary(summary))) => {
                job.status = TransferJobStatus::Completed;
                job.detail = if summary.skipped {
                    "Skipped duplicate".to_string()
                } else {
                    format!("{} transferred", format_file_size(Some(summary.bytes)))
                };
                job.entries.clear();
                job.progress = Some(SftpTransferProgress {
                    remote_path: summary.remote_path.clone(),
                    local_path: summary.local_path.clone(),
                    bytes_transferred: summary.bytes,
                    total_bytes: Some(summary.bytes),
                    item_count_completed: job
                        .progress
                        .as_ref()
                        .and_then(|progress| progress.item_count_total),
                    item_count_total: job
                        .progress
                        .as_ref()
                        .and_then(|progress| progress.item_count_total),
                });
                job.summary = Some(summary);
                self.shell
                    .set_status(format!("remote transfer completed: {}", job.detail));
                job.control = None;
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::Uploaded {
                summary,
                parent_path,
                entries,
            })) => {
                job.status = TransferJobStatus::Completed;
                job.detail = format!("{} uploaded", format_file_size(Some(summary.bytes)));
                job.progress = Some(SftpTransferProgress {
                    remote_path: summary.remote_path.clone(),
                    local_path: summary.local_path.clone(),
                    bytes_transferred: summary.bytes,
                    total_bytes: Some(summary.bytes),
                    item_count_completed: job
                        .progress
                        .as_ref()
                        .and_then(|progress| progress.item_count_total),
                    item_count_total: job
                        .progress
                        .as_ref()
                        .and_then(|progress| progress.item_count_total),
                });
                job.summary = Some(summary.clone());
                job.control = None;

                if transfer_event_paths_match(&self.transfer.browser.path, &parent_path) {
                    self.transfer.browser.path = parent_path.clone();
                    self.transfer.browser.entries = entries.clone();
                    self.transfer.browser.status = format!("{} item(s)", entries.len());
                    self.transfer.browser.selected_remote_path = Some(summary.remote_path.clone());
                    self.transfer.browser.selected_remote_paths.clear();
                    self.transfer
                        .browser
                        .selected_remote_paths
                        .insert(summary.remote_path.clone());
                    remote_path_to_set = Some(summary.remote_path.clone());
                } else {
                    self.transfer.browser.status =
                        format!("uploaded to {}", truncate_preview(&parent_path, 48));
                }

                job.entries = entries;
                self.shell.set_status(format!(
                    "remote upload completed in {parent_path}: {}",
                    job.detail
                ));
            }
            TransferJobEvent::Finished(Ok(TransferJobOutput::ZmodemProbeReady {
                session_id,
                files,
                probe_skipped,
            })) => {
                job.status = TransferJobStatus::Completed;
                job.detail = if probe_skipped {
                    format!("ZMODEM probe skipped; uploading {} file(s)", files.len())
                } else {
                    format!("ZMODEM probe ready; uploading {} file(s)", files.len())
                };
                job.entries.clear();
                job.summary = None;
                job.progress = None;
                job.control = None;
                self.shell.set_status(job.detail.clone());
                if files.is_empty() {
                    self.shell.set_status(
                        "ZMODEM upload cancelled — all conflicting files skipped".to_string(),
                    );
                } else {
                    zmodem_upload_after_probe = Some((session_id, files));
                }
            }
            TransferJobEvent::Finished(Err(error)) => {
                if matches!(&job.kind, TransferJobKind::ListDir { .. }) {
                    browser_navigation_rollback = self
                        .transfer
                        .browser
                        .pending_navigations
                        .remove(&event_id)
                        .map(|snapshot| (snapshot, error.clone()));
                }
                let browser_load_failed = matches!(
                    &job.kind,
                    TransferJobKind::ListDir { .. }
                        | TransferJobKind::ResolveHome
                        | TransferJobKind::SyncCwd
                );
                let property_remote_path = match &job.kind {
                    TransferJobKind::LoadProperties { remote_path }
                    | TransferJobKind::UpdateProperties { remote_path, .. }
                    | TransferJobKind::LoadEditor { remote_path, .. }
                    | TransferJobKind::SaveEditor { remote_path, .. }
                    | TransferJobKind::OpenExternal { remote_path, .. }
                    | TransferJobKind::AiFileAction { remote_path, .. } => {
                        Some(remote_path.clone())
                    }
                    _ => None,
                };
                if let TransferJobKind::ListChildren { remote_path } = &job.kind
                    && job_session_id.as_deref() == self.session.active_id()
                    && let Some(TransferBrowserPathMenuState {
                        session_id,
                        kind:
                            TransferBrowserPathMenuKind::Children {
                                path,
                                request_id,
                                status,
                                ..
                            },
                        ..
                    }) = self.transfer.browser.path_menu.as_mut()
                    && *session_id == job_session_id
                    && path == remote_path
                    && request_id.as_deref() == Some(event_id.as_str())
                {
                    *request_id = None;
                    *status = TransferBrowserChildrenMenuStatus::Error(error.clone());
                }
                if error == SFTP_TRANSFER_CANCELLED {
                    job.status = TransferJobStatus::Cancelled;
                    job.detail = "Cancelled".to_string();
                    self.shell
                        .set_status(format!("remote transfer cancelled: {}", job.id));
                } else {
                    job.status = TransferJobStatus::Failed;
                    job.detail = error.clone();
                    self.shell
                        .set_status(format!("remote transfer failed: {error}"));
                }
                if browser_load_failed {
                    self.transfer.browser.loading = false;
                    self.transfer.browser.error = self
                        .transfer
                        .browser
                        .entries
                        .is_empty()
                        .then_some(error.clone());
                }
                if let Some(remote_path) = property_remote_path.as_ref() {
                    self.transfer.fail_properties_operation(
                        job_session_id.as_deref(),
                        remote_path,
                        error.clone(),
                    );
                }
                if let TransferJobKind::LoadEditor { tab_id, .. }
                | TransferJobKind::SaveEditor { tab_id, .. } = &job.kind
                {
                    self.transfer.fail_editor_operation_tab(tab_id, error);
                }
                job.summary = None;
                job.control = None;
            }
        }
        if !cleanup_internal_job {
            self.transfer
                .restore_transfer_job_after_event((job_index, job));
        }
        if let Some(remote_path) = remote_path_to_set {
            self.transfer.set_remote_path(remote_path);
        }
        if sync_properties_inputs {
            self.sync_transfer_properties_inputs(cx);
        }
        if forget_properties_inputs {
            self.forget_text_inputs("transfer.properties.");
        }
        if let Some((snapshot, error)) = browser_navigation_rollback {
            self.restore_transfer_browser_navigation(snapshot);
            self.transfer.browser.status = format!("directory load failed: {error}");
            if self.transfer.browser.entries.is_empty() {
                self.transfer.browser.error = Some(error);
            }
        }
        if let Some((session_id, job_id, remote_path, raw_path_token, local_path)) =
            external_sync_to_start
        {
            self.spawn_external_editor_sync_upload(
                session_id,
                job_id,
                remote_path,
                raw_path_token,
                local_path,
            );
        }
        if let Some((session_id, files)) = zmodem_upload_after_probe {
            self.begin_zmodem_upload_after_probe(session_id, files, cx);
        }
        if let Some(entry) = open_after_create
            && inactive_browser_snapshot.is_none()
            && self.session.active_id() == job_session_id.as_deref()
        {
            self.open_transfer_default(entry, window, cx);
        }
        if browser_listing_completed && let Some(session_id) = job_session_id.as_deref() {
            self.cache_transfer_browser_session(session_id);
        }
        if let Some(snapshot) = inactive_browser_snapshot {
            self.transfer.restore_browser_event_snapshot(snapshot);
        }
        if event_finished
            && let Some(key) = navigation_job_key
            && self
                .transfer
                .browser
                .navigation_jobs
                .get(&key)
                .is_some_and(|latest_id| latest_id == &event_id)
        {
            self.transfer.browser.navigation_jobs.remove(&key);
        }
        if event_finished {
            self.transfer.browser.pending_navigations.remove(&event_id);
        }
        if let Some(prompt_id) = external_sync_prompt_to_open {
            self.open_transfer_external_sync_window(prompt_id, cx);
        }
        dirty
    }
}

fn transfer_event_paths_match(left: &str, right: &str) -> bool {
    transfer_event_normalized_path(left) == transfer_event_normalized_path(right)
}

fn transfer_event_needs_browser_context(kind: &TransferJobKind, event: &TransferJobEvent) -> bool {
    matches!(event, TransferJobEvent::Finished(_))
        && !matches!(kind, TransferJobKind::ListChildren { .. })
}

fn transfer_event_needs_ui_refresh(
    active_session_id: Option<&str>,
    job_session_id: Option<&str>,
    event: &TransferJobEvent,
) -> bool {
    !matches!(event, TransferJobEvent::Progress(_)) || job_session_id == active_session_id
}

fn transfer_event_remote_parent_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "/" {
        return "/".to_string();
    }
    match trimmed.rsplit_once('/') {
        Some(("", _)) => "/".to_string(),
        Some((parent, _)) if !parent.is_empty() => parent.to_string(),
        _ => ".".to_string(),
    }
}

fn transfer_event_normalized_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        ".".to_string()
    } else if trimmed == "/" {
        "/".to_string()
    } else {
        trimmed.trim_end_matches('/').to_string()
    }
}

fn transfer_navigation_job_is_stale(
    latest_jobs: &HashMap<String, String>,
    session_key: Option<&str>,
    event_id: &str,
) -> bool {
    session_key.is_some_and(|key| {
        latest_jobs
            .get(key)
            .is_none_or(|latest_id| latest_id != event_id)
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use nyaterm_transport::SftpTransferProgress;

    use crate::models::{TransferJobEvent, TransferJobKind, TransferJobOutput};

    use super::{
        transfer_event_needs_browser_context, transfer_event_needs_ui_refresh,
        transfer_navigation_job_is_stale,
    };

    #[test]
    fn superseded_transfer_navigation_result_is_stale_per_session() {
        let latest_jobs = HashMap::from([
            ("session-a".to_string(), "job-a2".to_string()),
            ("session-b".to_string(), "job-b1".to_string()),
        ]);

        assert!(transfer_navigation_job_is_stale(
            &latest_jobs,
            Some("session-a"),
            "job-a1"
        ));
        assert!(!transfer_navigation_job_is_stale(
            &latest_jobs,
            Some("session-a"),
            "job-a2"
        ));
        assert!(!transfer_navigation_job_is_stale(
            &latest_jobs,
            Some("session-b"),
            "job-b1"
        ));
        assert!(!transfer_navigation_job_is_stale(
            &latest_jobs,
            None,
            "unrelated-job"
        ));
    }

    #[test]
    fn transfer_progress_does_not_swap_inactive_browser_context() {
        assert!(!transfer_event_needs_browser_context(
            &TransferJobKind::Download {
                remote_path: "/tmp/file".to_string(),
                raw_path_token: None,
                local_path: PathBuf::from("/tmp/file"),
            },
            &TransferJobEvent::Progress(SftpTransferProgress {
                remote_path: "/tmp/file".to_string(),
                local_path: PathBuf::from("/tmp/file"),
                bytes_transferred: 1,
                total_bytes: Some(2),
                item_count_completed: None,
                item_count_total: None,
            },)
        ));
        assert!(transfer_event_needs_browser_context(
            &TransferJobKind::Upload {
                local_path: PathBuf::from("/tmp/file"),
                remote_path: "/tmp/file".to_string(),
            },
            &TransferJobEvent::Finished(Ok(TransferJobOutput::Uploaded {
                summary: nyaterm_transport::SftpTransferSummary {
                    remote_path: "/tmp/file".to_string(),
                    local_path: PathBuf::from("/tmp/file"),
                    bytes: 2,
                    skipped: false,
                },
                parent_path: "/tmp".to_string(),
                entries: Vec::new(),
            }))
        ));
    }

    #[test]
    fn child_directory_results_do_not_swap_browser_session_context() {
        assert!(!transfer_event_needs_browser_context(
            &TransferJobKind::ListChildren {
                remote_path: "/tmp".to_string(),
            },
            &TransferJobEvent::Finished(Ok(TransferJobOutput::ChildEntries {
                remote_path: "/tmp".to_string(),
                entries: Vec::new(),
            })),
        ));
    }

    #[test]
    fn inactive_transfer_progress_does_not_request_ui_refresh() {
        let progress = TransferJobEvent::Progress(SftpTransferProgress {
            remote_path: "/tmp/file".to_string(),
            local_path: PathBuf::from("/tmp/file"),
            bytes_transferred: 1,
            total_bytes: Some(2),
            item_count_completed: None,
            item_count_total: None,
        });

        assert!(transfer_event_needs_ui_refresh(
            Some("session-a"),
            Some("session-a"),
            &progress,
        ));
        assert!(!transfer_event_needs_ui_refresh(
            Some("session-a"),
            Some("session-b"),
            &progress,
        ));
        assert!(transfer_event_needs_ui_refresh(
            Some("session-a"),
            Some("session-b"),
            &TransferJobEvent::Finished(Err("failed".to_string())),
        ));
    }
}
