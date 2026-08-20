use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::{Child, ChildStdin};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use uuid::Uuid;

use crate::helper_process;
use crate::{
    PROTOCOL_VERSION, PacketType, RdpCertificateResponse, RdpControlMessage, RdpError,
    RdpErrorKind, RdpFrameEvent, RdpInputEvent, RdpRuntimeEvent, RdpSessionConfig, RdpSessionDrain,
    RdpSessionState, decode_control, decode_cursor_packet, decode_frame_packet, encode_control,
    read_packet, write_packet,
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
    control: VecDeque<RdpRuntimeEvent>,
    frames: VecDeque<RdpFrameEvent>,
    current_epoch: Option<u64>,
    waiting_for_full_frame: bool,
    dropped_frames: usize,
}

impl EventQueue {
    fn push_control(&mut self, event: RdpRuntimeEvent) {
        self.control.push_back(event);
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
    }

    fn push_frame(&mut self, frame: RdpFrameEvent) -> bool {
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
    queue: Arc<Mutex<EventQueue>>,
    writer: Arc<Mutex<ChildStdin>>,
    child: Option<Child>,
    reader: Option<JoinHandle<()>>,
}

#[derive(Default)]
pub struct RdpSessionManager {
    sessions: Mutex<HashMap<String, SessionRecord>>,
    pending_certificates: Arc<Mutex<HashMap<String, String>>>,
}

impl RdpSessionManager {
    pub fn new() -> Self {
        Self::default()
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
        let queue = Arc::new(Mutex::new(EventQueue::default()));
        let state = Arc::new(Mutex::new(RdpSessionState::Connecting));
        let reader = spawn_reader(
            session_id.clone(),
            stdout,
            writer.clone(),
            queue.clone(),
            state.clone(),
            self.pending_certificates.clone(),
        );
        let mut record = SessionRecord {
            state,
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

    pub fn send_input(&self, session_id: &str, events: Vec<RdpInputEvent>) -> Result<(), RdpError> {
        self.send(
            session_id,
            RdpControlMessage::Input {
                session_id: session_id.to_string(),
                events,
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
    pending_certificates: Arc<Mutex<HashMap<String, String>>>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name(format!("nyaterm-rdp-reader-{session_id}"))
        .spawn(move || {
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
                                &pending_certificates,
                            )
                        }),
                    PacketType::Frame => decode_frame_packet(&packet)
                        .map_err(|error| RdpError::new(RdpErrorKind::Protocol, error.to_string()))
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
                    PacketType::Cursor => decode_cursor_packet(&packet)
                        .map_err(|error| RdpError::new(RdpErrorKind::Protocol, error.to_string()))
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

fn handle_control(
    session_id: &str,
    message: RdpControlMessage,
    queue: &Arc<Mutex<EventQueue>>,
    state: &Arc<Mutex<RdpSessionState>>,
    pending: &Arc<Mutex<HashMap<String, String>>>,
) -> Result<(), RdpError> {
    match message {
        RdpControlMessage::ServerHello { version } if version != PROTOCOL_VERSION => {
            return Err(RdpError::new(
                RdpErrorKind::Protocol,
                format!("RDP helper protocol version {version} does not match {PROTOCOL_VERSION}"),
            ));
        }
        RdpControlMessage::ServerHello { .. } => {}
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
    use super::{EventQueue, FRAME_QUEUE_LIMIT};
    use crate::{PixelFormat, RdpFrameEvent};

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
