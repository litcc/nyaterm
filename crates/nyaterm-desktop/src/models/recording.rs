use std::sync::{Arc, mpsc};
use std::thread;

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use nyaterm_transport::{
    RecordingManager, TerminalHistorySearchRequest, TerminalHistorySearchResponse,
};

pub(crate) struct RecordingWritePipeline {
    command_tx: mpsc::Sender<RecordingWriteCommand>,
    /// Taken once by `NyaTermApp::start_recording_event_drain`, which owns
    /// delivery from then on. `None` afterwards, so a second start is a no-op.
    event_rx: Option<UnboundedReceiver<RecordingWriteEvent>>,
}

#[derive(Clone)]
pub(crate) struct RecordingWriteHandle {
    command_tx: mpsc::Sender<RecordingWriteCommand>,
}

impl RecordingWritePipeline {
    pub(crate) fn spawn(recording_manager: Arc<RecordingManager>) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        // Unbounded rather than the bounded channel this used to be: the cap only
        // mattered while the UI drained lazily on a tick. History-search replies
        // are one per outstanding request and are now applied as posted, so the
        // writer thread can never park on a full queue.
        let (event_tx, event_rx) = unbounded();
        thread::Builder::new()
            .name("nyaterm-recording-writer".to_string())
            .spawn(move || run_recording_writer(recording_manager, command_rx, event_tx))
            .expect("failed to spawn recording writer");
        Self {
            command_tx,
            event_rx: Some(event_rx),
        }
    }

    pub(crate) fn write_output(&self, session_id: impl Into<String>, text: impl Into<String>) {
        self.writer().write_output(session_id, text);
    }

    pub(crate) fn write_input(&self, session_id: impl Into<String>, data: impl Into<Vec<u8>>) {
        let data = data.into();
        if data.is_empty() {
            return;
        }
        let _ = self.command_tx.send(RecordingWriteCommand::WriteInput {
            session_id: session_id.into(),
            data,
        });
    }

    pub(crate) fn write_raw_input(&self, session_id: impl Into<String>, data: impl Into<Vec<u8>>) {
        let data = data.into();
        if data.is_empty() {
            return;
        }
        let _ = self.command_tx.send(RecordingWriteCommand::WriteRawInput {
            session_id: session_id.into(),
            data,
        });
    }

    pub(crate) fn cleanup_session(&self, session_id: impl Into<String>) {
        let _ = self.command_tx.send(RecordingWriteCommand::CleanupSession {
            session_id: session_id.into(),
        });
    }

    pub(crate) fn request_history_search(&self, key: RecordingHistorySearchKey) {
        if key.query.trim().is_empty() {
            return;
        }
        let _ = self
            .command_tx
            .send(RecordingWriteCommand::HistorySearch { key });
    }

    pub(crate) fn take_event_receiver(&mut self) -> Option<UnboundedReceiver<RecordingWriteEvent>> {
        self.event_rx.take()
    }

    pub(crate) fn writer(&self) -> RecordingWriteHandle {
        RecordingWriteHandle {
            command_tx: self.command_tx.clone(),
        }
    }
}

impl RecordingWriteHandle {
    pub(crate) fn write_output(&self, session_id: impl Into<String>, text: impl Into<String>) {
        let text = text.into();
        if text.is_empty() {
            return;
        }
        let _ = self.command_tx.send(RecordingWriteCommand::WriteOutput {
            session_id: session_id.into(),
            text,
        });
    }

    pub(crate) fn flush(&self) {
        let (ack_tx, ack_rx) = mpsc::sync_channel(0);
        if self
            .command_tx
            .send(RecordingWriteCommand::Flush { ack_tx })
            .is_ok()
        {
            let _ = ack_rx.recv();
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum RecordingWriteEvent {
    HistorySearch(RecordingHistorySearchEvent),
}

#[derive(Clone, Debug)]
pub(crate) struct RecordingHistorySearchEvent {
    pub(crate) key: RecordingHistorySearchKey,
    pub(crate) result: Result<TerminalHistorySearchResponse, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecordingHistorySearchKey {
    pub(crate) session_id: String,
    pub(crate) query: String,
    pub(crate) case_sensitive: bool,
    pub(crate) regex: bool,
    pub(crate) whole_word: bool,
    pub(crate) limit: Option<usize>,
    pub(crate) context_before: Option<usize>,
    pub(crate) context_after: Option<usize>,
    pub(crate) max_lines: Option<usize>,
}

impl RecordingHistorySearchKey {
    pub(crate) fn request(&self) -> TerminalHistorySearchRequest {
        TerminalHistorySearchRequest {
            session_id: self.session_id.clone(),
            query: self.query.clone(),
            case_sensitive: self.case_sensitive,
            regex: self.regex,
            whole_word: self.whole_word,
            limit: self.limit,
            context_before: self.context_before,
            context_after: self.context_after,
            max_lines: self.max_lines,
        }
    }
}

#[derive(Debug)]
enum RecordingWriteCommand {
    WriteOutput { session_id: String, text: String },
    WriteInput { session_id: String, data: Vec<u8> },
    WriteRawInput { session_id: String, data: Vec<u8> },
    CleanupSession { session_id: String },
    HistorySearch { key: RecordingHistorySearchKey },
    Flush { ack_tx: mpsc::SyncSender<()> },
}

fn run_recording_writer(
    recording_manager: Arc<RecordingManager>,
    command_rx: mpsc::Receiver<RecordingWriteCommand>,
    event_tx: UnboundedSender<RecordingWriteEvent>,
) {
    while let Ok(command) = command_rx.recv() {
        match command {
            RecordingWriteCommand::WriteOutput { session_id, text } => {
                recording_manager.write_output(&session_id, &text);
            }
            RecordingWriteCommand::WriteInput { session_id, data } => {
                recording_manager.write_input(&session_id, &data);
            }
            RecordingWriteCommand::WriteRawInput { session_id, data } => {
                recording_manager.write_raw_input(&session_id, &data);
            }
            RecordingWriteCommand::CleanupSession { session_id } => {
                recording_manager.cleanup_session(&session_id);
            }
            RecordingWriteCommand::HistorySearch { key } => {
                let result = recording_manager
                    .search_history(key.request())
                    .map_err(|error| error.to_string());
                let _ = event_tx.unbounded_send(RecordingWriteEvent::HistorySearch(
                    RecordingHistorySearchEvent { key, result },
                ));
            }
            RecordingWriteCommand::Flush { ack_tx } => {
                let _ = ack_tx.send(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use nyaterm_transport::RecordingManager;

    use super::{RecordingHistorySearchKey, RecordingWriteEvent, RecordingWritePipeline};

    #[test]
    fn recording_pipeline_preserves_write_order_before_flush() {
        let manager = Arc::new(RecordingManager::new());
        let pipeline = RecordingWritePipeline::spawn(Arc::clone(&manager));
        let session_id = "session-a";
        pipeline.write_input(session_id, b"echo hello\r".to_vec());
        pipeline.write_output(session_id, "hello\n");
        pipeline.writer().flush();

        let results = manager
            .search_history(nyaterm_transport::TerminalHistorySearchRequest {
                session_id: session_id.to_string(),
                query: "hello".to_string(),
                case_sensitive: false,
                regex: false,
                whole_word: false,
                limit: Some(10),
                context_before: Some(0),
                context_after: Some(0),
                max_lines: None,
            })
            .expect("search should succeed");
        assert_eq!(results.total, 2);
        assert_eq!(results.results[0].source, "input");
        assert_eq!(results.results[1].source, "output");
    }

    #[test]
    fn recording_pipeline_cleanup_runs_after_queued_writes() {
        let manager = Arc::new(RecordingManager::new());
        let pipeline = RecordingWritePipeline::spawn(Arc::clone(&manager));
        let session_id = "session-b";
        let path = unique_recording_path("pipeline-cleanup");
        manager
            .start(session_id, &path, true, false)
            .expect("recording should start");

        pipeline.write_output(session_id, "before cleanup\n");
        pipeline.cleanup_session(session_id);
        pipeline.writer().flush();

        let text = fs::read_to_string(&path).expect("recording file should exist");
        assert!(text.contains("before cleanup"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn recording_pipeline_search_runs_after_queued_writes() {
        let manager = Arc::new(RecordingManager::new());
        let mut pipeline = RecordingWritePipeline::spawn(Arc::clone(&manager));
        let session_id = "session-c";
        let key = RecordingHistorySearchKey {
            session_id: session_id.to_string(),
            query: "queued".to_string(),
            case_sensitive: false,
            regex: false,
            whole_word: false,
            limit: Some(8),
            context_before: Some(0),
            context_after: Some(0),
            max_lines: Some(30_000),
        };

        pipeline.write_output(session_id, "queued history line\n");
        pipeline.request_history_search(key.clone());
        pipeline.writer().flush();

        let event = pipeline
            .take_event_receiver()
            .expect("the pipeline holds its receiver until the drain starts")
            .try_recv()
            .expect("search event should be queued");
        let RecordingWriteEvent::HistorySearch(event) = event;
        assert_eq!(event.key, key);
        assert_eq!(event.result.expect("search should succeed").total, 1);
    }

    fn unique_recording_path(name: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("nyaterm-{name}-{nanos}.log"))
            .to_string_lossy()
            .to_string()
    }
}
