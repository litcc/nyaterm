use gpui::{Pixels, ScrollHandle, UniformListScrollHandle, px};
use nyaterm_transport::{
    SftpFileEntry, SftpFileProperties, SftpRemoteTextFile, SftpTransferControl,
    SftpTransferProgress, SftpTransferSummary, SftpWriteTextResult,
};
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TransferJobKind {
    ListDir {
        remote_path: String,
        select_after: Option<String>,
    },
    ListChildren {
        remote_path: String,
    },
    ResolveHome,
    SyncCwd,
    Download {
        remote_path: String,
        raw_path_token: Option<String>,
        local_path: PathBuf,
    },
    Upload {
        local_path: PathBuf,
        remote_path: String,
    },
    Rename {
        old_path: String,
        new_path: String,
        parent_path: String,
    },
    Move {
        old_path: String,
        new_path: String,
        parent_path: String,
    },
    Delete {
        remote_path: String,
        parent_path: String,
    },
    Mkdir {
        remote_path: String,
        parent_path: String,
    },
    CreateFile {
        remote_path: String,
        parent_path: String,
    },
    Symlink {
        link_path: String,
        target_path: String,
        parent_path: String,
    },
    LoadProperties {
        remote_path: String,
    },
    UpdateProperties {
        remote_path: String,
        parent_path: String,
    },
    LoadEditor {
        remote_path: String,
        tab_id: String,
    },
    SaveEditor {
        remote_path: String,
        tab_id: String,
    },
    OpenExternal {
        remote_path: String,
        local_path: PathBuf,
    },
    AiFileAction {
        remote_path: String,
        action_id: String,
        action_name: String,
    },
    /// In-band ZMODEM upload (local files -> remote `rz`).
    ZmodemUpload {
        session_id: String,
        file_name: String,
    },
    /// In-band ZMODEM download (remote `sz` -> local directory).
    ZmodemDownload {
        session_id: String,
        file_name: String,
    },
    /// In-band trzsz download (remote `tsz` -> local download directory).
    TrzszDownload {
        session_id: String,
        file_name: String,
    },
    /// In-band trzsz upload (local files -> remote `trz`).
    TrzszUpload {
        session_id: String,
        file_name: String,
    },
    /// Pre-upload SFTP name conflict probe before remote `rz` (Tauri parity).
    ZmodemConflictProbe {
        session_id: String,
        remote_dir: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransferJobStatus {
    Running,
    Paused,
    Cancelling,
    Cancelled,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub(crate) struct TransferJobState {
    pub(crate) id: String,
    pub(crate) session_id: Option<String>,
    pub(crate) kind: TransferJobKind,
    pub(crate) status: TransferJobStatus,
    pub(crate) detail: String,
    pub(crate) created_at_ms: u128,
    pub(crate) display_name: String,
    pub(crate) entries: Vec<SftpFileEntry>,
    pub(crate) summary: Option<SftpTransferSummary>,
    pub(crate) progress: Option<SftpTransferProgress>,
    pub(crate) control: Option<SftpTransferControl>,
}

impl TransferJobState {
    pub(crate) fn now_ms() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0)
    }

    pub(crate) fn display_name_for_kind(kind: &TransferJobKind) -> String {
        match kind {
            TransferJobKind::Download { remote_path, .. }
            | TransferJobKind::OpenExternal { remote_path, .. }
            | TransferJobKind::LoadEditor { remote_path, .. }
            | TransferJobKind::SaveEditor { remote_path, .. }
            | TransferJobKind::LoadProperties { remote_path }
            | TransferJobKind::UpdateProperties { remote_path, .. }
            | TransferJobKind::AiFileAction { remote_path, .. }
            | TransferJobKind::Delete { remote_path, .. }
            | TransferJobKind::Mkdir { remote_path, .. }
            | TransferJobKind::CreateFile { remote_path, .. }
            | TransferJobKind::Symlink {
                link_path: remote_path,
                ..
            }
            | TransferJobKind::ListChildren { remote_path }
            | TransferJobKind::ListDir { remote_path, .. } => remote_file_name(remote_path),
            TransferJobKind::Upload { local_path, .. } => local_file_name(local_path),
            TransferJobKind::Rename { new_path, .. } | TransferJobKind::Move { new_path, .. } => {
                remote_file_name(new_path)
            }
            TransferJobKind::ZmodemUpload { file_name, .. }
            | TransferJobKind::ZmodemDownload { file_name, .. }
            | TransferJobKind::TrzszDownload { file_name, .. }
            | TransferJobKind::TrzszUpload { file_name, .. } => file_name.clone(),
            TransferJobKind::ResolveHome | TransferJobKind::SyncCwd => String::new(),
            TransferJobKind::ZmodemConflictProbe { remote_dir, .. } => remote_file_name(remote_dir),
        }
    }

    pub(crate) fn ensure_presentation_fields(&mut self) {
        if self.created_at_ms == 0 {
            self.created_at_ms = Self::now_ms();
        }
        if self.display_name.trim().is_empty() {
            self.display_name = Self::display_name_for_kind(&self.kind);
        }
    }

    pub(crate) fn is_user_transfer(&self) -> bool {
        matches!(
            &self.kind,
            TransferJobKind::Download { .. }
                | TransferJobKind::Upload { .. }
                | TransferJobKind::OpenExternal { .. }
                | TransferJobKind::ZmodemUpload { .. }
                | TransferJobKind::ZmodemDownload { .. }
                | TransferJobKind::TrzszDownload { .. }
                | TransferJobKind::TrzszUpload { .. }
        )
    }

    pub(crate) fn is_visible_for_session(&self, session_id: Option<&str>) -> bool {
        self.is_user_transfer()
            && session_id.is_none_or(|session_id| self.session_id.as_deref() == Some(session_id))
    }
}

fn remote_file_name(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn local_file_name(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod transfer_job_state_tests {
    use std::path::PathBuf;

    use super::{TransferJobKind, TransferJobState, TransferJobStatus};

    fn job(kind: TransferJobKind, session_id: Option<&str>) -> TransferJobState {
        TransferJobState {
            id: "job-1".to_string(),
            session_id: session_id.map(ToString::to_string),
            kind,
            status: TransferJobStatus::Completed,
            detail: String::new(),
            created_at_ms: 1,
            display_name: String::new(),
            entries: Vec::new(),
            summary: None,
            progress: None,
            control: None,
        }
    }

    #[test]
    fn user_transfers_are_scoped_to_the_active_session() {
        let download = job(
            TransferJobKind::Download {
                remote_path: "/remote/file".to_string(),
                raw_path_token: None,
                local_path: PathBuf::from("/local/file"),
            },
            Some("session-a"),
        );

        assert!(download.is_visible_for_session(None));
        assert!(download.is_visible_for_session(Some("session-a")));
        assert!(!download.is_visible_for_session(Some("session-b")));
    }

    #[test]
    fn internal_jobs_do_not_appear_in_the_transfer_queue() {
        let list = job(
            TransferJobKind::ListDir {
                remote_path: "/remote".to_string(),
                select_after: None,
            },
            Some("session-a"),
        );

        assert!(!list.is_user_transfer());
        assert!(!list.is_visible_for_session(None));
        assert!(!list.is_visible_for_session(Some("session-a")));
    }

    #[test]
    fn external_editor_downloads_are_visible_user_transfers() {
        let external = job(
            TransferJobKind::OpenExternal {
                remote_path: "/remote/file.txt".to_string(),
                local_path: PathBuf::from("/tmp/file.txt"),
            },
            Some("session-a"),
        );

        assert!(external.is_user_transfer());
        assert!(external.is_visible_for_session(None));
        assert!(external.is_visible_for_session(Some("session-a")));
        assert!(!external.is_visible_for_session(Some("session-b")));
    }

    #[test]
    fn display_names_are_stable_from_initial_transfer_kind() {
        assert_eq!(
            TransferJobState::display_name_for_kind(&TransferJobKind::OpenExternal {
                remote_path: "/remote/project/file.txt".to_string(),
                local_path: PathBuf::from("/tmp/file.txt"),
            }),
            "file.txt"
        );
        assert_eq!(
            TransferJobState::display_name_for_kind(&TransferJobKind::Download {
                remote_path: "/remote/project/".to_string(),
                raw_path_token: None,
                local_path: PathBuf::from("/tmp/project"),
            }),
            "project"
        );
        assert_eq!(
            TransferJobState::display_name_for_kind(&TransferJobKind::Upload {
                local_path: PathBuf::from("/tmp/local.bin"),
                remote_path: "/remote/local.bin".to_string(),
            }),
            "local.bin"
        );
    }
}

#[derive(Debug)]
pub(crate) struct TransferJobResult {
    pub(crate) id: String,
    pub(crate) event: TransferJobEvent,
}

#[derive(Debug)]
pub(crate) enum TransferJobEvent {
    Started {
        detail: String,
    },
    ExternalModified {
        remote_path: String,
        raw_path_token: Option<String>,
        local_path: PathBuf,
    },
    Progress(SftpTransferProgress),
    Finished(Result<TransferJobOutput, String>),
}

#[derive(Debug)]
pub(crate) enum TransferJobOutput {
    Entries(Vec<SftpFileEntry>),
    ChildEntries {
        remote_path: String,
        entries: Vec<SftpFileEntry>,
    },
    HomeDir(String),
    CwdSynced {
        remote_path: String,
        entries: Vec<SftpFileEntry>,
    },
    Summary(SftpTransferSummary),
    Uploaded {
        summary: SftpTransferSummary,
        parent_path: String,
        entries: Vec<SftpFileEntry>,
    },
    Renamed {
        old_path: String,
        new_path: String,
        parent_path: String,
        entries: Vec<SftpFileEntry>,
    },
    Moved {
        old_path: String,
        new_path: String,
        parent_path: String,
        entries: Vec<SftpFileEntry>,
    },
    Deleted {
        remote_path: String,
        parent_path: String,
        entries: Vec<SftpFileEntry>,
    },
    CreatedDirectory {
        remote_path: String,
        parent_path: String,
        entries: Vec<SftpFileEntry>,
        open_after_create: bool,
    },
    CreatedFile {
        remote_path: String,
        parent_path: String,
        entries: Vec<SftpFileEntry>,
        open_after_create: bool,
    },
    CreatedSymlink {
        link_path: String,
        target_path: String,
        parent_path: String,
        entries: Vec<SftpFileEntry>,
    },
    PropertiesLoaded {
        remote_path: String,
        properties: SftpFileProperties,
    },
    PropertiesUpdated {
        remote_path: String,
        parent_path: String,
        properties: SftpFileProperties,
        entries: Vec<SftpFileEntry>,
    },
    EditorLoaded {
        tab_id: String,
        remote_path: String,
        file: SftpRemoteTextFile,
    },
    EditorSaved {
        tab_id: String,
        remote_path: String,
        result: SftpWriteTextResult,
    },
    ExternalOpened {
        remote_path: String,
        local_path: PathBuf,
    },
    AiFileActionLoaded {
        remote_path: String,
        action_id: String,
        action_name: String,
        prompt: String,
        file: SftpRemoteTextFile,
    },

    ZmodemProbeReady {
        session_id: String,
        files: Vec<PathBuf>,
        probe_skipped: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TransferBrowserSessionCacheState {
    /// Shared, and always replaced whole.
    ///
    /// The browser listing is swapped in and out of caches and navigation snapshots,
    /// and the filter/sort memo keys on this pointer. Both want the same thing: one
    /// allocation handed around rather than deep-copied, and a pointer that changes
    /// exactly when the listing does.
    pub(crate) entries: Arc<Vec<SftpFileEntry>>,
    pub(crate) current_path: String,
    pub(crate) current_raw_path_token: Option<String>,
    pub(crate) home_dir: String,
    pub(crate) history: VecDeque<String>,
    pub(crate) history_index: usize,
    pub(crate) visited_history: VecDeque<String>,
}

#[derive(Clone)]
pub(crate) struct TransferBrowserNavigationSnapshot {
    pub(crate) remote_path: String,
    pub(crate) browser_path: String,
    pub(crate) browser_raw_path_token: Option<String>,
    /// Shared, and always replaced whole.
    ///
    /// The browser listing is swapped in and out of caches and navigation snapshots,
    /// and the filter/sort memo keys on this pointer. Both want the same thing: one
    /// allocation handed around rather than deep-copied, and a pointer that changes
    /// exactly when the listing does.
    pub(crate) entries: Arc<Vec<SftpFileEntry>>,
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
    pub(crate) status: String,
    pub(crate) history: VecDeque<String>,
    pub(crate) history_index: usize,
    pub(crate) visited_history: VecDeque<String>,
    pub(crate) selected_path: Option<String>,
    pub(crate) selected_paths: HashSet<String>,
    pub(crate) list_scroll: UniformListScrollHandle,
    pub(crate) horizontal_scroll: ScrollHandle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransferBrowserSortColumn {
    Name,
    Modified,
    Size,
    Permissions,
    Owner,
    Group,
}

impl TransferBrowserSortColumn {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Modified => "Modified",
            Self::Size => "Size",
            Self::Permissions => "Perms",
            Self::Owner => "Owner",
            Self::Group => "Group",
        }
    }

    pub(crate) fn default_direction(self) -> TransferBrowserSortDirection {
        match self {
            Self::Name | Self::Permissions | Self::Owner | Self::Group => {
                TransferBrowserSortDirection::Ascending
            }
            Self::Size | Self::Modified => TransferBrowserSortDirection::Descending,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransferBrowserSortDirection {
    Ascending,
    Descending,
}

impl TransferBrowserSortDirection {
    pub(crate) fn marker(self) -> &'static str {
        match self {
            Self::Ascending => "up",
            Self::Descending => "down",
        }
    }

    pub(crate) fn toggled(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TransferBrowserColumnWidths {
    pub(crate) name: Pixels,
    pub(crate) modified: Pixels,
    pub(crate) size: Pixels,
    pub(crate) permissions: Pixels,
    pub(crate) owner: Pixels,
    pub(crate) group: Pixels,
}

impl Default for TransferBrowserColumnWidths {
    fn default() -> Self {
        Self {
            name: px(220.),
            modified: px(128.),
            size: px(80.),
            permissions: px(112.),
            owner: px(96.),
            group: px(96.),
        }
    }
}

impl TransferBrowserColumnWidths {
    pub(crate) fn get(self, column: TransferBrowserSortColumn) -> Pixels {
        match column {
            TransferBrowserSortColumn::Name => self.name,
            TransferBrowserSortColumn::Modified => self.modified,
            TransferBrowserSortColumn::Size => self.size,
            TransferBrowserSortColumn::Permissions => self.permissions,
            TransferBrowserSortColumn::Owner => self.owner,
            TransferBrowserSortColumn::Group => self.group,
        }
    }

    pub(crate) fn set(&mut self, column: TransferBrowserSortColumn, width: Pixels) {
        let width = if width < Self::min_width(column) {
            Self::min_width(column)
        } else {
            width
        };
        match column {
            TransferBrowserSortColumn::Name => self.name = width,
            TransferBrowserSortColumn::Modified => self.modified = width,
            TransferBrowserSortColumn::Size => self.size = width,
            TransferBrowserSortColumn::Permissions => self.permissions = width,
            TransferBrowserSortColumn::Owner => self.owner = width,
            TransferBrowserSortColumn::Group => self.group = width,
        }
    }

    pub(crate) fn min_width(column: TransferBrowserSortColumn) -> Pixels {
        match column {
            TransferBrowserSortColumn::Name => px(140.),
            TransferBrowserSortColumn::Modified => px(112.),
            TransferBrowserSortColumn::Size => px(72.),
            TransferBrowserSortColumn::Permissions => px(92.),
            TransferBrowserSortColumn::Owner => px(76.),
            TransferBrowserSortColumn::Group => px(76.),
        }
    }
}
