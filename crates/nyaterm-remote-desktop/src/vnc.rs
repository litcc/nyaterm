use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::{Child, ChildStdin};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use uuid::Uuid;

use crate::helper_process;
use crate::{
    MAX_VNC_CLIPBOARD_TEXT_BYTES, MAX_VNC_INPUT_BATCH, PROTOCOL_VERSION, PacketType, QueueWaker,
    RdpFrameEvent, VncControlMessage, VncError, VncErrorKind, VncInputEvent, VncRuntimeEvent,
    VncSecurityMode, VncServerCapabilities, VncSessionConfig, VncSessionDrain, VncSessionState,
    decode_frame_packet, decode_vnc_control, encode_vnc_control, read_packet,
    validate_committed_text, write_packet,
};

const FRAME_QUEUE_LIMIT: usize = 64;
const HELPER_PACKAGE: &str = "nyaterm-vnc-helper";
const HELPER_ENV_VAR: &str = "NYATERM_VNC_HELPER";

fn resolve_helper_path() -> Result<PathBuf, VncError> {
    helper_process::resolve_helper(HELPER_PACKAGE, HELPER_ENV_VAR)
        .map_err(|message| VncError::new(VncErrorKind::HelperMissing, message))
}

#[derive(Default)]
struct EventQueue {
    /// Signalled after anything is enqueued. Held here rather than at each
    /// call site so every producer path wakes the consumer.
    waker: Option<QueueWaker>,
    control: VecDeque<VncRuntimeEvent>,
    frames: VecDeque<RdpFrameEvent>,
    current_epoch: Option<u64>,
    waiting_for_full_frame: bool,
    dropped_frames: usize,
}

impl EventQueue {
    fn push_control(&mut self, event: VncRuntimeEvent) {
        self.control.push_back(event);
        self.wake();
    }

    fn wake(&self) {
        if let Some(waker) = &self.waker {
            waker();
        }
    }

    fn push_reset(&mut self, session_id: &str, epoch: u64, width: u32, height: u32) {
        self.current_epoch = Some(epoch);
        self.frames.clear();
        self.waiting_for_full_frame = true;
        self.control.push_back(VncRuntimeEvent::Frame {
            session_id: session_id.to_string(),
            event: RdpFrameEvent::Reset {
                epoch,
                width,
                height,
            },
        });
        self.wake();
    }

    /// Queue a framebuffer update, returning `true` when the queue overflowed and
    /// the helper must be asked for a full refresh.
    ///
    /// Frames from a superseded epoch are dropped here rather than decoded and
    /// rejected later by `Framebuffer::apply`. Unlike the RDP queue this one does
    /// not withhold partial frames while `waiting_for_full_frame`: a VNC frame is
    /// flagged `full` merely by starting at the origin, so withholding would stall
    /// the display whenever a server only sends interior rectangles.
    fn push_frame(&mut self, frame: RdpFrameEvent) -> bool {
        let dropped = self.push_frame_inner(frame);
        // Wake unconditionally rather than mirroring the branch structure below;
        // see the RDP queue for why a redundant wake is the cheaper mistake.
        self.wake();
        dropped
    }

    fn push_frame_inner(&mut self, frame: RdpFrameEvent) -> bool {
        let RdpFrameEvent::Bitmap { epoch, full, .. } = &frame else {
            self.frames.push_back(frame);
            return false;
        };
        if self.current_epoch != Some(*epoch) {
            self.dropped_frames += 1;
            return false;
        }
        if *full {
            self.waiting_for_full_frame = false;
        }
        if self.frames.len() >= FRAME_QUEUE_LIMIT {
            self.dropped_frames += self.frames.len() + 1;
            self.frames.clear();
            self.waiting_for_full_frame = true;
            return true;
        }
        self.frames.push_back(frame);
        false
    }

    fn drain(&mut self) -> VncSessionDrain {
        VncSessionDrain {
            control: self.control.drain(..).collect(),
            frames: self.frames.drain(..).collect(),
            dropped_frames: std::mem::take(&mut self.dropped_frames),
            waiting_for_full_frame: self.waiting_for_full_frame,
        }
    }
}

struct SessionRecord {
    state: Arc<Mutex<VncSessionState>>,
    capabilities: Arc<Mutex<Option<VncServerCapabilities>>>,
    queue: Arc<Mutex<EventQueue>>,
    writer: Arc<Mutex<ChildStdin>>,
    child: Option<Child>,
    reader: Option<JoinHandle<()>>,
}

/// Owns the `nyaterm-vnc-helper` child processes and their event queues.
///
/// The public surface is unchanged from the previous in-process implementation;
/// only the transport moved behind IPC.
#[derive(Default)]
pub struct VncSessionManager {
    sessions: Mutex<HashMap<String, SessionRecord>>,
    /// Installed once by the application; copied into every session queue so
    /// the reader thread can wake the consumer instead of being polled.
    queue_waker: Mutex<Option<QueueWaker>>,
}

impl VncSessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Install the waker every session queue signals after enqueuing.
    ///
    /// Sessions created before this call keep polling semantics, so install it
    /// during startup, before any session exists.
    pub fn set_queue_waker(&self, waker: QueueWaker) {
        if let Ok(mut slot) = self.queue_waker.lock() {
            *slot = Some(waker);
        }
    }

    fn queue_waker(&self) -> Option<QueueWaker> {
        self.queue_waker.lock().ok()?.clone()
    }

    pub fn create_session(&self, config: VncSessionConfig) -> Result<String, VncError> {
        self.create_session_with_id(Uuid::new_v4().to_string(), config)
    }

    pub fn create_session_with_id(
        &self,
        session_id: String,
        config: VncSessionConfig,
    ) -> Result<String, VncError> {
        // Validate before spawning so a bad configuration never costs a process.
        validate_vnc_config(&config)?;
        {
            let mut sessions = self.sessions.lock().map_err(|_| registry_poisoned())?;
            if sessions
                .get(&session_id)
                .is_some_and(|record| record.child.is_some())
            {
                return Err(VncError::new(
                    VncErrorKind::Protocol,
                    format!("VNC session '{session_id}' is already running"),
                ));
            }
            sessions.remove(&session_id);
        }
        let helper = resolve_helper_path()?;
        let helper_process::HelperProcess {
            child,
            stdin,
            stdout,
        } = helper_process::spawn_helper(&helper).map_err(|error| {
            VncError::new(
                VncErrorKind::HelperMissing,
                format!("failed to spawn the VNC helper: {error}"),
            )
        })?;
        let writer = Arc::new(Mutex::new(stdin));
        let queue = Arc::new(Mutex::new(EventQueue {
            waker: self.queue_waker(),
            ..EventQueue::default()
        }));
        let state = Arc::new(Mutex::new(VncSessionState::Connecting));
        let capabilities = Arc::new(Mutex::new(None));
        let reader = spawn_reader(
            session_id.clone(),
            stdout,
            writer.clone(),
            queue.clone(),
            state.clone(),
            capabilities.clone(),
        );
        let mut record = SessionRecord {
            state,
            capabilities,
            queue,
            writer,
            child: Some(child),
            reader: Some(reader),
        };
        let start_result = send_control(
            &record.writer,
            &VncControlMessage::ClientHello {
                version: PROTOCOL_VERSION,
            },
        )
        .and_then(|()| {
            send_control(
                &record.writer,
                &VncControlMessage::Connect {
                    session_id: session_id.clone(),
                    config,
                },
            )
        });
        if let Err(error) = start_result {
            cleanup_child(&mut record);
            return Err(error);
        }
        record
            .queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push_control(VncRuntimeEvent::State {
                session_id: session_id.clone(),
                state: VncSessionState::Connecting,
                message: None,
            });
        let mut sessions = match self.sessions.lock() {
            Ok(sessions) => sessions,
            Err(_) => {
                cleanup_child(&mut record);
                return Err(registry_poisoned());
            }
        };
        if sessions.contains_key(&session_id) {
            drop(sessions);
            cleanup_child(&mut record);
            return Err(VncError::new(
                VncErrorKind::Protocol,
                format!("VNC session '{session_id}' was created concurrently"),
            ));
        }
        sessions.insert(session_id.clone(), record);
        Ok(session_id)
    }

    /// Return the static helper capabilities confirmed by `ServerHello`.
    ///
    /// VNC has no Secure Attention capability or operation by design.
    pub fn server_capabilities(&self, session_id: &str) -> Option<VncServerCapabilities> {
        let sessions = self.sessions.lock().ok()?;
        *sessions.get(session_id)?.capabilities.lock().ok()?
    }

    pub fn send_input(&self, session_id: &str, events: Vec<VncInputEvent>) -> Result<(), VncError> {
        if events.len() > MAX_VNC_INPUT_BATCH {
            return Err(VncError::new(
                VncErrorKind::Protocol,
                format!("VNC input batch exceeds {MAX_VNC_INPUT_BATCH} events"),
            ));
        }
        let needs_committed_text = validate_vnc_input(&events)?;
        if needs_committed_text {
            let capabilities = self.confirmed_capabilities(session_id)?;
            if !capabilities.committed_unicode_keysyms {
                return Err(VncError::new(
                    VncErrorKind::Protocol,
                    "VNC helper did not advertise committed Unicode keysym input",
                ));
            }
        }
        self.send(
            session_id,
            VncControlMessage::Input {
                session_id: session_id.to_string(),
                events,
            },
        )
    }

    pub fn set_clipboard_text(&self, session_id: &str, text: String) -> Result<(), VncError> {
        if !is_latin1_within_limit(&text) {
            return Err(VncError::new(
                VncErrorKind::Clipboard,
                "VNC clipboard text must be Latin-1 and no larger than 1 MiB",
            ));
        }
        self.send(
            session_id,
            VncControlMessage::Clipboard {
                session_id: session_id.to_string(),
                text,
            },
        )
    }

    pub fn request_full_frame(&self, session_id: &str) -> Result<(), VncError> {
        self.send(
            session_id,
            VncControlMessage::RequestFullFrame {
                session_id: session_id.to_string(),
            },
        )
    }

    pub fn drain(&self, session_id: &str) -> VncSessionDrain {
        let Ok(sessions) = self.sessions.lock() else {
            return VncSessionDrain::default();
        };
        let Some(record) = sessions.get(session_id) else {
            return VncSessionDrain::default();
        };
        record
            .queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .drain()
    }

    /// Close a session, keeping its record so [`Self::state`] still answers.
    ///
    /// This matches `RdpSessionManager::close`: the record stays in the map with
    /// `child: None` so a caller can observe `Disconnected` afterwards, and a
    /// later `create_session_with_id` with the same id replaces the stale entry.
    pub fn close(&self, session_id: &str) -> Result<(), VncError> {
        let Some(mut record) = self
            .sessions
            .lock()
            .map_err(|_| registry_poisoned())?
            .remove(session_id)
        else {
            return Ok(());
        };
        set_state(&record.state, VncSessionState::Disconnecting);
        let _ = send_control(
            &record.writer,
            &VncControlMessage::Input {
                session_id: session_id.to_string(),
                events: vec![VncInputEvent::ReleaseAllKeys],
            },
        );
        let _ = send_control(
            &record.writer,
            &VncControlMessage::Disconnect {
                session_id: session_id.to_string(),
            },
        );
        cleanup_child(&mut record);
        set_state(&record.state, VncSessionState::Disconnected);
        record
            .queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push_control(VncRuntimeEvent::State {
                session_id: session_id.to_string(),
                state: VncSessionState::Disconnected,
                message: None,
            });
        self.sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(session_id.to_string(), record);
        Ok(())
    }

    pub fn state(&self, session_id: &str) -> Option<VncSessionState> {
        let sessions = self.sessions.lock().ok()?;
        sessions
            .get(session_id)?
            .state
            .lock()
            .ok()
            .map(|state| *state)
    }

    pub fn shutdown(&self) {
        let ids = self
            .sessions
            .lock()
            .map(|sessions| {
                sessions
                    .iter()
                    .filter_map(|(id, record)| record.child.is_some().then_some(id.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for id in ids {
            let _ = self.close(&id);
        }
    }

    fn confirmed_capabilities(&self, session_id: &str) -> Result<VncServerCapabilities, VncError> {
        let sessions = self.sessions.lock().map_err(|_| registry_poisoned())?;
        let record = sessions.get(session_id).ok_or_else(|| {
            VncError::new(
                VncErrorKind::Protocol,
                format!("VNC session '{session_id}' is not running"),
            )
        })?;
        record
            .capabilities
            .lock()
            .map_err(|_| VncError::new(VncErrorKind::Internal, "VNC capability lock is poisoned"))?
            .as_ref()
            .copied()
            .ok_or_else(|| {
                VncError::new(
                    VncErrorKind::Protocol,
                    "VNC helper capabilities have not been confirmed",
                )
            })
    }

    fn send(&self, session_id: &str, message: VncControlMessage) -> Result<(), VncError> {
        let sessions = self.sessions.lock().map_err(|_| registry_poisoned())?;
        let record = sessions.get(session_id).ok_or_else(|| {
            VncError::new(
                VncErrorKind::Protocol,
                format!("VNC session '{session_id}' is not running"),
            )
        })?;
        send_control(&record.writer, &message)
    }
}

impl Drop for VncSessionManager {
    fn drop(&mut self) {
        // Without this, every helper process outlives the application.
        self.shutdown();
    }
}

fn validate_vnc_input(events: &[VncInputEvent]) -> Result<bool, VncError> {
    let mut has_committed_text = false;
    for event in events {
        if let VncInputEvent::Text { text } = event {
            validate_committed_text(text).map_err(|error| {
                VncError::new(
                    VncErrorKind::Protocol,
                    format!("invalid VNC committed text: {error}"),
                )
            })?;
            has_committed_text = true;
        }
    }
    Ok(has_committed_text)
}

pub fn validate_vnc_config(config: &VncSessionConfig) -> Result<(), VncError> {
    if config.host.trim().is_empty() {
        return Err(VncError::new(
            VncErrorKind::Protocol,
            "VNC host is required",
        ));
    }
    if matches!(config.security.mode, VncSecurityMode::VncAuth) && config.password.is_none() {
        return Err(VncError::new(
            VncErrorKind::Authentication,
            "VNC Authentication requires a password",
        ));
    }
    // Classic VNC auth keys off the first 8 bytes; str::len() is that byte count.
    if let Some(password) = config.password.as_ref()
        && password.len() > 8
    {
        return Err(VncError::new(
            VncErrorKind::Authentication,
            "Classic VNC authentication passwords must be 8 bytes or fewer",
        ));
    }
    Ok(())
}

fn registry_poisoned() -> VncError {
    VncError::new(
        VncErrorKind::Internal,
        "VNC session registry lock is poisoned",
    )
}

fn send_control(
    writer: &Arc<Mutex<ChildStdin>>,
    message: &VncControlMessage,
) -> Result<(), VncError> {
    let packet = encode_vnc_control(message)
        .map_err(|error| VncError::new(VncErrorKind::Ipc, error.to_string()))?;
    write_packet(
        &mut *writer
            .lock()
            .map_err(|_| VncError::new(VncErrorKind::Ipc, "VNC helper writer lock is poisoned"))?,
        &packet,
    )
    .map_err(|error| VncError::new(VncErrorKind::Ipc, error.to_string()))
}

fn spawn_reader(
    session_id: String,
    mut stdout: std::process::ChildStdout,
    writer: Arc<Mutex<ChildStdin>>,
    queue: Arc<Mutex<EventQueue>>,
    state: Arc<Mutex<VncSessionState>>,
    capabilities: Arc<Mutex<Option<VncServerCapabilities>>>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name(format!("nyaterm-vnc-reader-{session_id}"))
        .spawn(move || {
            let mut hello_received = false;
            loop {
                let packet = match read_packet(&mut stdout) {
                    Ok(Some(packet)) => packet,
                    Ok(None) => break,
                    Err(error) => {
                        push_reader_error(
                            &session_id,
                            &queue,
                            &state,
                            VncErrorKind::Ipc,
                            error.to_string(),
                        );
                        return;
                    }
                };
                let result: Result<(), VncError> = match packet.packet_type {
                    PacketType::Control => decode_vnc_control(&packet)
                        .map_err(|error| VncError::new(VncErrorKind::Protocol, error.to_string()))
                        .and_then(|message| {
                            handle_control(
                                &session_id,
                                message,
                                &queue,
                                &state,
                                &capabilities,
                                &mut hello_received,
                            )
                        }),
                    PacketType::Frame => require_server_hello(hello_received, "frame packet")
                        .and_then(|()| {
                            decode_frame_packet(&packet).map_err(|error| {
                                VncError::new(VncErrorKind::Protocol, error.to_string())
                            })
                        })
                        .map(|(frame_session, frame)| {
                            if frame_session != session_id {
                                return;
                            }
                            let request_full = queue
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner())
                                .push_frame(frame);
                            if request_full {
                                // Overflow discarded queued rectangles, so ask the
                                // server to repaint instead of leaving them lost.
                                let _ = send_control(
                                    &writer,
                                    &VncControlMessage::RequestFullFrame {
                                        session_id: session_id.clone(),
                                    },
                                );
                            }
                        }),
                    // The VNC path never advertises cursor encodings.
                    PacketType::Cursor => {
                        if hello_received {
                            Err(VncError::new(
                                VncErrorKind::Protocol,
                                "VNC helper sent an unsupported cursor packet",
                            ))
                        } else {
                            require_server_hello(false, "cursor packet")
                        }
                    }
                };
                if let Err(error) = result {
                    push_reader_error(&session_id, &queue, &state, error.kind, error.message);
                    return;
                }
            }
            let current = *state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !matches!(
                current,
                VncSessionState::Disconnecting
                    | VncSessionState::Disconnected
                    | VncSessionState::Failed
            ) {
                // stdout EOF with a live session means the helper died. Reporting it
                // is what turns a silent hang into a retryable failure.
                push_reader_error(
                    &session_id,
                    &queue,
                    &state,
                    VncErrorKind::HelperCrashed,
                    "the VNC helper exited unexpectedly".to_string(),
                );
            }
        })
        .expect("failed to spawn the VNC helper reader")
}

fn require_server_hello(received: bool, packet_kind: &str) -> Result<(), VncError> {
    if received {
        Ok(())
    } else {
        Err(VncError::new(
            VncErrorKind::Protocol,
            format!("VNC helper sent {packet_kind} before ServerHello"),
        ))
    }
}

fn handle_control(
    session_id: &str,
    message: VncControlMessage,
    queue: &Arc<Mutex<EventQueue>>,
    state: &Arc<Mutex<VncSessionState>>,
    capabilities: &Arc<Mutex<Option<VncServerCapabilities>>>,
    hello_received: &mut bool,
) -> Result<(), VncError> {
    if !*hello_received {
        let VncControlMessage::ServerHello {
            version,
            capabilities: confirmed,
        } = message
        else {
            return Err(VncError::new(
                VncErrorKind::Protocol,
                "VNC helper's first control message must be ServerHello",
            ));
        };
        if version != PROTOCOL_VERSION {
            return Err(VncError::new(
                VncErrorKind::Protocol,
                format!("VNC helper protocol version {version} does not match {PROTOCOL_VERSION}"),
            ));
        }
        let mut slot = capabilities.lock().map_err(|_| {
            VncError::new(VncErrorKind::Internal, "VNC capability lock is poisoned")
        })?;
        if slot.is_some() {
            return Err(VncError::new(
                VncErrorKind::Protocol,
                "VNC helper capabilities were already confirmed",
            ));
        }
        *slot = Some(confirmed);
        *hello_received = true;
        return Ok(());
    }

    match message {
        VncControlMessage::ServerHello { .. } => {
            return Err(VncError::new(
                VncErrorKind::Protocol,
                "VNC helper sent duplicate ServerHello",
            ));
        }
        VncControlMessage::ClientHello { .. }
        | VncControlMessage::Connect { .. }
        | VncControlMessage::Input { .. }
        | VncControlMessage::RequestFullFrame { .. }
        | VncControlMessage::Disconnect { .. } => {
            return Err(VncError::new(
                VncErrorKind::Protocol,
                "VNC helper sent an application-only control message",
            ));
        }
        VncControlMessage::DesktopReset {
            session_id: event_session,
            epoch,
            width,
            height,
        } if event_session == session_id => queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push_reset(session_id, epoch, width, height),
        VncControlMessage::State {
            session_id: event_session,
            state: new_state,
            message,
        } if event_session == session_id => {
            set_state(state, new_state);
            queue
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push_control(VncRuntimeEvent::State {
                    session_id: session_id.to_string(),
                    state: new_state,
                    message,
                });
        }
        VncControlMessage::Clipboard {
            session_id: event_session,
            text,
        } if event_session == session_id => queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push_control(VncRuntimeEvent::Clipboard {
                session_id: session_id.to_string(),
                text,
            }),
        VncControlMessage::Error {
            session_id: event_session,
            error,
            fatal,
        } if event_session == session_id => {
            if fatal {
                set_state(state, VncSessionState::Failed);
            }
            queue
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push_control(VncRuntimeEvent::Error {
                    session_id: session_id.to_string(),
                    error,
                    fatal,
                });
        }
        _ => {}
    }
    Ok(())
}

fn push_reader_error(
    session_id: &str,
    queue: &Arc<Mutex<EventQueue>>,
    state: &Arc<Mutex<VncSessionState>>,
    kind: VncErrorKind,
    message: String,
) {
    let error = VncError::new(kind, message);
    set_state(state, VncSessionState::Failed);
    let mut queue = queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    queue.push_control(VncRuntimeEvent::Error {
        session_id: session_id.to_string(),
        error,
        fatal: true,
    });
    queue.push_control(VncRuntimeEvent::State {
        session_id: session_id.to_string(),
        state: VncSessionState::Failed,
        message: None,
    });
}

fn set_state(state: &Arc<Mutex<VncSessionState>>, new_state: VncSessionState) {
    *state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = new_state;
}

fn cleanup_child(record: &mut SessionRecord) {
    helper_process::cleanup_child(&mut record.child, &mut record.reader);
}

fn is_latin1_within_limit(text: &str) -> bool {
    text.len() <= MAX_VNC_CLIPBOARD_TEXT_BYTES && text.chars().all(|ch| u32::from(ch) <= 0xff)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use super::{
        EventQueue, FRAME_QUEUE_LIMIT, handle_control, is_latin1_within_limit,
        require_server_hello, validate_vnc_config, validate_vnc_input,
    };
    use crate::{
        Framebuffer, MAX_VNC_CLIPBOARD_TEXT_BYTES, PROTOCOL_VERSION, PixelFormat, RdpFrameEvent,
        VncClipboardConfig, VncControlMessage, VncDisplayConfig, VncErrorKind, VncInputEvent,
        VncReconnectConfig, VncRuntimeEvent, VncSecurityConfig, VncSecurityMode,
        VncServerCapabilities, VncSessionConfig, VncSessionState,
    };

    #[test]
    fn server_hello_must_be_first_and_records_capabilities_only_once() {
        let queue = Arc::new(Mutex::new(EventQueue::default()));
        let state = Arc::new(Mutex::new(VncSessionState::Connecting));
        let capabilities = Arc::new(Mutex::new(None));
        let advertised = VncServerCapabilities {
            committed_unicode_keysyms: true,
        };
        let mut hello_received = false;

        let error = handle_control(
            "s",
            VncControlMessage::Error {
                session_id: "s".to_string(),
                error: crate::VncError::new(VncErrorKind::Protocol, "before hello"),
                fatal: true,
            },
            &queue,
            &state,
            &capabilities,
            &mut hello_received,
        )
        .expect_err("error before ServerHello must fail");
        assert_eq!(error.kind, VncErrorKind::Protocol);
        assert!(!hello_received);
        assert_eq!(*capabilities.lock().unwrap(), None);

        let error = handle_control(
            "s",
            VncControlMessage::ServerHello {
                version: PROTOCOL_VERSION - 1,
                capabilities: advertised,
            },
            &queue,
            &state,
            &capabilities,
            &mut hello_received,
        )
        .expect_err("a mismatched version must fail");
        assert_eq!(error.kind, VncErrorKind::Protocol);
        assert!(!hello_received);
        assert_eq!(*capabilities.lock().unwrap(), None);

        handle_control(
            "s",
            VncControlMessage::ServerHello {
                version: PROTOCOL_VERSION,
                capabilities: advertised,
            },
            &queue,
            &state,
            &capabilities,
            &mut hello_received,
        )
        .expect("matching hello");
        assert!(hello_received);
        assert_eq!(*capabilities.lock().unwrap(), Some(advertised));

        let error = handle_control(
            "s",
            VncControlMessage::ServerHello {
                version: PROTOCOL_VERSION,
                capabilities: VncServerCapabilities::default(),
            },
            &queue,
            &state,
            &capabilities,
            &mut hello_received,
        )
        .expect_err("duplicate ServerHello must fail");
        assert_eq!(error.kind, VncErrorKind::Protocol);
        assert_eq!(*capabilities.lock().unwrap(), Some(advertised));

        let error = handle_control(
            "s",
            VncControlMessage::ClientHello {
                version: PROTOCOL_VERSION,
            },
            &queue,
            &state,
            &capabilities,
            &mut hello_received,
        )
        .expect_err("application-only messages from the helper must fail");
        assert_eq!(error.kind, VncErrorKind::Protocol);
        assert!(require_server_hello(false, "frame packet").is_err());
        assert!(require_server_hello(false, "cursor packet").is_err());
        assert!(require_server_hello(true, "frame packet").is_ok());
    }

    #[test]
    fn vnc_committed_text_batch_is_fully_validated() {
        let events = vec![
            VncInputEvent::Text {
                text: "Unicode 文本".to_string(),
            },
            VncInputEvent::Text {
                text: "invalid\u{001b}".to_string(),
            },
        ];
        assert!(validate_vnc_input(&events).is_err());
        assert!(!validate_vnc_input(&[VncInputEvent::ReleaseAllKeys]).unwrap());
    }

    #[test]
    fn every_producer_path_wakes_the_consumer() {
        let signals = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&signals);
        let mut queue = EventQueue {
            waker: Some(Arc::new(move || {
                counter.fetch_add(1, Ordering::Relaxed);
            })),
            ..EventQueue::default()
        };

        queue.push_reset("s", 1, 4, 4);
        assert_eq!(signals.load(Ordering::Relaxed), 1, "a reset must wake");
        queue.push_frame(frame(1, 0, true));
        assert_eq!(signals.load(Ordering::Relaxed), 2, "a frame must wake");
        queue.push_control(VncRuntimeEvent::Clipboard {
            session_id: "s".to_string(),
            text: String::new(),
        });
        assert_eq!(
            signals.load(Ordering::Relaxed),
            3,
            "a control event must wake"
        );
    }

    fn config() -> VncSessionConfig {
        VncSessionConfig {
            name: "vnc".to_string(),
            host: "127.0.0.1".to_string(),
            port: 5900,
            password: None,
            security: VncSecurityConfig::default(),
            display: VncDisplayConfig::default(),
            clipboard: VncClipboardConfig::default(),
            reconnect: VncReconnectConfig::default(),
            shared: true,
            view_only: false,
        }
    }

    fn frame(epoch: u64, x: u32, full: bool) -> RdpFrameEvent {
        RdpFrameEvent::Bitmap {
            epoch,
            full,
            x,
            y: 0,
            width: 1,
            height: 1,
            stride: 4,
            format: PixelFormat::Rgba8,
            pixels: vec![x as u8; 4],
        }
    }

    #[test]
    fn validates_password_length_for_classic_vnc_auth() {
        let mut config = config();
        config.security.mode = VncSecurityMode::VncAuth;
        config.password = Some("123456789".to_string());
        let error = validate_vnc_config(&config).expect_err("long password should fail");
        assert_eq!(error.kind, VncErrorKind::Authentication);
    }

    #[test]
    fn validates_clipboard_latin1_limit() {
        assert!(is_latin1_within_limit("hello"));
        assert!(!is_latin1_within_limit("hello \u{0100}"));
        assert!(!is_latin1_within_limit(
            &"a".repeat(MAX_VNC_CLIPBOARD_TEXT_BYTES + 1)
        ));
    }

    #[test]
    fn rgba_vnc_frame_reaches_shared_framebuffer_as_bgra() {
        let mut framebuffer = Framebuffer::new(1, 1, 1).expect("framebuffer");
        let frame = RdpFrameEvent::Bitmap {
            epoch: 1,
            full: true,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            stride: 4,
            format: PixelFormat::Rgba8,
            pixels: vec![10, 20, 30, 255],
        };
        framebuffer.apply(&frame).expect("apply");
        assert_eq!(framebuffer.pixels(), &[30, 20, 10, 255]);
    }

    #[test]
    fn frames_from_a_superseded_epoch_are_dropped() {
        let mut queue = EventQueue::default();
        queue.push_reset("s", 2, 4, 4);
        assert!(!queue.push_frame(frame(1, 0, true)));
        assert!(queue.frames.is_empty());
        assert_eq!(queue.dropped_frames, 1);
        assert!(!queue.push_frame(frame(2, 0, true)));
        assert_eq!(queue.frames.len(), 1);
    }

    #[test]
    fn partial_frames_are_kept_while_waiting_for_a_full_frame() {
        // A VNC frame is flagged `full` only by starting at the origin, so interior
        // rectangles must still paint instead of being withheld.
        let mut queue = EventQueue::default();
        queue.push_reset("s", 1, 100, 100);
        assert!(queue.waiting_for_full_frame);
        assert!(!queue.push_frame(frame(1, 40, false)));
        assert_eq!(queue.frames.len(), 1);
        assert!(queue.waiting_for_full_frame);
        assert!(!queue.push_frame(frame(1, 0, true)));
        assert!(!queue.waiting_for_full_frame);
    }

    #[test]
    fn overflow_requests_a_full_frame_and_reports_the_drop() {
        let mut queue = EventQueue::default();
        queue.push_reset("s", 1, 100, 100);
        for x in 0..FRAME_QUEUE_LIMIT as u32 {
            assert!(!queue.push_frame(frame(1, x, false)));
        }
        assert!(queue.push_frame(frame(1, FRAME_QUEUE_LIMIT as u32, false)));
        assert!(queue.frames.is_empty());
        assert!(queue.waiting_for_full_frame);
        assert_eq!(queue.dropped_frames, FRAME_QUEUE_LIMIT + 1);
    }

    #[test]
    fn drain_reports_and_clears_drop_accounting() {
        let mut queue = EventQueue::default();
        queue.push_reset("s", 1, 4, 4);
        queue.push_frame(frame(1, 0, true));
        let drain = queue.drain();
        assert_eq!(drain.frames.len(), 1);
        assert_eq!(drain.control.len(), 1);
        assert_eq!(drain.dropped_frames, 0);
        let drain = queue.drain();
        assert!(drain.frames.is_empty());
        assert!(drain.control.is_empty());
    }
}
