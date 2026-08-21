use futures::channel::mpsc::unbounded;
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;

use gpui::{ScrollHandle, ScrollStrategy, TestAppContext, UniformListScrollHandle, point, px};
use nyaterm_transport::{
    SftpDuplicatePolicy, SftpFileEntry, SftpFileProperties, SftpFileType, SftpTransferControl,
    SftpWriteTextResult,
};

use crate::models::{
    TransferBrowserContextTarget, TransferBrowserNavigationSnapshot,
    TransferBrowserSessionCacheState, TransferEditorField, TransferEditorState,
    TransferExternalSyncPromptState, TransferJobEvent, TransferJobKind, TransferJobResult,
    TransferJobState, TransferJobStatus, TransferNewFolderState, TransferPathPromptKind,
    TransferPropertiesState, TransferRenameState,
};

use super::{
    TransferEditorCloseAfterSave, TransferEditorCloseOutcome, TransferEditorSaveOutcome,
    TransferFeatureFocus, TransferFeatureState, TransferPanelState, TransferPathState,
    TransferQueueState,
};

fn transfer_focus(cx: &TestAppContext) -> TransferFeatureFocus {
    cx.update(|cx| TransferFeatureFocus {
        panel: cx.focus_handle(),
        queue: cx.focus_handle(),
        browser: cx.focus_handle(),
        editor: cx.focus_handle(),
        external_sync: cx.focus_handle(),
    })
}

fn transfer_state(cx: &TestAppContext) -> TransferFeatureState {
    TransferFeatureState::new(
        ".".to_string(),
        String::new(),
        SftpDuplicatePolicy::Ask,
        180.,
        transfer_focus(cx),
    )
}

fn file_entry(path: &str) -> SftpFileEntry {
    SftpFileEntry {
        name: path.rsplit('/').next().unwrap_or(path).to_string(),
        path: path.to_string(),
        file_type: SftpFileType::File,
        size: Some(12),
        permissions: Some(0o640),
        owner: "owner".to_string(),
        group: "group".to_string(),
        modified_at: Some(1),
        raw_path_token: None,
        symlink_target_is_directory: false,
    }
}

#[test]
fn browser_rename_click_requires_selection_before_mouse_down() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);

    assert!(!transfer.arm_browser_rename_click("/first.txt", true));
    transfer.select_browser_entry("/first.txt".to_string());
    assert!(!transfer.consume_browser_rename_click("/first.txt"));

    assert!(transfer.arm_browser_rename_click("/first.txt", true));
    let browser = transfer.browser_view();
    assert_eq!(browser.selected_remote_path.as_deref(), Some("/first.txt"));
    assert!(transfer.consume_browser_rename_click("/first.txt"));

    assert!(!transfer.arm_browser_rename_click("/first.txt", false));
    assert!(!transfer.consume_browser_rename_click("/first.txt"));

    transfer.replace_browser_selection(
        HashSet::from(["/first.txt".to_string(), "/second.txt".to_string()]),
        Some("/first.txt".to_string()),
    );
    assert!(!transfer.arm_browser_rename_click("/first.txt", true));
    assert!(!transfer.consume_browser_rename_click("/first.txt"));
}

#[test]
fn browser_context_target_cancels_armed_rename_click() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);
    transfer.select_browser_entry("/first.txt".to_string());
    assert!(transfer.arm_browser_rename_click("/first.txt", true));

    transfer.set_browser_context_target(TransferBrowserContextTarget::Entry(
        "/first.txt".to_string(),
    ));

    assert_eq!(
        transfer.browser_view().context_target,
        &TransferBrowserContextTarget::Entry("/first.txt".to_string())
    );
    assert!(!transfer.consume_browser_rename_click("/first.txt"));
}

#[test]
fn browser_external_drop_hover_tracks_overlay_visibility() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);

    assert!(!transfer.browser_view().external_drop_hover);
    assert!(transfer.set_browser_external_drop_hover(true));
    assert!(transfer.browser_external_drop_hover_is_pending());
    assert!(transfer.browser_view().external_drop_hover);
    assert!(!transfer.set_browser_external_drop_hover(true));
    assert!(transfer.set_browser_external_drop_hover(false));
    assert!(!transfer.browser_external_drop_hover_is_pending());
    assert!(!transfer.browser_view().external_drop_hover);
}

#[test]
fn browser_entry_context_target_preserves_an_existing_multi_selection() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);
    let selected = HashSet::from(["/first.txt".to_string(), "/second.txt".to_string()]);
    transfer.replace_browser_selection(selected.clone(), Some("/second.txt".to_string()));

    transfer.set_browser_context_target(TransferBrowserContextTarget::Entry(
        "/first.txt".to_string(),
    ));

    assert_eq!(transfer.activate_marked_browser_path("/first.txt"), Some(2));
    assert_eq!(transfer.browser.selected_remote_paths, selected);
    assert_eq!(
        transfer.browser.selected_remote_path.as_deref(),
        Some("/first.txt")
    );
}

#[test]
fn browser_navigation_clears_the_rename_click_candidate() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);
    transfer.select_browser_entry("/first.txt".to_string());
    assert!(transfer.arm_browser_rename_click("/first.txt", true));

    transfer.begin_browser_directory_load("/next".to_string());

    assert!(!transfer.consume_browser_rename_click("/first.txt"));
    assert_eq!(
        transfer.browser.context_target,
        TransferBrowserContextTarget::CurrentDirectory
    );
}

fn file_properties(path: &str) -> SftpFileProperties {
    SftpFileProperties {
        name: path.rsplit('/').next().unwrap_or(path).to_string(),
        path: path.to_string(),
        file_type: SftpFileType::File,
        size: Some(12),
        permissions: Some(0o600),
        permissions_symbolic: "rw-------".to_string(),
        owner: "updated-owner".to_string(),
        group: "updated-group".to_string(),
        uid: Some(1000),
        gid: Some(1000),
        modified_at: Some(2),
        accessed_at: Some(3),
        raw_path_token: None,
        symlink_target_is_directory: false,
    }
}

#[test]
fn browser_history_discards_the_forward_branch_and_tracks_visits() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);
    transfer.browser.history =
        VecDeque::from(["/three".to_string(), "/two".to_string(), "/one".to_string()]);
    transfer.browser.history_index = 1;

    transfer.record_browser_history("/four".to_string());

    assert_eq!(
        transfer.browser.history,
        VecDeque::from(["/four".to_string(), "/two".to_string(), "/one".to_string(),])
    );
    assert_eq!(transfer.browser.history_index, 0);
    assert_eq!(
        transfer.browser.visited_history.front().map(String::as_str),
        Some("/four")
    );
}

#[test]
fn browser_session_restore_clamps_history_and_clears_interaction() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);
    transfer.select_browser_entry("/stale.txt".to_string());
    assert!(
        transfer
            .schedule_browser_pending_rename("/stale.txt")
            .is_some()
    );
    transfer.store_browser_session_cache(
        "session-a".to_string(),
        TransferBrowserSessionCacheState {
            entries: vec![file_entry("/srv/current.txt")],
            current_path: "/srv".to_string(),
            current_raw_path_token: None,
            home_dir: "/home/test".to_string(),
            history: VecDeque::from(["/srv".to_string()]),
            history_index: 99,
            visited_history: VecDeque::from(["/srv".to_string()]),
        },
    );
    transfer
        .browser
        .horizontal_scroll
        .set_offset(point(px(-24.), px(0.)));

    assert_eq!(
        transfer.restore_browser_session_cache("session-a"),
        Some("/srv".to_string())
    );
    assert!(transfer.browser.pending_rename.is_none());
    let browser = transfer.browser_view();
    assert_eq!(browser.path.as_str(), "/srv");
    assert_eq!(browser.history_index, 0);
    assert!(browser.selected_remote_paths.is_empty());
    assert_eq!(browser.entries.len(), 1);
    assert_eq!(browser.horizontal_scroll.offset().x, px(0.));
}

#[test]
fn browser_session_restore_preserves_the_raw_directory_token() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);
    let remote =
        nyaterm_transport::RemoteFilePath::from_raw("/srv/non-utf8-?", b"/srv/non-utf8-\xff");
    transfer.store_browser_session_cache(
        "session-a".to_string(),
        TransferBrowserSessionCacheState {
            entries: Vec::new(),
            current_path: remote.display_path.clone(),
            current_raw_path_token: remote.raw_path_token.clone(),
            home_dir: "/home/test".to_string(),
            history: VecDeque::from([remote.display_path.clone()]),
            history_index: 0,
            visited_history: VecDeque::new(),
        },
    );

    transfer.restore_browser_session_cache("session-a").unwrap();

    assert_eq!(transfer.browser_remote_file_path(), remote);
}

#[test]
fn browser_navigation_restores_the_stable_pending_snapshot() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);
    transfer.browser.path = "/optimistic".to_string();
    transfer
        .browser
        .navigation_jobs
        .insert("session-a".to_string(), "list-1".to_string());
    let list_scroll = UniformListScrollHandle::new();
    list_scroll.scroll_to_item_strict(3, ScrollStrategy::Top);
    let horizontal_scroll = ScrollHandle::new();
    horizontal_scroll.set_offset(point(px(-24.), px(0.)));
    let stable = TransferBrowserNavigationSnapshot {
        remote_path: "/stable".to_string(),
        browser_path: "/stable".to_string(),
        browser_raw_path_token: None,
        entries: vec![file_entry("/stable/file.txt")],
        loading: false,
        error: None,
        status: "stable".to_string(),
        history: VecDeque::from(["/stable".to_string()]),
        history_index: 0,
        visited_history: VecDeque::from(["/stable".to_string()]),
        selected_path: None,
        selected_paths: Default::default(),
        list_scroll,
        horizontal_scroll,
    };
    transfer
        .browser
        .pending_navigations
        .insert("list-1".to_string(), stable.clone());

    let rollback = transfer.prepare_browser_navigation("session-a", "/optimistic".to_string());

    assert_eq!(rollback.browser_path, "/stable");
    assert_eq!(transfer.browser.path, "/stable");
    assert_eq!(transfer.browser.list_scroll.logical_scroll_top_index(), 3);
    assert_eq!(transfer.browser.horizontal_scroll.offset().x, px(-24.));
    assert!(!transfer.browser.navigation_jobs.contains_key("session-a"));
    assert!(!transfer.browser.pending_navigations.contains_key("list-1"));
}

#[test]
fn browser_navigation_and_filters_reset_horizontal_scroll() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);

    transfer
        .browser
        .horizontal_scroll
        .set_offset(point(px(-24.), px(0.)));
    transfer.set_browser_search("term".to_string());
    assert_eq!(transfer.browser.horizontal_scroll.offset().x, px(0.));

    transfer
        .browser
        .horizontal_scroll
        .set_offset(point(px(-24.), px(0.)));
    transfer.toggle_browser_sort(crate::models::TransferBrowserSortColumn::Modified);
    assert_eq!(transfer.browser.horizontal_scroll.offset().x, px(0.));

    transfer
        .browser
        .horizontal_scroll
        .set_offset(point(px(-24.), px(0.)));
    transfer.begin_browser_directory_load("/next".to_string());
    assert_eq!(transfer.browser.horizontal_scroll.offset().x, px(0.));

    transfer
        .browser
        .horizontal_scroll
        .set_offset(point(px(-24.), px(0.)));
    transfer.reset_browser_for_session(true);
    assert_eq!(transfer.browser.horizontal_scroll.offset().x, px(0.));
}

#[test]
fn transfer_session_id_migration_preserves_reconnected_sftp_state() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);
    let cache = TransferBrowserSessionCacheState {
        entries: vec![file_entry("/srv/app.txt")],
        current_path: "/srv".to_string(),
        current_raw_path_token: None,
        home_dir: "/home/nya".to_string(),
        history: VecDeque::from(["/srv".to_string()]),
        history_index: 0,
        visited_history: VecDeque::from(["/srv".to_string()]),
    };
    transfer.store_browser_session_cache("old-session".to_string(), cache);
    transfer.store_browser_session_cache(
        "new-session".to_string(),
        TransferBrowserSessionCacheState {
            entries: vec![file_entry("/stale/file.txt")],
            current_path: "/stale".to_string(),
            current_raw_path_token: None,
            home_dir: "/stale".to_string(),
            history: VecDeque::from(["/stale".to_string()]),
            history_index: 0,
            visited_history: VecDeque::from(["/stale".to_string()]),
        },
    );
    transfer
        .browser
        .navigation_jobs
        .insert("old-session".to_string(), "sftp-list-1".to_string());
    transfer.browser.pending_navigations.insert(
        "sftp-list-1".to_string(),
        TransferBrowserNavigationSnapshot {
            remote_path: "/srv".to_string(),
            browser_path: "/srv".to_string(),
            browser_raw_path_token: None,
            entries: vec![file_entry("/srv/old.txt")],
            loading: true,
            error: None,
            status: "listing".to_string(),
            history: VecDeque::from(["/srv".to_string()]),
            history_index: 0,
            visited_history: VecDeque::new(),
            selected_path: None,
            selected_paths: HashSet::new(),
            list_scroll: UniformListScrollHandle::new(),
            horizontal_scroll: ScrollHandle::new(),
        },
    );
    transfer.browser.pending_navigations.insert(
        "orphan-list".to_string(),
        TransferBrowserNavigationSnapshot {
            remote_path: "/tmp".to_string(),
            browser_path: "/tmp".to_string(),
            browser_raw_path_token: None,
            entries: Vec::new(),
            loading: false,
            error: None,
            status: "orphan".to_string(),
            history: VecDeque::new(),
            history_index: 0,
            visited_history: VecDeque::new(),
            selected_path: None,
            selected_paths: HashSet::new(),
            list_scroll: UniformListScrollHandle::new(),
            horizontal_scroll: ScrollHandle::new(),
        },
    );
    transfer.enqueue_transfer_job(TransferJobState {
        id: "download".to_string(),
        session_id: Some("old-session".to_string()),
        kind: TransferJobKind::Download {
            remote_path: "/srv/app.txt".to_string(),
            raw_path_token: None,
            local_path: PathBuf::from("/tmp/app.txt"),
        },
        status: TransferJobStatus::Running,
        detail: "Downloading".to_string(),
        created_at_ms: TransferJobState::now_ms(),
        display_name: String::new(),
        entries: Vec::new(),
        summary: None,
        progress: None,
        control: None,
    });
    transfer.enqueue_transfer_job(TransferJobState {
        id: "upload".to_string(),
        session_id: Some("old-session".to_string()),
        kind: TransferJobKind::Upload {
            local_path: PathBuf::from("/tmp/app.txt"),
            remote_path: "/srv/app.txt".to_string(),
        },
        status: TransferJobStatus::Running,
        detail: "Uploading".to_string(),
        created_at_ms: TransferJobState::now_ms(),
        display_name: String::new(),
        entries: Vec::new(),
        summary: None,
        progress: None,
        control: None,
    });
    transfer.enqueue_transfer_job(TransferJobState {
        id: "list".to_string(),
        session_id: Some("old-session".to_string()),
        kind: TransferJobKind::ListDir {
            remote_path: "/srv".to_string(),
            select_after: None,
        },
        status: TransferJobStatus::Running,
        detail: "Listing".to_string(),
        created_at_ms: TransferJobState::now_ms(),
        display_name: String::new(),
        entries: Vec::new(),
        summary: None,
        progress: None,
        control: None,
    });

    assert!(transfer.replace_session_id("old-session", "new-session"));
    transfer.remove_browser_session_cache("old-session");

    assert!(transfer.has_browser_session_cache("new-session"));
    assert!(!transfer.has_browser_session_cache("old-session"));
    assert_eq!(
        transfer
            .restore_browser_session_cache("new-session")
            .as_deref(),
        Some("/srv")
    );
    assert_eq!(
        transfer
            .browser
            .navigation_jobs
            .get("new-session")
            .map(String::as_str),
        Some("sftp-list-1")
    );
    assert!(!transfer.browser.navigation_jobs.contains_key("old-session"));
    assert!(
        transfer
            .browser
            .pending_navigations
            .contains_key("sftp-list-1")
    );
    assert!(
        !transfer
            .browser
            .pending_navigations
            .contains_key("orphan-list")
    );
    assert_eq!(
        transfer.transfer_jobs()[0].session_id.as_deref(),
        Some("new-session")
    );
    assert_eq!(
        transfer.transfer_jobs()[1].session_id.as_deref(),
        Some("new-session")
    );
    assert_eq!(
        transfer.transfer_jobs()[2].session_id.as_deref(),
        Some("old-session")
    );
}

#[test]
fn browser_selection_replacement_preserves_the_explicit_active_path() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);
    transfer.select_browser_entry("/active".to_string());

    let selected = ["/base".to_string()].into_iter().collect();
    let selected_count = transfer.replace_browser_selection(selected, Some("/active".to_string()));

    assert_eq!(selected_count, 1);
    assert_eq!(
        transfer.browser.selected_remote_path.as_deref(),
        Some("/active")
    );
    assert_eq!(
        transfer.browser.selected_remote_paths,
        ["/base".to_string()].into_iter().collect()
    );
}

fn transfer_queue(cx: &TestAppContext) -> TransferQueueState {
    let (tx, rx) = unbounded();
    let focus = cx.update(|cx| cx.focus_handle());
    TransferQueueState::new(tx, rx, focus)
}

fn transfer_job(
    id: &str,
    session_id: &str,
    status: TransferJobStatus,
    controlled: bool,
) -> TransferJobState {
    TransferJobState {
        id: id.to_string(),
        session_id: Some(session_id.to_string()),
        kind: TransferJobKind::Download {
            remote_path: format!("/remote/{id}"),
            raw_path_token: None,
            local_path: PathBuf::from(format!("/local/{id}")),
        },
        status,
        detail: String::new(),
        created_at_ms: TransferJobState::now_ms(),
        display_name: String::new(),
        entries: Vec::new(),
        summary: None,
        progress: None,
        control: controlled.then(SftpTransferControl::new),
    }
}

fn external_sync_prompt(session_id: Option<&str>, job_id: &str) -> TransferExternalSyncPromptState {
    TransferExternalSyncPromptState {
        session_id: session_id.map(str::to_string),
        job_id: job_id.to_string(),
        remote_path: format!("/remote/{job_id}.txt"),
        raw_path_token: None,
        local_path: PathBuf::from(format!("/local/{job_id}.txt")),
    }
}

fn editor_tab(session_id: &str, remote_path: &str) -> TransferEditorState {
    TransferEditorState {
        id: TransferEditorState::tab_id(Some(session_id), remote_path),
        session_id: Some(session_id.to_string()),
        remote_path: remote_path.to_string(),
        raw_path_token: None,
        name: remote_path.rsplit('/').next().unwrap().to_string(),
        content: String::new(),
        search_query: String::new(),
        active_match: 0,
        base_size: Some(0),
        base_modified_at: Some(1),
        loading: false,
        saving: false,
        dirty: false,
        conflict: false,
        close_after_save: false,
        reload_confirm: false,
        error: None,
        focused_field: TransferEditorField::Content,
    }
}

#[test]
fn transfer_paths_own_endpoints_policy_and_prompt_admission() {
    let mut paths = TransferPathState::new(
        "  ".to_string(),
        "/tmp/download".to_string(),
        SftpDuplicatePolicy::Ask,
    );

    assert_eq!(paths.normalized_remote_path(), ".");
    assert_eq!(paths.local_path(), "/tmp/download");
    assert_eq!(paths.duplicate_policy(), SftpDuplicatePolicy::Ask);

    paths.set_remote_path("/srv/files");
    paths.set_local_path("/tmp/upload");
    paths.set_duplicate_policy(SftpDuplicatePolicy::Overwrite);
    assert_eq!(paths.remote_path(), "/srv/files");
    assert_eq!(paths.normalized_remote_path(), "/srv/files");
    assert_eq!(paths.local_path(), "/tmp/upload");
    assert_eq!(paths.duplicate_policy(), SftpDuplicatePolicy::Overwrite);

    assert!(paths.begin_prompt(TransferPathPromptKind::UploadFile));
    assert!(!paths.begin_prompt(TransferPathPromptKind::DownloadDirectory));
    assert!(!paths.finish_prompt(TransferPathPromptKind::DownloadDirectory));
    assert!(!paths.begin_prompt(TransferPathPromptKind::UploadDirectory));
    assert!(paths.finish_prompt(TransferPathPromptKind::UploadFile));
    assert!(!paths.finish_prompt(TransferPathPromptKind::UploadFile));
}

#[test]
fn transfer_panel_owns_focus_height_and_resize_lifecycle() {
    let cx = TestAppContext::single();
    let mut panel = TransferPanelState {
        focus: cx.update(|cx| cx.focus_handle()),
        height: 120.,
        height_resize: None,
    };

    panel.start_height_resize(px(400.));
    assert_eq!(panel.update_height_resize(px(450.)), Some(70.));
    assert_eq!(panel.update_height_resize(px(800.)), Some(60.));
    assert!(panel.finish_height_resize());
    assert!(!panel.finish_height_resize());
    assert!(panel.update_height_resize(px(300.)).is_none());

    panel.start_height_resize(px(400.));
    assert_eq!(panel.update_height_resize(px(-200.)), Some(600.));
    assert!(panel.finish_height_resize());
}

#[test]
fn transfer_file_ops_track_real_rename_input_focus_and_creation_options() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);

    transfer.schedule_rename_focus();
    assert!(!transfer.rename_focus_is_pending());
    transfer.open_rename_dialog(TransferRenameState {
        old_path: "/srv/old".to_string(),
        raw_path_token: None,
        initial_name: "old".to_string(),
        value: "old".to_string(),
    });
    transfer.schedule_rename_focus();
    assert!(transfer.rename_focus_is_pending());
    assert_eq!(
        transfer.pending_rename_input_id().as_deref(),
        Some("transfer.rename./srv/old")
    );
    transfer.finish_rename_focus();
    assert!(!transfer.rename_focus_is_pending());
    transfer.schedule_rename_focus();
    transfer.close_rename_dialog();
    assert!(!transfer.rename_dialog_is_open());
    assert!(!transfer.rename_focus_is_pending());
    assert!(transfer.pending_rename_input_id().is_none());
    transfer.close_rename_dialog();
    assert!(!transfer.rename_dialog_is_open());
    assert!(!transfer.rename_focus_is_pending());

    transfer.open_new_folder_dialog(TransferNewFolderState {
        parent_path: "/srv".to_string(),
        value: String::new(),
        mode: 0o755,
        open_after_create: false,
    });
    assert!(transfer.set_new_folder_name("logs".to_string()));
    assert!(transfer.toggle_new_folder_open_after_create());
    assert!(transfer.toggle_new_folder_mode_bit(0o020));
    let folder = transfer
        .new_folder_dialog()
        .expect("new folder dialog should remain open");
    assert_eq!(folder.value, "logs");
    assert!(folder.open_after_create);
    assert_eq!(folder.mode, 0o775);
}

#[test]
fn external_sync_prompts_are_filtered_by_session_and_window_admission() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);
    transfer.insert_external_sync_prompt(
        "prompt-a".to_string(),
        external_sync_prompt(Some("session-a"), "job-a"),
    );
    transfer.insert_external_sync_prompt(
        "prompt-b".to_string(),
        external_sync_prompt(Some("session-b"), "job-b"),
    );

    assert_eq!(
        transfer
            .active_external_sync_prompt("session-a")
            .map(|(prompt_id, _)| prompt_id),
        Some("prompt-a".to_string())
    );
    assert!(transfer.begin_external_sync_window_open("prompt-a"));
    assert!(!transfer.begin_external_sync_window_open("prompt-a"));
    assert!(transfer.active_external_sync_prompt("session-a").is_none());
    assert_eq!(
        transfer
            .active_external_sync_prompt("session-b")
            .map(|(prompt_id, _)| prompt_id),
        Some("prompt-b".to_string())
    );
    assert!(!transfer.begin_external_sync_window_open("missing"));

    assert!(transfer.clear_external_sync_window_tracking("prompt-a"));
    assert!(!transfer.external_sync_window_open_is_pending("prompt-a"));
    assert_eq!(
        transfer
            .active_external_sync_prompt("session-a")
            .map(|(prompt_id, _)| prompt_id),
        Some("prompt-a".to_string())
    );

    assert!(transfer.begin_external_sync_window_open("prompt-b"));
    assert!(transfer.dismiss_external_sync_prompt("prompt-b"));
    assert!(transfer.external_sync_prompt("prompt-b").is_none());
    assert!(!transfer.external_sync_window_open_is_pending("prompt-b"));
    assert!(!transfer.dismiss_external_sync_prompt("prompt-b"));
}

#[test]
fn external_sync_upload_resolution_cleans_tracking_and_records_policy() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);
    transfer.insert_external_sync_prompt(
        "prompt-a".to_string(),
        external_sync_prompt(Some("session-a"), "job-a"),
    );
    assert!(transfer.begin_external_sync_window_open("prompt-a"));

    let prompt = transfer
        .take_external_sync_prompt_for_upload(
            "prompt-a",
            Some("/remote/job-a.txt\n/local/job-a.txt".to_string()),
        )
        .expect("known prompt should resolve for upload");

    assert_eq!(prompt.job_id, "job-a");
    assert!(transfer.external_sync_prompt("prompt-a").is_none());
    assert!(!transfer.external_sync_window_open_is_pending("prompt-a"));
    assert!(transfer.external_sync_always_uploads("/remote/job-a.txt\n/local/job-a.txt"));
    assert!(
        transfer
            .take_external_sync_prompt_for_upload("prompt-a", None)
            .is_none()
    );
}

#[test]
fn external_sync_session_cleanup_preserves_other_sessions_and_policy() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);
    transfer.insert_external_sync_prompt(
        "prompt-a-1".to_string(),
        external_sync_prompt(Some("session-a"), "job-a-1"),
    );
    transfer.insert_external_sync_prompt(
        "prompt-a-2".to_string(),
        external_sync_prompt(Some("session-a"), "job-a-2"),
    );
    transfer.insert_external_sync_prompt(
        "prompt-b".to_string(),
        external_sync_prompt(Some("session-b"), "job-b"),
    );
    transfer.insert_external_sync_prompt(
        "policy-source".to_string(),
        external_sync_prompt(None, "policy-source"),
    );
    transfer.take_external_sync_prompt_for_upload(
        "policy-source",
        Some("persistent-watch-key".to_string()),
    );
    assert!(transfer.begin_external_sync_window_open("prompt-a-1"));
    assert!(transfer.begin_external_sync_window_open("prompt-b"));

    assert_eq!(transfer.clear_external_sync_for_session("session-a"), 2);
    assert!(transfer.external_sync_prompt("prompt-a-1").is_none());
    assert!(transfer.external_sync_prompt("prompt-a-2").is_none());
    assert!(!transfer.external_sync_window_open_is_pending("prompt-a-1"));
    assert!(transfer.external_sync_prompt("prompt-b").is_some());
    assert!(transfer.external_sync_window_open_is_pending("prompt-b"));
    assert!(transfer.external_sync_always_uploads("persistent-watch-key"));
}

#[test]
fn transfer_editor_owns_tab_activation_and_close_confirmation() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);
    let tab_a = editor_tab("session-a", "/srv/a.txt");
    let tab_a_id = tab_a.id.clone();
    let tab_b = editor_tab("session-b", "/srv/b.txt");
    let tab_b_id = tab_b.id.clone();

    assert!(!transfer.open_editor_tab(tab_a));
    assert!(!transfer.open_editor_tab(tab_b));
    assert_eq!(
        transfer.active_editor_tab().map(|tab| tab.id.as_str()),
        Some(tab_b_id.as_str())
    );
    assert!(transfer.activate_editor_tab(&tab_a_id));
    transfer.active_editor_tab_mut().unwrap().dirty = true;

    assert_eq!(
        transfer.request_editor_tab_close(&tab_a_id),
        TransferEditorCloseOutcome::ConfirmationRequired
    );
    let workspace = transfer.editor_workspace().unwrap();
    assert!(workspace.close_confirm);
    assert_eq!(
        workspace.pending_close_tab_id.as_deref(),
        Some(tab_a_id.as_str())
    );
    assert!(transfer.cancel_editor_close());
    assert!(!transfer.editor_close_confirmation_is_open());

    transfer.active_editor_tab_mut().unwrap().dirty = false;
    assert_eq!(
        transfer.request_editor_tab_close(&tab_a_id),
        TransferEditorCloseOutcome::Closed
    );
    assert_eq!(
        transfer.active_editor_tab().map(|tab| tab.id.as_str()),
        Some(tab_b_id.as_str())
    );
}

#[test]
fn transfer_editor_save_completion_closes_requested_tab_atomically() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);
    let tab = editor_tab("session-a", "/srv/a.txt");
    let tab_id = tab.id.clone();
    transfer.open_editor_tab(tab);
    assert!(transfer.sync_editor_content(&tab_id, "updated".to_string()));
    assert_eq!(
        transfer.request_editor_tab_close(&tab_id),
        TransferEditorCloseOutcome::ConfirmationRequired
    );
    assert_eq!(
        transfer.prepare_editor_close_after_save(),
        TransferEditorCloseAfterSave::Ready(tab_id.clone())
    );
    assert!(transfer.begin_editor_tab_save(&tab_id));

    assert_eq!(
        transfer.complete_editor_save(
            Some("session-a"),
            "/srv/a.txt",
            SftpWriteTextResult::Saved {
                modified_at: 2,
                size: 7,
            },
        ),
        Some(TransferEditorSaveOutcome::SavedAndClosed)
    );
    assert!(!transfer.editor_has_workspace());
}

#[test]
fn transfer_editor_save_all_waits_for_every_dirty_tab() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);
    let tab_a = editor_tab("session-a", "/srv/a.txt");
    let tab_a_id = tab_a.id.clone();
    let tab_b = editor_tab("session-b", "/srv/b.txt");
    let tab_b_id = tab_b.id.clone();
    transfer.open_editor_tab(tab_a);
    transfer.open_editor_tab(tab_b);
    assert!(transfer.sync_editor_content(&tab_a_id, "updated a".to_string()));
    assert!(transfer.sync_editor_content(&tab_b_id, "updated b".to_string()));
    assert_eq!(
        transfer.request_editor_close(),
        TransferEditorCloseOutcome::ConfirmationRequired
    );
    assert_eq!(
        transfer.prepare_editor_close_after_save(),
        TransferEditorCloseAfterSave::All
    );
    assert!(transfer.begin_editor_tab_save(&tab_a_id));
    assert!(transfer.begin_editor_tab_save(&tab_b_id));

    assert_eq!(
        transfer.complete_editor_save(
            Some("session-a"),
            "/srv/a.txt",
            SftpWriteTextResult::Saved {
                modified_at: 2,
                size: 9,
            },
        ),
        Some(TransferEditorSaveOutcome::Saved)
    );
    assert!(transfer.editor_has_workspace());
    assert_eq!(
        transfer.complete_editor_save(
            Some("session-b"),
            "/srv/b.txt",
            SftpWriteTextResult::Saved {
                modified_at: 3,
                size: 9,
            },
        ),
        Some(TransferEditorSaveOutcome::SavedAndClosed)
    );
    assert!(!transfer.editor_has_workspace());
}

#[test]
fn transfer_editor_conflict_and_session_cleanup_preserve_other_tabs() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);
    let tab_a = editor_tab("session-a", "/srv/a.txt");
    let tab_a_id = tab_a.id.clone();
    let tab_b = editor_tab("session-b", "/srv/b.txt");
    let tab_b_id = tab_b.id.clone();
    transfer.open_editor_tab(tab_a);
    transfer.open_editor_tab(tab_b);
    assert!(transfer.activate_editor_tab(&tab_a_id));
    assert!(transfer.begin_editor_tab_save(&tab_a_id));
    assert_eq!(
        transfer.complete_editor_save(
            Some("session-a"),
            "/srv/a.txt",
            SftpWriteTextResult::Conflict {
                modified_at: 3,
                size: 9,
            },
        ),
        Some(TransferEditorSaveOutcome::Conflict)
    );
    assert!(transfer.editor_close_confirmation_is_open());

    assert_eq!(transfer.remove_editor_tabs_for_session("session-a"), 1);
    assert_eq!(
        transfer.active_editor_tab().map(|tab| tab.id.as_str()),
        Some(tab_b_id.as_str())
    );
    assert!(!transfer.editor_close_confirmation_is_open());
    assert!(transfer.begin_editor_window_open());
    assert!(!transfer.begin_editor_window_open());
    assert!(transfer.clear_editor_window_tracking());
    assert_eq!(transfer.remove_editor_tabs_for_session("session-b"), 1);
    assert!(!transfer.editor_has_workspace());
}

#[test]
fn transfer_properties_ignore_stale_results_and_close_for_the_owner_session() {
    let cx = TestAppContext::single();
    let mut transfer = transfer_state(&cx);
    transfer.open_properties_dialog(TransferPropertiesState {
        session_id: Some("session-a".to_string()),
        entry: file_entry("/srv/file.txt"),
        properties: None,
        mode_value: "0640".to_string(),
        owner_value: String::new(),
        group_value: String::new(),
        recursive: false,
        saving: false,
        error: None,
    });

    assert!(!transfer.complete_properties_load(
        Some("session-b"),
        "/srv/file.txt",
        file_properties("/srv/file.txt"),
        "0600".to_string(),
        "updated-owner".to_string(),
        "updated-group".to_string(),
    ));
    assert!(
        transfer
            .properties_dialog()
            .is_some_and(|state| state.properties.is_none())
    );

    assert!(transfer.complete_properties_load(
        Some("session-a"),
        "/srv/file.txt",
        file_properties("/srv/file.txt"),
        "0600".to_string(),
        "updated-owner".to_string(),
        "updated-group".to_string(),
    ));
    assert_eq!(
        transfer
            .properties_dialog()
            .map(|state| state.owner_value.as_str()),
        Some("updated-owner")
    );
    assert!(transfer.begin_properties_save());
    assert!(!transfer.fail_properties_operation(
        Some("session-b"),
        "/srv/file.txt",
        "stale".to_string(),
    ));
    assert!(
        transfer
            .properties_dialog()
            .is_some_and(|state| state.saving && state.error.is_none())
    );
    assert!(transfer.fail_properties_operation(
        Some("session-a"),
        "/srv/file.txt",
        "denied".to_string(),
    ));
    assert!(
        transfer
            .properties_dialog()
            .is_some_and(|state| { !state.saving && state.error.as_deref() == Some("denied") })
    );
    assert!(!transfer.close_properties_dialog_for_session("session-b"));
    assert!(transfer.close_properties_dialog_for_session("session-a"));
    assert!(transfer.properties_dialog().is_none());

    transfer.open_properties_dialog(TransferPropertiesState {
        session_id: Some("session-a".to_string()),
        entry: file_entry("/srv/file.txt"),
        properties: Some(file_properties("/srv/file.txt")),
        mode_value: "0600".to_string(),
        owner_value: "updated-owner".to_string(),
        group_value: "updated-group".to_string(),
        recursive: false,
        saving: true,
        error: None,
    });
    assert!(!transfer.complete_properties_update(
        Some("session-a"),
        "/srv/other.txt",
        file_properties("/srv/other.txt"),
    ));
    assert!(transfer.complete_properties_update(
        Some("session-a"),
        "/srv/file.txt",
        file_properties("/srv/file.txt"),
    ));
    assert!(transfer.properties_dialog().is_none());
}

#[test]
fn transfer_queue_owns_admission_events_and_job_removal() {
    let cx = TestAppContext::single();
    let mut queue = transfer_queue(&cx);
    queue.enqueue(transfer_job(
        "job-1",
        "session-a",
        TransferJobStatus::Completed,
        false,
    ));

    assert_eq!(queue.next_job_id("download"), "download-2");
    assert!(queue.select_job("job-1"));
    assert!(queue.open_job_menu("job-1", px(12.), px(24.)));
    assert_eq!(queue.selected_job_id(), Some("job-1"));
    assert_eq!(
        queue.job_menu().map(|menu| menu.job_id.as_str()),
        Some("job-1")
    );
    assert!(queue.can_delete_job("job-1", Some("session-a")));

    assert!(queue.remove_job("job-1"));
    assert!(queue.jobs().is_empty());
    assert_eq!(queue.selected_job_id(), None);
    assert_eq!(queue.next_job_id("download"), "download-3");

    let mut rx = queue
        .take_event_receiver()
        .expect("the queue holds its receiver until the drain starts");
    let sender = queue.event_sender();
    sender
        .unbounded_send(TransferJobResult {
            id: "missing-job".to_string(),
            event: TransferJobEvent::Started {
                detail: "started".to_string(),
            },
        })
        .expect("queue receiver should remain connected");
    let event = rx.try_recv().expect("queue should receive its typed event");
    assert_eq!(event.id, "missing-job");
    assert!(matches!(event.event, TransferJobEvent::Started { .. }));
}

#[test]
fn transfer_queue_batches_are_scoped_to_the_visible_session() {
    let cx = TestAppContext::single();
    let mut queue = transfer_queue(&cx);
    queue.enqueue(transfer_job(
        "running-a",
        "session-a",
        TransferJobStatus::Running,
        true,
    ));
    queue.enqueue(transfer_job(
        "running-b",
        "session-b",
        TransferJobStatus::Running,
        true,
    ));
    queue.enqueue(transfer_job(
        "completed-a",
        "session-a",
        TransferJobStatus::Completed,
        false,
    ));
    assert!(queue.open_job_menu("completed-a", px(8.), px(8.)));

    assert_eq!(queue.pause_visible_jobs(Some("session-a")), 1);
    assert_eq!(
        queue.job("running-a").map(|job| job.status),
        Some(TransferJobStatus::Paused)
    );
    assert_eq!(
        queue.job("running-b").map(|job| job.status),
        Some(TransferJobStatus::Running)
    );
    assert_eq!(queue.resume_visible_jobs(Some("session-a")), 1);
    assert_eq!(queue.cancel_visible_jobs(Some("session-a")), 1);
    assert_eq!(
        queue.job("running-a").map(|job| job.status),
        Some(TransferJobStatus::Cancelling)
    );
    assert_eq!(queue.clear_completed_jobs(Some("session-a")), 1);
    assert!(queue.job("completed-a").is_none());
    assert_eq!(queue.selected_job_id(), None);
    assert!(queue.job_menu().is_none());
    assert!(queue.job("running-b").is_some());
    assert_eq!(queue.clear_stopped_jobs(Some("session-b")), 0);
}
