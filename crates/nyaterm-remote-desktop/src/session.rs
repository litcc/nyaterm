use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::{Child, ChildStdin};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use uuid::Uuid;

use crate::helper_process;
use crate::{
    PROTOCOL_VERSION, PacketType, QueueWaker, RdpCertificateResponse, RdpControlMessage, RdpError,
    RdpErrorKind, RdpFrameEvent, RdpInputEvent, RdpRuntimeEvent, RdpServerCapabilities,
    RdpSessionConfig, RdpSessionDrain, RdpSessionState, decode_control, decode_cursor_packet,
    decode_frame_packet, encode_control, read_packet, validate_committed_text, write_packet,
};

const MIN_WIDTH: u32 = 200;
const MIN_HEIGHT: u32 = 200;
const MAX_WIDTH: u32 = 8192;
const MAX_HEIGHT: u32 = 8192;
const FRAME_QUEUE_LIMIT: usize = 64;
const HELPER_PACKAGE: &str = "nyaterm-rdp-helper";
const HELPER_ENV_VAR: &str = "NYATERM_RDP_HELPER";

pub fn resolve_helper_path() -> Result<PathBuf, RdpError> {
    helper_process::resolve_helper(HELPER_PACKAGE, HELPER_ENV_VAR)
        .map_err(|message| RdpError::new(RdpErrorKind::HelperMissing, message))
}

#[derive(Default)]
struct EventQueue {
    /// Signalled after anything is enqueued. Held here rather than at each
    /// call site so every producer path wakes the consumer.
    waker: Option<QueueWaker>,
    control: VecDeque<RdpRuntimeEvent>,
    frames: VecDeque<RdpFrameEvent>,
    current_epoch: Option<u64>,
    waiting_for_full_frame: bool,
    dropped_frames: usize,
}

impl EventQueue {
    fn push_control(&mut self, event: RdpRuntimeEvent) {
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
        self.control.push_back(RdpRuntimeEvent::Frame {
            session_id: session_id.to_string(),
            event: RdpFrameEvent::Reset {
                epoch,
                width,
                height,
            },
        });
        self.wake();
    }

    fn push_frame(&mut self, frame: RdpFrameEvent) -> bool {
        let dropped = self.push_frame_inner(frame);
        // Wake unconditionally rather than mirroring the branch structure below.
        // Two paths discard a frame for a stale epoch without touching the queue,
        // and a redundant wake there costs one empty drain -- cheaper than a
        // missed wake, and those paths only occur briefly after a resize.
        self.wake();
        dropped
    }

    fn push_frame_inner(&mut self, frame: RdpFrameEvent) -> bool {
        let RdpFrameEvent::Bitmap {
            epoch,
            full,
            x,
            y,
            width,
            height,
            ..
        } = &frame
        else {
            self.frames.push_back(frame);
            return false;
        };
        if self.current_epoch != Some(*epoch) {
            return false;
        }
        if self.waiting_for_full_frame {
            if !*full {
                return false;
            }
            self.frames.clear();
            self.waiting_for_full_frame = false;
            self.frames.push_back(frame);
            return false;
        }
        if let Some(existing) = self.frames.iter_mut().rev().find(|queued| matches!(queued,
            RdpFrameEvent::Bitmap { epoch: queued_epoch, full: false, x: queued_x, y: queued_y, width: queued_width, height: queued_height, .. }
                if queued_epoch == epoch && queued_x == x && queued_y == y && queued_width == width && queued_height == height
        )) {
            *existing = frame;
            return false;
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

    fn drain(&mut self) -> RdpSessionDrain {
        RdpSessionDrain {
            control: self.control.drain(..).collect(),
            frames: self.frames.drain(..).collect(),
            dropped_frames: std::mem::take(&mut self.dropped_frames),
            waiting_for_full_frame: self.waiting_for_full_frame,
        }
    }
}

struct SessionRecord {
    state: Arc<Mutex<RdpSessionState>>,
    capabilities: Arc<Mutex<Option<RdpServerCapabilities>>>,
    queue: Arc<Mutex<EventQueue>>,
    writer: Arc<Mutex<ChildStdin>>,
    child: Option<Child>,
    reader: Option<JoinHandle<()>>,
}

#[derive(Default)]
pub struct RdpSessionManager {
    sessions: Mutex<HashMap<String, SessionRecord>>,
    pending_certificates: Arc<Mutex<HashMap<String, String>>>,
    /// Installed once by the application; copied into every session queue so
    /// the reader thread can wake the consumer instead of being polled.
    queue_waker: Mutex<Option<QueueWaker>>,
}

impl RdpSessionManager {
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

    pub fn create_session(&self, config: RdpSessionConfig) -> Result<String, RdpError> {
        self.create_session_with_id(Uuid::new_v4().to_string(), config)
    }

    pub fn create_session_with_id(
        &self,
        session_id: String,
        config: RdpSessionConfig,
    ) -> Result<String, RdpError> {
        validate_config(&config)?;
        {
            let mut sessions = self.sessions.lock().map_err(|_| {
                RdpError::new(RdpErrorKind::Ipc, "RDP session registry lock is poisoned")
            })?;
            if sessions
                .get(&session_id)
                .is_some_and(|record| record.child.is_some())
            {
                return Err(RdpError::new(
                    RdpErrorKind::Protocol,
                    format!("RDP session '{session_id}' is already running"),
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
            RdpError::new(
                RdpErrorKind::HelperMissing,
                format!("failed to spawn the RDP helper: {error}"),
            )
        })?;
        let writer = Arc::new(Mutex::new(stdin));
        let queue = Arc::new(Mutex::new(EventQueue {
            waker: self.queue_waker(),
            ..EventQueue::default()
        }));
        let state = Arc::new(Mutex::new(RdpSessionState::Connecting));
        let capabilities = Arc::new(Mutex::new(None));
        let reader = spawn_reader(
            session_id.clone(),
            stdout,
            writer.clone(),
            queue.clone(),
            state.clone(),
            capabilities.clone(),
            self.pending_certificates.clone(),
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
            &RdpControlMessage::ClientHello {
                version: PROTOCOL_VERSION,
            },
        )
        .and_then(|()| {
            send_control(
                &record.writer,
                &RdpControlMessage::Connect {
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
            .push_control(RdpRuntimeEvent::State {
                session_id: session_id.clone(),
                state: RdpSessionState::Connecting,
                message: None,
            });
        let mut sessions = match self.sessions.lock() {
            Ok(sessions) => sessions,
            Err(_) => {
                cleanup_child(&mut record);
                return Err(RdpError::new(
                    RdpErrorKind::Ipc,
                    "RDP session registry lock is poisoned",
                ));
            }
        };
        if sessions.contains_key(&session_id) {
            drop(sessions);
            cleanup_child(&mut record);
            return Err(RdpError::new(
                RdpErrorKind::Protocol,
                format!("RDP session '{session_id}' was created concurrently"),
            ));
        }
        sessions.insert(session_id.clone(), record);
        Ok(session_id)
    }

    pub fn state(&self, session_id: &str) -> Option<RdpSessionState> {
        let sessions = self.sessions.lock().ok()?;
        sessions
            .get(session_id)?
            .state
            .lock()
            .ok()
            .map(|state| state.clone())
    }

    pub fn drain(&self, session_id: &str) -> RdpSessionDrain {
        let Ok(sessions) = self.sessions.lock() else {
            return RdpSessionDrain::default();
        };
        let Some(record) = sessions.get(session_id) else {
            return RdpSessionDrain::default();
        };
        record
            .queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .drain()
    }

    pub fn drain_events(&self, session_id: &str) -> Vec<RdpRuntimeEvent> {
        let mut drain = self.drain(session_id);
        drain
            .control
            .extend(drain.frames.drain(..).map(|event| RdpRuntimeEvent::Frame {
                session_id: session_id.to_string(),
                event,
            }));
        drain.control
    }

    /// Return the static helper capabilities confirmed by `ServerHello`.
    ///
    /// `None` means that no valid hello has been received; capability-dependent
    /// operations fail closed in that state.
    pub fn server_capabilities(&self, session_id: &str) -> Option<RdpServerCapabilities> {
        let sessions = self.sessions.lock().ok()?;
        *sessions.get(session_id)?.capabilities.lock().ok()?
    }

    pub fn send_input(&self, session_id: &str, events: Vec<RdpInputEvent>) -> Result<(), RdpError> {
        let needs_committed_text = validate_rdp_input(&events)?;
        if needs_committed_text {
            let capabilities = self.confirmed_capabilities(session_id)?;
            if !capabilities.committed_unicode_text {
                return Err(RdpError::new(
                    RdpErrorKind::Unsupported,
                    "RDP helper did not advertise committed Unicode text input",
                ));
            }
        }
        self.send(
            session_id,
            RdpControlMessage::Input {
                session_id: session_id.to_string(),
                events,
            },
        )
    }

    pub fn send_secure_attention(&self, session_id: &str) -> Result<(), RdpError> {
        if !self.confirmed_capabilities(session_id)?.secure_attention {
            return Err(RdpError::new(
                RdpErrorKind::Unsupported,
                "RDP helper did not advertise Secure Attention",
            ));
        }
        self.send(
            session_id,
            RdpControlMessage::SecureAttention {
                session_id: session_id.to_string(),
            },
        )
    }

    pub fn resize(&self, session_id: &str, width: u32, height: u32) -> Result<(), RdpError> {
        validate_size(width, height)?;
        self.send(
            session_id,
            RdpControlMessage::Resize {
                session_id: session_id.to_string(),
                width,
                height,
            },
        )
    }

    pub fn set_clipboard_text(&self, session_id: &str, text: String) -> Result<(), RdpError> {
        if text.len() > crate::MAX_CLIPBOARD_TEXT_BYTES {
            return Err(RdpError::new(
                RdpErrorKind::Unsupported,
                "clipboard text exceeds the 4 MiB limit",
            ));
        }
        self.send(
            session_id,
            RdpControlMessage::Clipboard {
                session_id: session_id.to_string(),
                text,
                generation: 0,
            },
        )
    }

    pub fn respond_certificate(
        &self,
        request_id: &str,
        response: RdpCertificateResponse,
    ) -> Result<(), RdpError> {
        let session_id = self
            .pending_certificates
            .lock()
            .map_err(|_| {
                RdpError::new(
                    RdpErrorKind::Ipc,
                    "RDP certificate registry lock is poisoned",
                )
            })?
            .remove(request_id)
            .ok_or_else(|| {
                RdpError::new(
                    RdpErrorKind::Protocol,
                    format!("RDP certificate request '{request_id}' was not found"),
                )
            })?;
        self.send(
            &session_id,
            RdpControlMessage::CertificateResponse {
                request_id: request_id.to_string(),
                response,
            },
        )
    }

    pub fn close(&self, session_id: &str) -> Result<(), RdpError> {
        let mut record = self
            .sessions
            .lock()
            .map_err(|_| RdpError::new(RdpErrorKind::Ipc, "RDP session registry lock is poisoned"))?
            .remove(session_id)
            .ok_or_else(|| {
                RdpError::new(
                    RdpErrorKind::Protocol,
                    format!("RDP session '{session_id}' was not found"),
                )
            })?;
        set_state(&record.state, RdpSessionState::Disconnecting);
        let _ = send_control(
            &record.writer,
            &RdpControlMessage::Input {
                session_id: session_id.to_string(),
                events: vec![RdpInputEvent::ReleaseAllKeys],
            },
        );
        let _ = send_control(
            &record.writer,
            &RdpControlMessage::Disconnect {
                session_id: session_id.to_string(),
            },
        );
        cleanup_child(&mut record);
        set_state(&record.state, RdpSessionState::Disconnected);
        record
            .queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push_control(RdpRuntimeEvent::State {
                session_id: session_id.to_string(),
                state: RdpSessionState::Disconnected,
                message: None,
            });
        self.pending_certificates
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|_, pending_session| pending_session != session_id);
        self.sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(session_id.to_string(), record);
        Ok(())
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

    fn confirmed_capabilities(&self, session_id: &str) -> Result<RdpServerCapabilities, RdpError> {
        let sessions = self.sessions.lock().map_err(|_| {
            RdpError::new(RdpErrorKind::Ipc, "RDP session registry lock is poisoned")
        })?;
        let record = sessions.get(session_id).ok_or_else(|| {
            RdpError::new(
                RdpErrorKind::Protocol,
                format!("RDP session '{session_id}' was not found"),
            )
        })?;
        record
            .capabilities
            .lock()
            .map_err(|_| RdpError::new(RdpErrorKind::Ipc, "RDP capability lock is poisoned"))?
            .as_ref()
            .copied()
            .ok_or_else(|| {
                RdpError::new(
                    RdpErrorKind::Protocol,
                    "RDP helper capabilities have not been confirmed",
                )
            })
    }

    fn send(&self, session_id: &str, message: RdpControlMessage) -> Result<(), RdpError> {
        let sessions = self.sessions.lock().map_err(|_| {
            RdpError::new(RdpErrorKind::Ipc, "RDP session registry lock is poisoned")
        })?;
        let record = sessions.get(session_id).ok_or_else(|| {
            RdpError::new(
                RdpErrorKind::Protocol,
                format!("RDP session '{session_id}' was not found"),
            )
        })?;
        send_control(&record.writer, &message)
    }
}

impl Drop for RdpSessionManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn send_control(
    writer: &Arc<Mutex<ChildStdin>>,
    message: &RdpControlMessage,
) -> Result<(), RdpError> {
    let packet = encode_control(message)
        .map_err(|error| RdpError::new(RdpErrorKind::Ipc, error.to_string()))?;
    write_packet(
        &mut *writer
            .lock()
            .map_err(|_| RdpError::new(RdpErrorKind::Ipc, "RDP helper writer lock is poisoned"))?,
        &packet,
    )
    .map_err(|error| RdpError::new(RdpErrorKind::Ipc, error.to_string()))
}

fn spawn_reader(
    session_id: String,
    mut stdout: std::process::ChildStdout,
    writer: Arc<Mutex<ChildStdin>>,
    queue: Arc<Mutex<EventQueue>>,
    state: Arc<Mutex<RdpSessionState>>,
    capabilities: Arc<Mutex<Option<RdpServerCapabilities>>>,
    pending_certificates: Arc<Mutex<HashMap<String, String>>>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name(format!("nyaterm-rdp-reader-{session_id}"))
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
                            RdpErrorKind::Ipc,
                            error.to_string(),
                        );
                        return;
                    }
                };
                let result: Result<(), RdpError> = match packet.packet_type {
                    PacketType::Control => decode_control(&packet)
                        .map_err(|error| RdpError::new(RdpErrorKind::Protocol, error.to_string()))
                        .and_then(|message| {
                            handle_control(
                                &session_id,
                                message,
                                &queue,
                                &state,
                                &capabilities,
                                &pending_certificates,
                                &mut hello_received,
                            )
                        }),
                    PacketType::Frame => require_server_hello(hello_received, "frame packet")
                        .and_then(|()| {
                            decode_frame_packet(&packet).map_err(|error| {
                                RdpError::new(RdpErrorKind::Protocol, error.to_string())
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
                                let _ = send_control(
                                    &writer,
                                    &RdpControlMessage::RequestFullFrame {
                                        session_id: session_id.clone(),
                                    },
                                );
                            }
                        }),
                    PacketType::Cursor => require_server_hello(hello_received, "cursor packet")
                        .and_then(|()| {
                            decode_cursor_packet(&packet).map_err(|error| {
                                RdpError::new(RdpErrorKind::Protocol, error.to_string())
                            })
                        })
                        .map(|(cursor_session, cursor)| {
                            if cursor_session == session_id {
                                queue
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                                    .push_control(RdpRuntimeEvent::Frame {
                                        session_id: session_id.clone(),
                                        event: RdpFrameEvent::Cursor(cursor),
                                    });
                            }
                        }),
                };
                if let Err(error) = result {
                    push_reader_error(&session_id, &queue, &state, error.kind, error.message);
                    return;
                }
            }
            let current = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            if !matches!(
                current,
                RdpSessionState::Disconnecting
                    | RdpSessionState::Disconnected
                    | RdpSessionState::Failed(_)
            ) {
                push_reader_error(
                    &session_id,
                    &queue,
                    &state,
                    RdpErrorKind::HelperCrashed,
                    "RDP helper exited unexpectedly".to_string(),
                );
            }
        })
        .expect("failed to spawn RDP helper reader")
}

fn require_server_hello(received: bool, packet_kind: &str) -> Result<(), RdpError> {
    if received {
        Ok(())
    } else {
        Err(RdpError::new(
            RdpErrorKind::Protocol,
            format!("RDP helper sent {packet_kind} before ServerHello"),
        ))
    }
}

fn handle_control(
    session_id: &str,
    message: RdpControlMessage,
    queue: &Arc<Mutex<EventQueue>>,
    state: &Arc<Mutex<RdpSessionState>>,
    capabilities: &Arc<Mutex<Option<RdpServerCapabilities>>>,
    pending: &Arc<Mutex<HashMap<String, String>>>,
    hello_received: &mut bool,
) -> Result<(), RdpError> {
    if !*hello_received {
        let RdpControlMessage::ServerHello {
            version,
            capabilities: confirmed,
        } = message
        else {
            return Err(RdpError::new(
                RdpErrorKind::Protocol,
                "RDP helper's first control message must be ServerHello",
            ));
        };
        if version != PROTOCOL_VERSION {
            return Err(RdpError::new(
                RdpErrorKind::Protocol,
                format!("RDP helper protocol version {version} does not match {PROTOCOL_VERSION}"),
            ));
        }
        let mut slot = capabilities
            .lock()
            .map_err(|_| RdpError::new(RdpErrorKind::Ipc, "RDP capability lock is poisoned"))?;
        if slot.is_some() {
            return Err(RdpError::new(
                RdpErrorKind::Protocol,
                "RDP helper capabilities were already confirmed",
            ));
        }
        *slot = Some(confirmed);
        *hello_received = true;
        return Ok(());
    }

    match message {
        RdpControlMessage::ServerHello { .. } => {
            return Err(RdpError::new(
                RdpErrorKind::Protocol,
                "RDP helper sent duplicate ServerHello",
            ));
        }
        RdpControlMessage::ClientHello { .. }
        | RdpControlMessage::Connect { .. }
        | RdpControlMessage::Input { .. }
        | RdpControlMessage::SecureAttention { .. }
        | RdpControlMessage::Resize { .. }
        | RdpControlMessage::CertificateResponse { .. }
        | RdpControlMessage::RequestFullFrame { .. }
        | RdpControlMessage::Disconnect { .. } => {
            return Err(RdpError::new(
                RdpErrorKind::Protocol,
                "RDP helper sent an application-only control message",
            ));
        }
        RdpControlMessage::DesktopReset {
            session_id: event_session,
            epoch,
            width,
            height,
        } if event_session == session_id => queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push_reset(session_id, epoch, width, height),
        RdpControlMessage::State {
            session_id: event_session,
            state: new_state,
            message,
        } if event_session == session_id => {
            set_state(state, new_state.clone());
            queue
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push_control(RdpRuntimeEvent::State {
                    session_id: session_id.to_string(),
                    state: new_state,
                    message,
                });
        }
        RdpControlMessage::Clipboard {
            session_id: event_session,
            text,
            generation,
        } if event_session == session_id => queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push_control(RdpRuntimeEvent::Clipboard {
                session_id: session_id.to_string(),
                text,
                generation,
            }),
        RdpControlMessage::CertificateRequest(request) => {
            pending
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(request.request_id.clone(), session_id.to_string());
            queue
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push_control(RdpRuntimeEvent::CertificateRequest(request));
        }
        RdpControlMessage::Capability {
            session_id: event_session,
            capability,
        } if event_session == session_id => queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push_control(RdpRuntimeEvent::Capability {
                session_id: session_id.to_string(),
                capability,
            }),
        RdpControlMessage::Error {
            session_id: event_session,
            error,
            fatal,
        } if event_session == session_id => {
            if fatal {
                set_state(state, RdpSessionState::Failed(error.clone()));
            }
            queue
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push_control(RdpRuntimeEvent::Error {
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
    state: &Arc<Mutex<RdpSessionState>>,
    kind: RdpErrorKind,
    message: String,
) {
    let error = RdpError::new(kind, message);
    set_state(state, RdpSessionState::Failed(error.clone()));
    let mut queue = queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    queue.push_control(RdpRuntimeEvent::Error {
        session_id: session_id.to_string(),
        error: error.clone(),
        fatal: true,
    });
    queue.push_control(RdpRuntimeEvent::State {
        session_id: session_id.to_string(),
        state: RdpSessionState::Failed(error),
        message: None,
    });
}

fn set_state(state: &Arc<Mutex<RdpSessionState>>, new_state: RdpSessionState) {
    *state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = new_state;
}

fn cleanup_child(record: &mut SessionRecord) {
    helper_process::cleanup_child(&mut record.child, &mut record.reader);
}

fn validate_rdp_input(events: &[RdpInputEvent]) -> Result<bool, RdpError> {
    let mut has_committed_text = false;
    for event in events {
        if let RdpInputEvent::Unicode { text } = event {
            validate_committed_text(text).map_err(|error| {
                RdpError::new(
                    RdpErrorKind::Protocol,
                    format!("invalid RDP committed text: {error}"),
                )
            })?;
            has_committed_text = true;
        }
    }
    Ok(has_committed_text)
}

fn validate_config(config: &RdpSessionConfig) -> Result<(), RdpError> {
    if config.host.trim().is_empty() {
        return Err(RdpError::new(
            RdpErrorKind::Protocol,
            "RDP host is required",
        ));
    }
    validate_size(config.display.width, config.display.height)?;
    if !matches!(config.display.color_depth, 16 | 24 | 32) {
        return Err(RdpError::new(
            RdpErrorKind::Unsupported,
            "RDP color depth must be 16, 24, or 32",
        ));
    }
    Ok(())
}

fn validate_size(width: u32, height: u32) -> Result<(), RdpError> {
    if !(MIN_WIDTH..=MAX_WIDTH).contains(&width) || !(MIN_HEIGHT..=MAX_HEIGHT).contains(&height) {
        return Err(RdpError::new(
            RdpErrorKind::Unsupported,
            "RDP desktop size is outside the supported range",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use super::{
        EventQueue, FRAME_QUEUE_LIMIT, handle_control, require_server_hello, validate_rdp_input,
    };
    use crate::{
        PROTOCOL_VERSION, PixelFormat, RdpCapability, RdpControlMessage, RdpFrameEvent,
        RdpInputEvent, RdpRuntimeEvent, RdpServerCapabilities, RdpSessionState,
    };

    #[test]
    fn server_hello_must_be_first_and_records_capabilities_only_once() {
        let queue = Arc::new(Mutex::new(EventQueue::default()));
        let state = Arc::new(Mutex::new(RdpSessionState::Connecting));
        let capabilities = Arc::new(Mutex::new(None));
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let advertised = RdpServerCapabilities {
            committed_unicode_text: true,
            secure_attention: true,
        };
        let mut hello_received = false;

        let error = handle_control(
            "s",
            RdpControlMessage::State {
                session_id: "s".to_string(),
                state: RdpSessionState::Connecting,
                message: None,
            },
            &queue,
            &state,
            &capabilities,
            &pending,
            &mut hello_received,
        )
        .expect_err("state before ServerHello must fail");
        assert_eq!(error.kind, crate::RdpErrorKind::Protocol);
        assert!(!hello_received);
        assert_eq!(*capabilities.lock().unwrap(), None);

        let error = handle_control(
            "s",
            RdpControlMessage::ServerHello {
                version: PROTOCOL_VERSION - 1,
                capabilities: advertised,
            },
            &queue,
            &state,
            &capabilities,
            &pending,
            &mut hello_received,
        )
        .expect_err("a mismatched version must fail");
        assert_eq!(error.kind, crate::RdpErrorKind::Protocol);
        assert!(!hello_received);
        assert_eq!(*capabilities.lock().unwrap(), None);

        handle_control(
            "s",
            RdpControlMessage::ServerHello {
                version: PROTOCOL_VERSION,
                capabilities: advertised,
            },
            &queue,
            &state,
            &capabilities,
            &pending,
            &mut hello_received,
        )
        .expect("matching hello");
        assert!(hello_received);
        assert_eq!(*capabilities.lock().unwrap(), Some(advertised));

        let error = handle_control(
            "s",
            RdpControlMessage::ServerHello {
                version: PROTOCOL_VERSION,
                capabilities: RdpServerCapabilities::default(),
            },
            &queue,
            &state,
            &capabilities,
            &pending,
            &mut hello_received,
        )
        .expect_err("duplicate ServerHello must fail");
        assert_eq!(error.kind, crate::RdpErrorKind::Protocol);
        assert_eq!(*capabilities.lock().unwrap(), Some(advertised));

        let error = handle_control(
            "s",
            RdpControlMessage::ClientHello {
                version: PROTOCOL_VERSION,
            },
            &queue,
            &state,
            &capabilities,
            &pending,
            &mut hello_received,
        )
        .expect_err("application-only messages from the helper must fail");
        assert_eq!(error.kind, crate::RdpErrorKind::Protocol);
        assert!(require_server_hello(false, "frame packet").is_err());
        assert!(require_server_hello(false, "cursor packet").is_err());
        assert!(require_server_hello(true, "frame packet").is_ok());
    }

    #[test]
    fn rdp_committed_text_batch_is_fully_validated() {
        let events = vec![
            RdpInputEvent::Unicode {
                text: "valid text".to_string(),
            },
            RdpInputEvent::Unicode {
                text: "invalid\ntext".to_string(),
            },
        ];
        assert!(validate_rdp_input(&events).is_err());
        assert!(!validate_rdp_input(&[RdpInputEvent::ReleaseAllKeys]).unwrap());
    }

    #[test]
    fn dynamic_resize_unavailable_remains_a_runtime_capability_event() {
        let queue = Arc::new(Mutex::new(EventQueue::default()));
        let state = Arc::new(Mutex::new(RdpSessionState::Connected));
        let capabilities = Arc::new(Mutex::new(Some(RdpServerCapabilities {
            committed_unicode_text: true,
            secure_attention: true,
        })));
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let mut hello_received = true;

        handle_control(
            "s",
            RdpControlMessage::Capability {
                session_id: "s".to_string(),
                capability: RdpCapability::DynamicResizeUnavailable,
            },
            &queue,
            &state,
            &capabilities,
            &pending,
            &mut hello_received,
        )
        .expect("runtime capability event");

        assert_eq!(
            *capabilities.lock().unwrap(),
            Some(RdpServerCapabilities {
                committed_unicode_text: true,
                secure_attention: true,
            })
        );
        assert!(matches!(
            queue.lock().unwrap().control.front(),
            Some(RdpRuntimeEvent::Capability {
                capability: RdpCapability::DynamicResizeUnavailable,
                ..
            })
        ));
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
        queue.push_control(RdpRuntimeEvent::Clipboard {
            session_id: "s".to_string(),
            text: String::new(),
            generation: 0,
        });
        assert_eq!(
            signals.load(Ordering::Relaxed),
            3,
            "a control event must wake"
        );
    }

    #[test]
    fn a_queue_without_a_waker_still_works() {
        let mut queue = EventQueue::default();

        queue.push_reset("s", 1, 4, 4);
        queue.push_frame(frame(1, 0, true));

        assert_eq!(queue.drain().frames.len(), 1);
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
            format: PixelFormat::Bgra8,
            pixels: vec![x as u8; 4],
        }
    }

    #[test]
    fn reset_discards_old_epoch_and_requires_full_frame() {
        let mut queue = EventQueue::default();
        queue.push_reset("s", 2, 4, 4);
        queue.push_frame(frame(1, 0, true));
        queue.push_frame(frame(2, 0, false));
        assert!(queue.frames.is_empty());
        queue.push_frame(frame(2, 0, true));
        assert_eq!(queue.frames.len(), 1);
        assert!(!queue.waiting_for_full_frame);
    }

    #[test]
    fn overflow_clears_dirty_frames_and_waits_for_full_frame() {
        let mut queue = EventQueue::default();
        queue.push_reset("s", 1, 100, 100);
        queue.push_frame(frame(1, 0, true));
        queue.frames.clear();
        for x in 0..FRAME_QUEUE_LIMIT as u32 {
            assert!(!queue.push_frame(frame(1, x, false)));
        }
        assert!(queue.push_frame(frame(1, FRAME_QUEUE_LIMIT as u32, false)));
        assert!(queue.frames.is_empty());
        assert!(queue.waiting_for_full_frame);
        queue.push_frame(frame(1, 99, false));
        assert!(queue.frames.is_empty());
        queue.push_frame(frame(1, 0, true));
        assert_eq!(queue.frames.len(), 1);
    }

    #[test]
    fn identical_dirty_region_is_replaced() {
        let mut queue = EventQueue::default();
        queue.push_reset("s", 1, 4, 4);
        queue.push_frame(frame(1, 0, true));
        queue.frames.clear();
        queue.push_frame(frame(1, 1, false));
        queue.push_frame(frame(1, 1, false));
        assert_eq!(queue.frames.len(), 1);
    }
}
