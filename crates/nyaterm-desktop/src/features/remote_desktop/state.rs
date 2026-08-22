use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use crate::models::event_wake::{ANY_INTEREST, EventWake};
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{Bounds, DynamicTexture, FocusHandle, Pixels, Subscription};
use nyaterm_remote_desktop::{
    ClipboardTracker, Framebuffer, KeyMapper, RdpCapability, RdpCertificateRequest, RdpCursorEvent,
    RdpError, RdpSessionManager, RdpSessionState, VncSessionManager,
};

pub(in crate::features) struct RemoteDesktopFeatureState {
    pub(super) manager: Arc<RdpSessionManager>,
    pub(super) vnc_manager: Arc<VncSessionManager>,
    pub(super) sessions: HashMap<String, RemoteDesktopSessionState>,
    pub(super) focus: FocusHandle,
    pub(super) last_clipboard_poll: Option<Instant>,
    pub(super) metrics_enabled: bool,
    pub(super) metrics_last_report: Instant,
    pub(super) metrics_control_events: usize,
    pub(super) metrics_frame_updates: usize,
    pub(super) metrics_dropped_frames: usize,
    pub(super) pending_texture_removals: Vec<DynamicTexture>,
    pub(super) focus_subscriptions: Vec<Subscription>,
    /// Signalled by both session managers after they enqueue. Taken once by
    /// `NyaTermApp::start_remote_desktop_event_drain`.
    wake: EventWake,
    wake_rx: Option<UnboundedReceiver<()>>,
    /// True while the periodic-maintenance clock task is alive.
    periodic_clock_armed: bool,
}

pub(super) struct RemoteDesktopSessionState {
    pub(super) state: RdpSessionState,
    pub(super) framebuffer: Option<Framebuffer>,
    pub(super) texture: Option<DynamicTexture>,
    pub(super) cursor: Option<RdpCursorEvent>,
    pub(super) cursor_texture: Option<DynamicTexture>,
    pub(super) certificate_request: Option<RdpCertificateRequest>,
    pub(super) error: Option<RdpError>,
    pub(super) capability: Option<RdpCapability>,
    pub(super) clipboard: ClipboardTracker,
    pub(super) keys: KeyMapper,
    pub(super) last_pointer: Option<(u32, u32)>,
    pub(super) vnc_button_mask: u8,
    pub(super) last_pointer_sent_at: Option<Instant>,
    pub(super) pending_pointer: Option<(u32, u32)>,
    pub(super) last_resize: Option<(u32, u32)>,
    pub(super) last_resize_sent_at: Option<Instant>,
    pub(super) pending_resize: Option<(u32, u32, Instant)>,
    pub(super) dynamic_resize_disabled: bool,
    pub(super) viewport: Option<Bounds<Pixels>>,
    pub(super) reconnect_attempts: u32,
    pub(super) reconnect_at: Option<Instant>,
}

impl Default for RemoteDesktopSessionState {
    fn default() -> Self {
        Self {
            state: RdpSessionState::Connecting,
            framebuffer: None,
            texture: None,
            cursor: None,
            cursor_texture: None,
            certificate_request: None,
            error: None,
            capability: None,
            clipboard: ClipboardTracker::default(),
            keys: KeyMapper::default(),
            last_pointer: None,
            vnc_button_mask: 0,
            last_pointer_sent_at: None,
            pending_pointer: None,
            last_resize: None,
            last_resize_sent_at: None,
            pending_resize: None,
            dynamic_resize_disabled: false,
            viewport: None,
            reconnect_attempts: 0,
            reconnect_at: None,
        }
    }
}

impl RemoteDesktopFeatureState {
    pub(in crate::features) fn new(focus: FocusHandle) -> Self {
        let (wake, wake_rx) = EventWake::new();
        let manager = Arc::new(RdpSessionManager::new());
        let vnc_manager = Arc::new(VncSessionManager::new());
        // Installed before any session exists, so every session queue gets it.
        // The closure only touches an atomic and an unbounded channel, which is
        // what `QueueWaker` requires of a callback invoked under the queue lock.
        let rdp_wake = wake.clone();
        manager.set_queue_waker(Arc::new(move || {
            rdp_wake.signal(ANY_INTEREST);
        }));
        let vnc_wake = wake.clone();
        vnc_manager.set_queue_waker(Arc::new(move || {
            vnc_wake.signal(ANY_INTEREST);
        }));
        Self {
            manager,
            vnc_manager,
            sessions: HashMap::new(),
            focus,
            last_clipboard_poll: None,
            metrics_enabled: std::env::var("NYATERM_RDP_METRICS").as_deref() == Ok("1"),
            metrics_last_report: Instant::now(),
            metrics_control_events: 0,
            metrics_frame_updates: 0,
            metrics_dropped_frames: 0,
            pending_texture_removals: Vec::new(),
            focus_subscriptions: Vec::new(),
            wake,
            wake_rx: Some(wake_rx),
            periodic_clock_armed: false,
        }
    }

    pub(in crate::features) fn take_wake_receiver(&mut self) -> Option<UnboundedReceiver<()>> {
        self.wake_rx.take()
    }

    /// Declare interest in the next enqueued event. See `models::event_wake`:
    /// this must happen before the consumer checks the queues.
    pub(in crate::features) fn arm_event_wake(&self) {
        self.wake.arm(ANY_INTEREST);
    }

    pub(in crate::features) fn has_sessions(&self) -> bool {
        !self.sessions.is_empty()
    }

    /// Whether any session is holding a coalesced pointer move that still needs
    /// sending. This is what decides the periodic clock's cadence.
    pub(in crate::features) fn pointer_flush_is_pending(&self) -> bool {
        self.sessions
            .values()
            .any(|session| session.pending_pointer.is_some())
    }

    pub(in crate::features) fn periodic_clock_is_armed(&self) -> bool {
        self.periodic_clock_armed
    }

    pub(in crate::features) fn set_periodic_clock_armed(&mut self, armed: bool) {
        self.periodic_clock_armed = armed;
    }

    pub(in crate::features) fn is_session(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }

    pub(in crate::features) fn focus(&self) -> &FocusHandle {
        &self.focus
    }

    pub(super) fn insert_connecting(&mut self, session_id: String) {
        self.sessions
            .insert(session_id, RemoteDesktopSessionState::default());
    }

    pub(in crate::features) fn insert_disconnected(&mut self, session_id: String) {
        self.insert_connecting(session_id.clone());
        if let Some(session) = self.sessions.get_mut(&session_id) {
            session.state = RdpSessionState::Disconnected;
        }
    }
}
