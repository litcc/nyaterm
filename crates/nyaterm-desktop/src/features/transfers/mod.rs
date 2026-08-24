//! Transfer jobs, transfer options, path prompts and transfer widgets.

mod cwd_sync_clock;
mod editor_window;
mod external_sync_window;
mod remote_text_editor;
mod state;
mod transfer_events;
mod transfer_jobs;
mod transfer_options;
mod transfer_paths;
mod transfer_widgets;

use std::sync::Arc;

use nyaterm_store::{StoreBlockingClient, StoreDomain};
use nyaterm_transport::{
    RemoteFileBackendKind, RemoteFileBackendPreference, RemoteFileBackendPreferenceStore,
    RemoteFileService, SshMultiplexHandle, SshProcessService, SshSessionConfig,
};

use crate::features::NyaTermApp;

#[derive(Clone)]
pub(in crate::features) struct SftpJobSession {
    pub session_id: Option<String>,
    pub service: RemoteFileService,
}

struct ConnectionStoreBackendPreferences {
    store: StoreBlockingClient,
}

impl ConnectionStoreBackendPreferences {
    fn new(store: StoreBlockingClient) -> Self {
        Self { store }
    }
}

impl RemoteFileBackendPreferenceStore for ConnectionStoreBackendPreferences {
    fn load_backend(
        &self,
        endpoint_key: &str,
    ) -> anyhow::Result<Option<RemoteFileBackendPreference>> {
        let cache = self.store.request_fn(StoreDomain::Transfers, |store| {
            store.load_remote_file_backend_cache()
        })?;
        Ok(cache.entries.get(endpoint_key).and_then(|entry| {
            RemoteFileBackendKind::from_cache_name(&entry.last_working_backend).map(|backend| {
                RemoteFileBackendPreference {
                    backend,
                    sftp_unavailable: entry.sftp_unavailable,
                    last_failure_reason: entry.last_failure_reason.clone(),
                }
            })
        }))
    }

    fn save_backend(
        &self,
        endpoint_key: &str,
        preference: &RemoteFileBackendPreference,
    ) -> anyhow::Result<()> {
        let endpoint_key = endpoint_key.to_string();
        let backend = preference.backend.cache_name().to_string();
        let sftp_unavailable = preference.sftp_unavailable;
        let last_failure_reason = preference.last_failure_reason.clone();
        self.store
            .request_fn(StoreDomain::Transfers, move |store| {
                store.update_remote_file_backend_cache_entry(
                    &endpoint_key,
                    &backend,
                    sftp_unavailable,
                    last_failure_reason,
                )
            })?;
        Ok(())
    }
}

impl NyaTermApp {
    pub(in crate::features) fn remote_file_service_for_session(
        &mut self,
        session_id: &str,
        config: SshSessionConfig,
    ) -> anyhow::Result<RemoteFileService> {
        let preferences: Arc<dyn RemoteFileBackendPreferenceStore> = Arc::new(
            ConnectionStoreBackendPreferences::new(self.store_blocking_client()),
        );
        self.session
            .remote_file_service_for_session(session_id, config, preferences)
    }

    pub(in crate::features) fn active_remote_file_service(
        &mut self,
    ) -> anyhow::Result<RemoteFileService> {
        let session_id = self
            .session
            .active_id_owned()
            .ok_or_else(|| anyhow::anyhow!("start an SSH session first"))?;
        let config = self
            .session
            .active_ssh_config_owned()
            .ok_or_else(|| anyhow::anyhow!("active session is not SSH"))?;
        self.remote_file_service_for_session(&session_id, config)
    }

    pub(in crate::features) fn transfer_remote_file_service(
        &mut self,
        session_id: Option<&str>,
        config: SshSessionConfig,
    ) -> anyhow::Result<RemoteFileService> {
        let session_id = session_id
            .or_else(|| self.session.active_id())
            .ok_or_else(|| anyhow::anyhow!("source SSH session is unavailable"))?
            .to_string();
        self.remote_file_service_for_session(&session_id, config)
    }
}

pub(in crate::features) fn session_ssh_process_service(
    config: SshSessionConfig,
    multiplex: Option<SshMultiplexHandle>,
) -> anyhow::Result<SshProcessService> {
    match multiplex {
        Some(multiplex) => SshProcessService::with_multiplex(config, multiplex),
        None => Ok(SshProcessService::new(config)),
    }
}

pub(in crate::features) use cwd_sync_clock::TRANSFER_CWD_SYNC_POLL_INTERVAL;
pub(in crate::features) use remote_text_editor::RemoteTextEditor;
pub(in crate::features) use state::natural_compare_ascii;
#[cfg(test)]
pub(in crate::features) use state::transfer_browser_entry_is_visible;
pub(in crate::features) use state::{
    TransferEditorCloseAfterSave, TransferEditorCloseOutcome, TransferEditorDiscardOutcome,
    TransferFeatureFocus, TransferFeatureState,
};
pub(in crate::features) use transfer_widgets::{
    duplicate_decision_label, duplicate_policy_label, format_file_size,
};
