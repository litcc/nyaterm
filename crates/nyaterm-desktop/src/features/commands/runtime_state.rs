//! Background runtime ownership shared by command history and quick commands.

use std::sync::mpsc;

use futures::channel::mpsc::UnboundedReceiver;
use nyaterm_store::StoreBlockingClient;

use crate::features::{
    runtime_jobs::CommandPersistenceRequest, runtime_jobs::CommandPersistenceResult,
    runtime_jobs::spawn_command_persistence_worker,
};

pub(in crate::features) struct CommandRuntimeState {
    tx: mpsc::Sender<CommandPersistenceRequest>,
    /// Taken once by `NyaTermApp::start_command_persistence_event_drain`,
    /// which owns delivery from then on. `None` afterwards, so a second start
    /// is a no-op.
    rx: Option<UnboundedReceiver<CommandPersistenceResult>>,
    pending: usize,
}

impl CommandRuntimeState {
    pub(in crate::features) fn new(store: StoreBlockingClient) -> Self {
        let (tx, rx) = spawn_command_persistence_worker(store);
        Self {
            tx,
            rx: Some(rx),
            pending: 0,
        }
    }

    pub(in crate::features) fn queue(&mut self, request: CommandPersistenceRequest) -> bool {
        if self.tx.send(request).is_err() {
            return false;
        }
        self.pending = self.pending.saturating_add(1);
        true
    }

    pub(in crate::features) fn take_event_receiver(
        &mut self,
    ) -> Option<UnboundedReceiver<CommandPersistenceResult>> {
        self.rx.take()
    }

    /// Account for one delivered result.
    pub(in crate::features) fn note_event_delivered(&mut self) {
        self.pending = self.pending.saturating_sub(1);
    }

    /// Account for the worker thread dropping its sender, reporting whether any
    /// request was still outstanding and so lost.
    pub(in crate::features) fn note_worker_disconnected(&mut self) -> bool {
        std::mem::take(&mut self.pending) > 0
    }

    /// The pending counter still gates `note_worker_disconnected`; this read
    /// accessor is only needed to assert the lifecycle.
    #[cfg(test)]
    pub(in crate::features) fn is_idle(&self) -> bool {
        self.pending == 0
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use futures::channel::mpsc::unbounded;

    use super::CommandRuntimeState;
    use crate::features::{
        runtime_jobs::CommandPersistenceRequest, runtime_jobs::CommandPersistenceResult,
    };

    #[test]
    fn command_runtime_owns_pending_request_lifecycle() {
        let (request_tx, request_rx) = mpsc::channel();
        let (result_tx, result_rx) = unbounded();
        let mut runtime = CommandRuntimeState {
            tx: request_tx,
            rx: Some(result_rx),
            pending: 0,
        };

        assert!(runtime.is_idle());
        assert!(runtime.queue(CommandPersistenceRequest::AppendHistory(vec![
            "pwd".to_string(),
        ])));
        assert!(!runtime.is_idle());
        assert!(matches!(
            request_rx.recv().expect("request should reach worker"),
            CommandPersistenceRequest::AppendHistory(commands) if commands == ["pwd"]
        ));

        let mut result_rx = runtime
            .take_event_receiver()
            .expect("the runtime holds its receiver until the drain starts");
        result_tx
            .unbounded_send(CommandPersistenceResult::History(Ok(Vec::new())))
            .expect("result channel should stay connected");
        assert!(matches!(
            result_rx
                .try_recv()
                .expect("the result should be queued"),
            CommandPersistenceResult::History(Ok(history)) if history.is_empty()
        ));
        runtime.note_event_delivered();
        assert!(runtime.is_idle());

        assert!(runtime.queue(CommandPersistenceRequest::AppendHistory(Vec::new())));
        assert!(
            runtime.note_worker_disconnected(),
            "a request outstanding when the worker drops its sender is lost and must be reported"
        );
        assert!(runtime.is_idle());
    }
}
