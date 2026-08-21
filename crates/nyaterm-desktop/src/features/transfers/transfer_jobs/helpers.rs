use futures::channel::mpsc::UnboundedSender;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use nyaterm_transport::SftpTransferProgress;

use crate::models::{TransferJobEvent, TransferJobKind, TransferJobResult, TransferJobState};

const TRANSFER_PROGRESS_EVENT_INTERVAL: Duration = Duration::from_millis(50);

pub(super) struct TransferProgressEventSender {
    id: String,
    tx: UnboundedSender<TransferJobResult>,
    last_sent_at: Option<Instant>,
}

impl TransferProgressEventSender {
    pub(super) fn new(id: String, tx: UnboundedSender<TransferJobResult>) -> Self {
        Self {
            id,
            tx,
            last_sent_at: None,
        }
    }

    pub(super) fn send(&mut self, progress: SftpTransferProgress) {
        let now = Instant::now();
        let completed = progress
            .total_bytes
            .is_some_and(|total| progress.bytes_transferred >= total);
        let due = self.last_sent_at.is_none_or(|last_sent_at| {
            now.duration_since(last_sent_at) >= TRANSFER_PROGRESS_EVENT_INTERVAL
        });
        if !completed && !due {
            return;
        }

        self.last_sent_at = Some(now);
        let _ = self.tx.unbounded_send(TransferJobResult {
            id: self.id.clone(),
            event: TransferJobEvent::Progress(progress),
        });
    }
}

pub(super) fn transfer_job_remote_parent_path(path: &str) -> String {
    let path = path.trim_end_matches('/');
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(index) => path[..index].to_string(),
        None => ".".to_string(),
    }
}

pub(super) fn transfer_job_local_target_path(job: &TransferJobState) -> Option<PathBuf> {
    job.summary
        .as_ref()
        .map(|summary| summary.local_path.clone())
        .or_else(|| {
            job.progress
                .as_ref()
                .map(|progress| progress.local_path.clone())
        })
        .or_else(|| match &job.kind {
            TransferJobKind::Download { local_path, .. }
            | TransferJobKind::OpenExternal { local_path, .. } => Some(local_path.clone()),
            _ => None,
        })
}

pub(super) fn transfer_job_reveal_dir(path: PathBuf) -> PathBuf {
    if path.is_dir() {
        return path;
    }
    path.parent()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.clone())
}
