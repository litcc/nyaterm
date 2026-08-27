use std::time::{Duration, Instant};

use futures::StreamExt as _;
use gpui::{Bounds, ClipboardItem, Context, DevicePixels, Point, Size, Window, point, size};
use nyaterm_remote_desktop::{
    CertificateDecision, CertificateMatchState, CertificatePromptReason, ClipboardOrigin,
    DirtyRect, Framebuffer, RdpCapability, RdpCertificatePolicy, RdpCertificateRequest,
    RdpCertificateResponse, RdpClipboardMode, RdpDisplayMode, RdpError, RdpErrorKind,
    RdpFrameEvent, RdpInputEvent, RdpRuntimeEvent, RdpServerCapabilities, RdpSessionConfig,
    RdpSessionState, VncError, VncErrorKind, VncInputEvent, VncRuntimeEvent, VncServerCapabilities,
    VncSessionConfig, VncSessionState, evaluate_certificate_match,
};
use nyaterm_store::{RdpCertificateMetadata, RdpKnownHostCheck, StoreDomain, store_request};

use super::state::RdpCertificatePrompt;

use crate::features::NyaTermApp;

const RESIZE_DEBOUNCE: Duration = Duration::from_millis(150);
const RESIZE_FAILURE_WINDOW: Duration = Duration::from_secs(3);
const RESIZE_MIN_DELTA: u32 = 32;
const CLIPBOARD_POLL_INTERVAL: Duration = Duration::from_millis(250);
const POINTER_MOVE_INTERVAL: Duration = Duration::from_millis(8);
const METRICS_REPORT_INTERVAL: Duration = Duration::from_secs(5);
/// Cadence for remote-desktop maintenance when no pointer move is waiting.
///
/// Finer than the shortest thing it services (`RESIZE_DEBOUNCE`), so a debounce still
/// resolves promptly after the user stops; the clipboard and metrics intervals gate
/// themselves, so this only costs a cheap check for those.
const MAINTENANCE_INTERVAL: Duration = Duration::from_millis(100);

/// How long before the next remote-desktop maintenance pass.
///
/// A waiting pointer move gets `POINTER_MOVE_INTERVAL`, because that is the interval
/// its own send is budgeted against and a late flush is a visibly late cursor.
/// Everything else is happy on the coarser maintenance cadence.
fn remote_desktop_periodic_delay(pointer_flush_pending: bool) -> Duration {
    if pointer_flush_pending {
        POINTER_MOVE_INTERVAL
    } else {
        MAINTENANCE_INTERVAL
    }
}

impl NyaTermApp {
    pub(in crate::features) fn ensure_rdp_focus_reporting(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.remote_desktop.focus_subscriptions.is_empty() {
            return;
        }
        let subscription = cx.on_focus_out(
            &self.remote_desktop.focus,
            window,
            |this, _event, _window, _cx| {
                super::keyboard_capture::set_keyboard_capture(
                    this.remote_desktop.manager.clone(),
                    None,
                );
                if let Some(session_id) = this.session.active_id_owned() {
                    this.release_remote_keys(&session_id);
                }
            },
        );
        self.remote_desktop.focus_subscriptions.push(subscription);
    }

    pub(in crate::features) fn create_rdp_runtime(
        &mut self,
        config: RdpSessionConfig,
    ) -> Result<String, RdpError> {
        let session_id = self.remote_desktop.manager.create_session(config)?;
        self.remote_desktop.insert_connecting(session_id.clone());
        Ok(session_id)
    }

    pub(in crate::features) fn create_failed_rdp_runtime(&mut self, error: RdpError) -> String {
        let session_id = nyaterm_core::uuid();
        self.remote_desktop.insert_connecting(session_id.clone());
        if let Some(session) = self.remote_desktop.sessions.get_mut(&session_id) {
            set_rdp_view_error(session, error.kind, error.message);
        }
        session_id
    }

    pub(in crate::features) fn create_vnc_runtime(
        &mut self,
        config: VncSessionConfig,
    ) -> Result<String, VncError> {
        let session_id = self.remote_desktop.vnc_manager.create_session(config)?;
        self.remote_desktop.insert_connecting(session_id.clone());
        Ok(session_id)
    }

    pub(in crate::features) fn create_failed_vnc_runtime(&mut self, error: VncError) -> String {
        let session_id = nyaterm_core::uuid();
        self.remote_desktop.insert_connecting(session_id.clone());
        if let Some(session) = self.remote_desktop.sessions.get_mut(&session_id) {
            set_rdp_view_error(session, vnc_error_as_rdp_kind(error.kind), error.message);
        }
        session_id
    }

    pub(in crate::features) fn retry_rdp_runtime(
        &mut self,
        session_id: &str,
        cx: &mut Context<Self>,
    ) {
        if self.session.metadata(session_id).is_some_and(|metadata| {
            matches!(
                metadata.launch_config,
                crate::models::SessionLaunchConfig::Vnc(_)
            )
        }) {
            self.restart_vnc_runtime(session_id, true, cx);
        } else {
            self.restart_rdp_runtime(session_id, true, cx);
        }
    }

    fn restart_rdp_runtime(
        &mut self,
        session_id: &str,
        reset_attempts: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(metadata) = self.session.metadata(session_id).cloned() else {
            return;
        };
        let mut config = match metadata.launch_config {
            crate::models::SessionLaunchConfig::Rdp(config) => config,
            _ => return,
        };
        if config.password.is_none()
            && let Some(connection_id) = metadata.source_connection_id.as_deref()
            && let Some(connection) = self
                .connection_state
                .connections()
                .iter()
                .find(|connection| connection.id == connection_id)
        {
            config.password = inline_remote_desktop_password(connection.auth.as_ref());
            if config.password.is_none()
                && let Some(password_id) = remote_desktop_password_id(connection.auth.as_ref())
            {
                self.request_remote_desktop_restart_password(
                    session_id.to_string(),
                    password_id,
                    reset_attempts,
                    false,
                    cx,
                );
                return;
            }
        }
        let reconnect_attempts = if reset_attempts {
            0
        } else {
            self.remote_desktop
                .sessions
                .get(session_id)
                .map_or(0, |session| session.reconnect_attempts)
        };
        let dynamic_resize_disabled = self
            .remote_desktop
            .sessions
            .get(session_id)
            .is_some_and(|session| session.dynamic_resize_disabled);
        let _ = self.close_rdp_runtime(session_id);
        match self
            .remote_desktop
            .manager
            .create_session_with_id(session_id.to_string(), config)
        {
            Ok(_) => {
                self.remote_desktop
                    .insert_connecting(session_id.to_string());
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    session.reconnect_attempts = reconnect_attempts;
                    session.dynamic_resize_disabled = dynamic_resize_disabled;
                }
                if let Some(metadata) = self.session.metadata_mut(session_id) {
                    metadata.disconnected = false;
                }
                self.shell.set_status("RDP reconnecting".to_string());
            }
            Err(error) => {
                self.remote_desktop
                    .insert_connecting(session_id.to_string());
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    set_rdp_view_error(session, error.kind, error.message);
                }
            }
        }
    }

    fn restart_vnc_runtime(
        &mut self,
        session_id: &str,
        reset_attempts: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(metadata) = self.session.metadata(session_id).cloned() else {
            return;
        };
        let mut config = match metadata.launch_config {
            crate::models::SessionLaunchConfig::Vnc(config) => config,
            _ => return,
        };
        if config.password.is_none()
            && let Some(connection_id) = metadata.source_connection_id.as_deref()
            && let Some(connection) = self
                .connection_state
                .connections()
                .iter()
                .find(|connection| connection.id == connection_id)
        {
            config.password = inline_remote_desktop_password(connection.auth.as_ref());
            if config.password.is_none()
                && let Some(password_id) = remote_desktop_password_id(connection.auth.as_ref())
            {
                self.request_remote_desktop_restart_password(
                    session_id.to_string(),
                    password_id,
                    reset_attempts,
                    true,
                    cx,
                );
                return;
            }
        }
        let reconnect_attempts = if reset_attempts {
            0
        } else {
            self.remote_desktop
                .sessions
                .get(session_id)
                .map_or(0, |session| session.reconnect_attempts)
        };
        let _ = self.close_vnc_runtime(session_id);
        match self
            .remote_desktop
            .vnc_manager
            .create_session_with_id(session_id.to_string(), config)
        {
            Ok(_) => {
                self.remote_desktop
                    .insert_connecting(session_id.to_string());
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    session.reconnect_attempts = reconnect_attempts;
                }
                if let Some(metadata) = self.session.metadata_mut(session_id) {
                    metadata.disconnected = false;
                }
                self.shell.set_status("VNC reconnecting".to_string());
            }
            Err(error) => {
                self.remote_desktop
                    .insert_connecting(session_id.to_string());
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    set_rdp_view_error(session, vnc_error_as_rdp_kind(error.kind), error.message);
                }
            }
        }
    }

    fn request_remote_desktop_restart_password(
        &mut self,
        session_id: String,
        password_id: String,
        reset_attempts: bool,
        vnc: bool,
        cx: &mut Context<Self>,
    ) {
        let response_session_id = session_id.clone();
        self.submit_store_request(
            0,
            store_request(StoreDomain::Security, move |store| {
                store.load_decrypted_password_by_id(&password_id)
            }),
            move |this, event, cx| {
                let password = match event.outcome {
                    Ok(Some(entry)) => entry
                        .password
                        .filter(|password| !password.trim().is_empty()),
                    Ok(None) => None,
                    Err(error) => {
                        this.shell.set_status(format!(
                            "remote desktop reconnect could not load saved password: {error}"
                        ));
                        cx.notify();
                        return;
                    }
                };
                let Some(password) = password else {
                    this.shell.set_status(
                        "remote desktop reconnect saved password is missing or locked".to_string(),
                    );
                    cx.notify();
                    return;
                };
                let Some(metadata) = this.session.metadata_mut(&response_session_id) else {
                    return;
                };
                match &mut metadata.launch_config {
                    crate::models::SessionLaunchConfig::Rdp(config) if !vnc => {
                        config.password = Some(password);
                    }
                    crate::models::SessionLaunchConfig::Vnc(config) if vnc => {
                        config.password = Some(password);
                    }
                    _ => return,
                }
                if vnc {
                    this.restart_vnc_runtime(&response_session_id, reset_attempts, cx);
                } else {
                    this.restart_rdp_runtime(&response_session_id, reset_attempts, cx);
                }
                cx.notify();
            },
            cx,
        );
    }

    pub(in crate::features) fn close_remote_desktop_runtime(
        &mut self,
        session_id: &str,
    ) -> anyhow::Result<()> {
        match self
            .session
            .metadata(session_id)
            .map(|metadata| &metadata.launch_config)
        {
            Some(crate::models::SessionLaunchConfig::Vnc(_)) => self
                .close_vnc_runtime(session_id)
                .map_err(anyhow::Error::from),
            _ => self
                .close_rdp_runtime(session_id)
                .map_err(anyhow::Error::from),
        }
    }

    pub(in crate::features) fn close_rdp_runtime(
        &mut self,
        session_id: &str,
    ) -> Result<(), RdpError> {
        self.remote_desktop.input.clear_session(session_id);
        if self.session.active_id() == Some(session_id) {
            super::keyboard_capture::set_keyboard_capture(
                self.remote_desktop.manager.clone(),
                None,
            );
            self.release_remote_keys(session_id);
        }
        if let Some(mut session) = self.remote_desktop.sessions.remove(session_id) {
            if let Some(texture) = session.texture.take() {
                self.remote_desktop.pending_texture_removals.push(texture);
            }
            if let Some(texture) = session.cursor_texture.take() {
                self.remote_desktop.pending_texture_removals.push(texture);
            }
        }
        self.remote_desktop.manager.close(session_id)
    }

    pub(in crate::features) fn close_vnc_runtime(
        &mut self,
        session_id: &str,
    ) -> Result<(), VncError> {
        self.remote_desktop.input.clear_session(session_id);
        if self.session.active_id() == Some(session_id) {
            self.release_remote_keys(session_id);
        }
        if let Some(mut session) = self.remote_desktop.sessions.remove(session_id) {
            if let Some(texture) = session.texture.take() {
                self.remote_desktop.pending_texture_removals.push(texture);
            }
            if let Some(texture) = session.cursor_texture.take() {
                self.remote_desktop.pending_texture_removals.push(texture);
            }
        }
        self.remote_desktop.vnc_manager.close(session_id)
    }

    pub(in crate::features) fn release_remote_keys(&mut self, session_id: &str) {
        self.remote_desktop.input.clear_session(session_id);
        if self.session.metadata(session_id).is_some_and(|metadata| {
            matches!(
                metadata.launch_config,
                crate::models::SessionLaunchConfig::Vnc(_)
            )
        }) {
            if self.vnc_input_enabled(session_id) {
                let _ = self
                    .remote_desktop
                    .vnc_manager
                    .send_input(session_id, vec![VncInputEvent::ReleaseAllKeys]);
            }
            return;
        }
        let Some(session) = self.remote_desktop.sessions.get_mut(session_id) else {
            return;
        };
        if let Some(event) = session.keys.release_all() {
            let _ = self
                .remote_desktop
                .manager
                .send_input(session_id, vec![event]);
        }
    }

    /// Deliver RDP and VNC session events as the helper processes produce them.
    ///
    /// Started once at window open. Before this the runtime tick polled every
    /// session queue, which capped remote-desktop framerate at the tick cadence:
    /// `has_protocol_runtime_sessions()` keeps the tick off the 500ms quiet
    /// interval, but that still left 50ms idle / 16ms under pressure, so a helper
    /// delivering 60fps was sampled at 20-60fps.
    ///
    /// The session queues keep only the newest frame, so they stay queues and
    /// only the signal is a channel; see `models::event_wake`. `update_in` rather
    /// than `update` because applying a frame needs the `Window` for its dynamic
    /// texture.
    pub(in crate::features) fn start_remote_desktop_event_drain(&mut self, cx: &mut Context<Self>) {
        let Some(mut wake_rx) = self.remote_desktop.take_wake_receiver() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            loop {
                // Arm before draining, so a frame enqueued in between still
                // signals rather than waiting for the next one.
                let drained = this.update_in(cx, |this, window, cx| {
                    this.remote_desktop.arm_event_wake();
                    // Any event means a session exists; the periodic clock is scoped
                    // to that, and every reconnect is scheduled from an event handler.
                    this.ensure_remote_desktop_periodic_clock(cx);
                    let dirty = this.drain_remote_desktop_queues(window, cx);
                    if dirty {
                        cx.notify();
                    }
                    dirty
                });
                match drained {
                    Err(_) => break,
                    // A frame can arrive while the previous one is being applied.
                    Ok(true) => continue,
                    Ok(false) => {}
                }
                if wake_rx.next().await.is_none() {
                    break;
                }
            }
        })
        .detach();
    }

    /// The queue half: everything the helper processes push.
    fn drain_remote_desktop_queues(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        for texture in self.remote_desktop.pending_texture_removals.drain(..) {
            window.remove_dynamic_texture(texture);
        }
        let ids = self
            .remote_desktop
            .sessions
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let mut dirty = false;
        for session_id in ids {
            let drain = self.remote_desktop.manager.drain(&session_id);
            let vnc_drain = self.remote_desktop.vnc_manager.drain(&session_id);
            if drain.control.is_empty()
                && drain.frames.is_empty()
                && vnc_drain.control.is_empty()
                && vnc_drain.frames.is_empty()
            {
                continue;
            }
            if self.remote_desktop.metrics_enabled {
                self.remote_desktop.metrics_control_events += drain.control.len();
                self.remote_desktop.metrics_frame_updates += drain.frames.len();
                self.remote_desktop.metrics_dropped_frames += drain.dropped_frames;
                self.remote_desktop.metrics_control_events += vnc_drain.control.len();
                self.remote_desktop.metrics_frame_updates += vnc_drain.frames.len();
                self.remote_desktop.metrics_dropped_frames += vnc_drain.dropped_frames;
            }
            dirty = true;
            for event in drain.control {
                self.apply_rdp_control_event(&session_id, event, window, cx);
            }
            self.apply_rdp_frame_batch(&session_id, drain.frames, window);
            for event in vnc_drain.control {
                self.apply_vnc_control_event(&session_id, event, window, cx);
            }
            self.apply_rdp_frame_batch(&session_id, vnc_drain.frames, window);
        }
        dirty
    }

    /// The time-based half, still driven by the runtime tick.
    ///
    /// None of these is a queue read: a pointer batch flushes after a hold, a
    /// resize is debounced, the reconnect ladder waits out a backoff, the
    /// clipboard is polled on an interval, and metrics report on one. Giving each
    /// its own timer is Phase 2 of the runtime-tick plan.
    /// Drive remote-desktop maintenance on its own cadence while a session exists.
    ///
    /// These six are all genuinely time-based -- a coalesced pointer move flushes after
    /// a hold, a resize is debounced, the reconnect ladder waits out a backoff, and
    /// the clipboard and metrics report on intervals -- so this stays a poll. What was
    /// wrong was *whose* cadence it used: `runtime_quiet_tick_allowed` has no
    /// remote-desktop term, so an otherwise-idle app with a live RDP session ran this
    /// at the 500ms quiet interval, and the trailing pointer move of a gesture --
    /// budgeted at `POINTER_MOVE_INTERVAL`, 8ms -- landed up to half a second late.
    ///
    /// Armed from the remote-desktop event drain. Every reconnect is scheduled by an
    /// event handler, and a connecting session always reports at least one state
    /// change, so an event is a reliable point to start from.
    pub(in crate::features) fn ensure_remote_desktop_periodic_clock(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.remote_desktop.periodic_clock_is_armed() || !self.remote_desktop.has_sessions() {
            return;
        }
        self.remote_desktop.set_periodic_clock_armed(true);
        cx.spawn(async move |this, cx| {
            loop {
                let Ok(delay) = this.update(cx, |this, _| {
                    remote_desktop_periodic_delay(this.remote_desktop.pointer_flush_is_pending())
                }) else {
                    break;
                };
                cx.background_executor().timer(delay).await;
                // `update_in`: keyboard-capture sync needs the window.
                let Ok(keep_running) = this.update_in(cx, |this, window, cx| {
                    if this.drive_remote_desktop_periodic(window, cx) {
                        cx.notify();
                    }
                    let running = this.remote_desktop.has_sessions();
                    if !running {
                        this.remote_desktop.set_periodic_clock_armed(false);
                    }
                    running
                }) else {
                    break;
                };
                if !keep_running {
                    break;
                }
            }
        })
        .detach();
    }

    pub(in crate::features) fn drive_remote_desktop_periodic(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut dirty = self.drive_rdp_pointer_flush();
        dirty |= self.drive_rdp_resize_debounce();
        dirty |= self.drive_rdp_reconnects(cx);
        self.sync_rdp_keyboard_capture(window);
        dirty |= self.poll_active_rdp_clipboard(cx);
        self.report_rdp_metrics();
        dirty
    }

    fn apply_vnc_control_event(
        &mut self,
        session_id: &str,
        event: VncRuntimeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            VncRuntimeEvent::State { state, message, .. } => {
                let vnc_server_capabilities = vnc_capabilities_for_state(
                    &state,
                    self.remote_desktop
                        .vnc_manager
                        .server_capabilities(session_id),
                );
                let state = match state {
                    VncSessionState::Connecting
                    | VncSessionState::Authenticating
                    | VncSessionState::Negotiating => RdpSessionState::Connecting,
                    VncSessionState::Connected => RdpSessionState::Connected,
                    VncSessionState::Reconnecting => RdpSessionState::Reconnecting,
                    VncSessionState::Disconnecting => RdpSessionState::Disconnecting,
                    VncSessionState::Disconnected => RdpSessionState::Disconnected,
                    VncSessionState::Failed => RdpSessionState::Failed(RdpError::new(
                        RdpErrorKind::Session,
                        message
                            .clone()
                            .unwrap_or_else(|| "VNC session failed".to_string()),
                    )),
                };
                if remote_state_clears_input(&state) {
                    self.remote_desktop.input.clear_session(session_id);
                }
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    session.vnc_server_capabilities = vnc_server_capabilities;
                    session.state = state;
                }
                if let Some(message) = message {
                    self.shell.set_status(message);
                }
            }
            VncRuntimeEvent::Frame {
                event:
                    RdpFrameEvent::Reset {
                        epoch,
                        width,
                        height,
                    },
                ..
            } => {
                self.reset_rdp_framebuffer(session_id, epoch, width, height, window);
            }
            VncRuntimeEvent::Frame { .. } => {}
            VncRuntimeEvent::Clipboard { text, .. } => {
                if self.session.active_id() == Some(session_id) {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            VncRuntimeEvent::Error { error, .. } => {
                self.remote_desktop.input.clear_session(session_id);
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    session.vnc_server_capabilities = None;
                    set_rdp_view_error(
                        session,
                        vnc_error_as_rdp_kind(error.kind),
                        format!("VNC connection failed: {error}"),
                    );
                }
            }
        }
    }

    fn sync_rdp_keyboard_capture(&self, window: &Window) {
        let target = self.session.active_id().and_then(|session_id| {
            (self.remote_desktop.focus.is_focused(window)
                && self
                    .remote_desktop
                    .sessions
                    .get(session_id)
                    .is_some_and(|session| matches!(session.state, RdpSessionState::Connected)))
            .then(|| session_id.to_string())
        });
        super::keyboard_capture::set_keyboard_capture(self.remote_desktop.manager.clone(), target);
    }

    pub(in crate::features) fn update_rdp_viewport(
        &mut self,
        session_id: &str,
        bounds: Bounds<gpui::Pixels>,
    ) {
        let fit_window = self.session.metadata(session_id).is_some_and(|metadata| {
            matches!(
                &metadata.launch_config,
                crate::models::SessionLaunchConfig::Rdp(config)
                    if config.display.mode == RdpDisplayMode::FitWindow
            )
        });
        let Some(session) = self.remote_desktop.sessions.get_mut(session_id) else {
            return;
        };
        session.viewport = Some(bounds);
        if !fit_window || session.dynamic_resize_disabled {
            session.pending_resize = None;
            return;
        }
        self.queue_rdp_resize(
            session_id,
            f32::from(bounds.size.width).round().max(1.0) as u32,
            f32::from(bounds.size.height).round().max(1.0) as u32,
        );
    }

    pub(in crate::features) fn focused_remote_desktop_session_id(
        &self,
        window: &Window,
    ) -> Option<String> {
        self.remote_desktop
            .focus
            .is_focused(window)
            .then(|| self.session.active_id_owned())
            .flatten()
            .filter(|session_id| self.remote_desktop.is_session(session_id))
    }

    pub(in crate::features) fn remote_marked_text(&self, session_id: &str) -> String {
        self.remote_desktop
            .input
            .marked_text(session_id)
            .to_string()
    }

    pub(in crate::features) fn set_remote_marked_text(&mut self, session_id: &str, text: &str) {
        self.remote_desktop.input.set_marked_text(session_id, text);
    }

    pub(in crate::features) fn clear_remote_marked_text(&mut self, session_id: &str) {
        self.remote_desktop.input.clear_marked_text(session_id);
    }

    pub(in crate::features) fn send_remote_committed_text(
        &mut self,
        session_id: &str,
        text: &str,
    ) -> bool {
        self.remote_desktop.input.clear_marked_text(session_id);
        if text.is_empty() {
            return true;
        }
        if !self
            .remote_desktop
            .sessions
            .get(session_id)
            .is_some_and(|session| matches!(session.state, RdpSessionState::Connected))
        {
            self.shell.set_status(
                "Remote desktop text input is unavailable while disconnected".to_string(),
            );
            return false;
        }
        let is_vnc = self.session.metadata(session_id).is_some_and(|metadata| {
            matches!(
                metadata.launch_config,
                crate::models::SessionLaunchConfig::Vnc(_)
            )
        });
        let committed_text_supported =
            self.remote_desktop
                .sessions
                .get(session_id)
                .is_some_and(|session| {
                    remote_committed_text_supported(
                        is_vnc,
                        session.server_capabilities,
                        session.vnc_server_capabilities,
                    )
                });
        if !committed_text_supported {
            return false;
        }
        let result = if is_vnc {
            if !self.vnc_input_enabled(session_id) {
                self.shell
                    .set_status("VNC view-only mode does not accept input".to_string());
                return false;
            }
            self.remote_desktop
                .vnc_manager
                .send_input(
                    session_id,
                    vec![VncInputEvent::Text {
                        text: text.to_string(),
                    }],
                )
                .map_err(|error| error.to_string())
        } else {
            self.remote_desktop
                .manager
                .send_input(
                    session_id,
                    vec![RdpInputEvent::Unicode {
                        text: text.to_string(),
                    }],
                )
                .map_err(|error| format_rdp_error(&error))
        };
        match result {
            Ok(()) => true,
            Err(error) => {
                self.shell.set_status(error);
                false
            }
        }
    }

    pub(in crate::features) fn send_rdp_key_down(
        &mut self,
        session_id: &str,
        key: &str,
        key_char: Option<&str>,
        repeat: bool,
        control: bool,
        alt: bool,
        platform: bool,
    ) -> bool {
        if !self
            .remote_desktop
            .sessions
            .get(session_id)
            .is_some_and(|session| matches!(session.state, RdpSessionState::Connected))
        {
            return false;
        }
        let is_vnc = self.session.metadata(session_id).is_some_and(|metadata| {
            matches!(
                metadata.launch_config,
                crate::models::SessionLaunchConfig::Vnc(_)
            )
        });
        let committed_text_supported =
            self.remote_desktop
                .sessions
                .get(session_id)
                .is_some_and(|session| {
                    remote_committed_text_supported(
                        is_vnc,
                        session.server_capabilities,
                        session.vnc_server_capabilities,
                    )
                });
        let classified_for_text = remote_key_down_should_defer_to_text(
            key_char,
            committed_text_supported,
            RemoteInputPlatform::current(),
            RemoteKeyModifiers {
                control,
                alt,
                platform,
            },
        );
        let key_already_uses_text = self
            .remote_desktop
            .input
            .is_key_up_suppressed(session_id, key);
        if remote_key_down_uses_text_route(repeat, key_already_uses_text, classified_for_text) {
            if is_vnc && !self.vnc_input_enabled(session_id) {
                return false;
            }
            self.remote_desktop.input.suppress_key_up(session_id, key);
            return true;
        }
        if is_vnc {
            return self.send_vnc_key(session_id, key, key_char, true);
        }
        let Some(event) = self
            .remote_desktop
            .sessions
            .get_mut(session_id)
            .and_then(|session| session.keys.key_down(key, repeat))
        else {
            return false;
        };
        self.remote_desktop
            .manager
            .send_input(session_id, vec![event])
            .is_ok()
    }

    pub(in crate::features) fn send_rdp_key_up(&mut self, session_id: &str, key: &str) -> bool {
        if self
            .remote_desktop
            .input
            .take_suppressed_key_up(session_id, key)
        {
            return true;
        }
        if self.session.metadata(session_id).is_some_and(|metadata| {
            matches!(
                metadata.launch_config,
                crate::models::SessionLaunchConfig::Vnc(_)
            )
        }) {
            return self.send_vnc_key(session_id, key, None, false);
        }
        let Some(event) = self
            .remote_desktop
            .sessions
            .get_mut(session_id)
            .and_then(|session| session.keys.key_up(key))
        else {
            return false;
        };
        self.remote_desktop
            .manager
            .send_input(session_id, vec![event])
            .is_ok()
    }

    pub(in crate::features) fn rdp_secure_attention_available(&self, session_id: &str) -> bool {
        let is_rdp = self.session.metadata(session_id).is_some_and(|metadata| {
            matches!(
                metadata.launch_config,
                crate::models::SessionLaunchConfig::Rdp(_)
            )
        });
        self.remote_desktop
            .sessions
            .get(session_id)
            .is_some_and(|session| {
                secure_attention_available(is_rdp, &session.state, session.server_capabilities)
            })
    }

    pub(in crate::features) fn send_rdp_secure_attention(&mut self, session_id: &str) -> bool {
        if !self.rdp_secure_attention_available(session_id) {
            return false;
        }
        match self
            .remote_desktop
            .manager
            .send_secure_attention(session_id)
        {
            Ok(()) => {
                self.shell
                    .set_status("RDP Secure Attention sent".to_string());
                true
            }
            Err(error) => {
                self.shell.set_status(format_rdp_error(&error));
                false
            }
        }
    }

    pub(in crate::features) fn send_rdp_pointer(
        &mut self,
        session_id: &str,
        position: gpui::Point<gpui::Pixels>,
        button: Option<nyaterm_remote_desktop::RdpPointerButton>,
        pressed: bool,
    ) -> bool {
        if self.session.metadata(session_id).is_some_and(|metadata| {
            matches!(
                metadata.launch_config,
                crate::models::SessionLaunchConfig::Vnc(_)
            )
        }) {
            return self.send_vnc_pointer(session_id, position, button, pressed);
        }
        let Some(session) = self.remote_desktop.sessions.get_mut(session_id) else {
            return false;
        };
        let (Some(viewport), Some(framebuffer)) = (session.viewport, session.framebuffer.as_ref())
        else {
            return false;
        };
        let x = f32::from(position.x - viewport.origin.x);
        let y = f32::from(position.y - viewport.origin.y);
        let Some(remote) = nyaterm_remote_desktop::viewport_to_remote(
            x,
            y,
            f32::from(viewport.size.width),
            f32::from(viewport.size.height),
            framebuffer.width(),
            framebuffer.height(),
        ) else {
            return false;
        };
        let now = Instant::now();
        if button.is_none() {
            if session.last_pointer == Some(remote) {
                return false;
            }
            session.last_pointer = Some(remote);
            if session.last_pointer_sent_at.is_some_and(|sent_at| {
                now.saturating_duration_since(sent_at) < POINTER_MOVE_INTERVAL
            }) {
                session.pending_pointer = Some(remote);
                return true;
            }
        }
        session.last_pointer = Some(remote);
        session.pending_pointer = None;
        session.last_pointer_sent_at = Some(now);
        self.remote_desktop
            .manager
            .send_input(
                session_id,
                vec![RdpInputEvent::Pointer {
                    x: remote.0,
                    y: remote.1,
                    button,
                    pressed,
                }],
            )
            .is_ok()
    }

    fn send_vnc_key(
        &mut self,
        session_id: &str,
        key: &str,
        key_char: Option<&str>,
        pressed: bool,
    ) -> bool {
        if !self.vnc_input_enabled(session_id) {
            return false;
        }
        let Some(keysym) = vnc_keysym_for_key(key, key_char) else {
            return false;
        };
        self.remote_desktop
            .vnc_manager
            .send_input(session_id, vec![VncInputEvent::Key { keysym, pressed }])
            .is_ok()
    }

    fn send_vnc_pointer(
        &mut self,
        session_id: &str,
        position: gpui::Point<gpui::Pixels>,
        button: Option<nyaterm_remote_desktop::RdpPointerButton>,
        pressed: bool,
    ) -> bool {
        if !self.vnc_input_enabled(session_id) {
            return false;
        }
        let Some(session) = self.remote_desktop.sessions.get_mut(session_id) else {
            return false;
        };
        let (Some(viewport), Some(framebuffer)) = (session.viewport, session.framebuffer.as_ref())
        else {
            return false;
        };
        let x = f32::from(position.x - viewport.origin.x);
        let y = f32::from(position.y - viewport.origin.y);
        let Some(remote) = nyaterm_remote_desktop::viewport_to_remote(
            x,
            y,
            f32::from(viewport.size.width),
            f32::from(viewport.size.height),
            framebuffer.width(),
            framebuffer.height(),
        ) else {
            return false;
        };
        let mut button_mask = session.vnc_button_mask;
        match button {
            Some(nyaterm_remote_desktop::RdpPointerButton::Left) => {
                set_button_mask(&mut button_mask, 0x01, pressed);
            }
            Some(nyaterm_remote_desktop::RdpPointerButton::Middle) => {
                set_button_mask(&mut button_mask, 0x02, pressed);
            }
            Some(nyaterm_remote_desktop::RdpPointerButton::Right) => {
                set_button_mask(&mut button_mask, 0x04, pressed);
            }
            Some(nyaterm_remote_desktop::RdpPointerButton::WheelUp) => {
                return self.send_vnc_pointer_wheel(session_id, remote, 0x08);
            }
            Some(nyaterm_remote_desktop::RdpPointerButton::WheelDown) => {
                return self.send_vnc_pointer_wheel(session_id, remote, 0x10);
            }
            None => {}
        }
        if button.is_none() && session.last_pointer == Some(remote) {
            return false;
        }
        session.last_pointer = Some(remote);
        session.vnc_button_mask = button_mask;
        self.remote_desktop
            .vnc_manager
            .send_input(
                session_id,
                vec![VncInputEvent::Pointer {
                    x: remote.0,
                    y: remote.1,
                    button_mask,
                }],
            )
            .is_ok()
    }

    fn send_vnc_pointer_wheel(
        &mut self,
        session_id: &str,
        remote: (u32, u32),
        wheel_mask: u8,
    ) -> bool {
        let Some(session) = self.remote_desktop.sessions.get(session_id) else {
            return false;
        };
        let current = session.vnc_button_mask;
        self.remote_desktop
            .vnc_manager
            .send_input(
                session_id,
                vec![
                    VncInputEvent::Pointer {
                        x: remote.0,
                        y: remote.1,
                        button_mask: current | wheel_mask,
                    },
                    VncInputEvent::Pointer {
                        x: remote.0,
                        y: remote.1,
                        button_mask: current,
                    },
                ],
            )
            .is_ok()
    }

    fn vnc_input_enabled(&self, session_id: &str) -> bool {
        self.session.metadata(session_id).is_some_and(|metadata| {
            matches!(
                &metadata.launch_config,
                crate::models::SessionLaunchConfig::Vnc(config) if vnc_input_allowed(config.view_only)
            )
        })
    }

    fn drive_rdp_pointer_flush(&mut self) -> bool {
        let now = Instant::now();
        let mut sent = false;
        for (session_id, session) in &mut self.remote_desktop.sessions {
            let Some(pointer) = session.pending_pointer else {
                continue;
            };
            if session.last_pointer_sent_at.is_some_and(|sent_at| {
                now.saturating_duration_since(sent_at) < POINTER_MOVE_INTERVAL
            }) {
                continue;
            }
            session.pending_pointer = None;
            session.last_pointer_sent_at = Some(now);
            sent |= self
                .remote_desktop
                .manager
                .send_input(
                    session_id,
                    vec![RdpInputEvent::Pointer {
                        x: pointer.0,
                        y: pointer.1,
                        button: None,
                        pressed: false,
                    }],
                )
                .is_ok();
        }
        sent
    }

    fn report_rdp_metrics(&mut self) {
        if !self.remote_desktop.metrics_enabled {
            return;
        }
        let now = Instant::now();
        if now.saturating_duration_since(self.remote_desktop.metrics_last_report)
            < METRICS_REPORT_INTERVAL
        {
            return;
        }
        tracing::debug!(
            active_sessions = self.remote_desktop.sessions.len(),
            control_events = self.remote_desktop.metrics_control_events,
            frame_updates = self.remote_desktop.metrics_frame_updates,
            dropped_frames = self.remote_desktop.metrics_dropped_frames,
            "RDP runtime metrics"
        );
        self.remote_desktop.metrics_last_report = now;
        self.remote_desktop.metrics_control_events = 0;
        self.remote_desktop.metrics_frame_updates = 0;
        self.remote_desktop.metrics_dropped_frames = 0;
    }

    fn apply_rdp_control_event(
        &mut self,
        session_id: &str,
        event: RdpRuntimeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            RdpRuntimeEvent::State { state, message, .. } => {
                let server_capabilities = matches!(state, RdpSessionState::Connected)
                    .then(|| self.remote_desktop.manager.server_capabilities(session_id))
                    .flatten();
                if remote_state_clears_input(&state) {
                    self.remote_desktop.input.clear_session(session_id);
                }
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    if should_disable_dynamic_resize_after_state(
                        &state,
                        session.last_resize_sent_at,
                        Instant::now(),
                    ) {
                        session.dynamic_resize_disabled = true;
                        session.pending_resize = None;
                    }
                    if let RdpSessionState::Failed(error) = &state {
                        session.error = Some(error.clone());
                    }
                    session.server_capabilities = server_capabilities;
                    session.state = state;
                }
                if let Some(message) = message {
                    self.shell.set_status(message);
                }
            }
            RdpRuntimeEvent::Frame {
                event:
                    RdpFrameEvent::Reset {
                        epoch,
                        width,
                        height,
                    },
                ..
            } => {
                self.reset_rdp_framebuffer(session_id, epoch, width, height, window);
            }
            RdpRuntimeEvent::Frame { .. } => {}
            RdpRuntimeEvent::Clipboard { text, .. } => {
                if self.session.active_id() != Some(session_id) {
                    return;
                }
                let accepted = self
                    .remote_desktop
                    .sessions
                    .get_mut(session_id)
                    .and_then(|session| {
                        session
                            .clipboard
                            .accept(ClipboardOrigin::Remote, &text)
                            .ok()
                            .flatten()
                    })
                    .is_some();
                if accepted {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            RdpRuntimeEvent::CertificateRequest(request) => {
                self.handle_rdp_certificate_request(session_id, request, cx);
            }
            RdpRuntimeEvent::Capability { capability, .. } => {
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    session.capability = Some(capability);
                }
                if capability == RdpCapability::DynamicResizeUnavailable {
                    self.shell
                        .set_status("RDP server does not support dynamic resize".to_string());
                }
            }
            RdpRuntimeEvent::Error { error, fatal, .. } => {
                if fatal {
                    self.remote_desktop.input.clear_session(session_id);
                }
                let should_reconnect = fatal && self.schedule_rdp_reconnect(session_id, &error);
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    session.error = Some(error.clone());
                    if fatal {
                        session.server_capabilities = None;
                    }
                    if fatal && !should_reconnect {
                        session.state = RdpSessionState::Failed(error.clone());
                    }
                }
                if !should_reconnect {
                    self.shell.set_status(format_rdp_error(&error));
                }
            }
        }
    }

    fn schedule_rdp_reconnect(&mut self, session_id: &str, error: &RdpError) -> bool {
        if !rdp_error_is_retryable(error.kind) {
            return false;
        }
        let Some(config) =
            self.session
                .metadata(session_id)
                .and_then(|metadata| match &metadata.launch_config {
                    crate::models::SessionLaunchConfig::Rdp(config) => Some(&config.reconnect),
                    _ => None,
                })
        else {
            return false;
        };
        if !config.enabled {
            return false;
        }
        let Some(session) = self.remote_desktop.sessions.get_mut(session_id) else {
            return false;
        };
        if session.reconnect_attempts >= config.max_attempts {
            return false;
        }
        session.reconnect_attempts += 1;
        let delay = rdp_reconnect_delay(session.reconnect_attempts, rand::random_range(0..250));
        session.reconnect_at = Some(Instant::now() + delay);
        session.state = RdpSessionState::Reconnecting;
        self.shell.set_status(format!(
            "RDP reconnecting in {:.1}s (attempt {}/{})",
            delay.as_secs_f32(),
            session.reconnect_attempts,
            config.max_attempts
        ));
        true
    }

    fn drive_rdp_reconnects(&mut self, cx: &mut Context<Self>) -> bool {
        let now = Instant::now();
        let due = self
            .remote_desktop
            .sessions
            .iter()
            .filter(|(_, session)| session.reconnect_at.is_some_and(|deadline| now >= deadline))
            .map(|(session_id, _)| session_id.clone())
            .collect::<Vec<_>>();
        for session_id in &due {
            if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                session.reconnect_at = None;
            }
            self.restart_rdp_runtime(session_id, false, cx);
        }
        !due.is_empty()
    }

    fn reset_rdp_framebuffer(
        &mut self,
        session_id: &str,
        epoch: u64,
        width: u32,
        height: u32,
        window: &mut Window,
    ) {
        let Some(session) = self.remote_desktop.sessions.get_mut(session_id) else {
            return;
        };
        if let Some(texture) = session.texture.take() {
            window.remove_dynamic_texture(texture);
        }
        if let Some(texture) = session.cursor_texture.take() {
            window.remove_dynamic_texture(texture);
        }
        match Framebuffer::new(epoch, width, height) {
            Ok(framebuffer) => {
                let texture_size = size(DevicePixels(width as i32), DevicePixels(height as i32));
                match window.create_dynamic_texture(texture_size, framebuffer.pixels(), width * 4) {
                    Ok(texture) => {
                        session.framebuffer = Some(framebuffer);
                        session.texture = Some(texture);
                        session.cursor = None;
                    }
                    Err(error) => set_rdp_view_error(
                        session,
                        RdpErrorKind::Protocol,
                        format!("failed to create RDP texture: {error}"),
                    ),
                }
            }
            Err(error) => set_rdp_view_error(
                session,
                RdpErrorKind::Protocol,
                format!("invalid RDP desktop reset: {error}"),
            ),
        }
    }

    fn apply_rdp_frame_batch(
        &mut self,
        session_id: &str,
        frames: Vec<RdpFrameEvent>,
        window: &mut Window,
    ) {
        let Some(session) = self.remote_desktop.sessions.get_mut(session_id) else {
            return;
        };
        let mut dirty_rects = Vec::new();
        for frame in frames {
            if let RdpFrameEvent::Cursor(cursor) = frame {
                if session
                    .framebuffer
                    .as_ref()
                    .is_some_and(|framebuffer| framebuffer.epoch() == cursor.epoch)
                {
                    if let Some(texture) = session.cursor_texture.take() {
                        window.remove_dynamic_texture(texture);
                    }
                    if cursor.visible
                        && cursor.width > 0
                        && cursor.height > 0
                        && let Ok(texture) = window.create_dynamic_texture(
                            size(
                                DevicePixels(cursor.width as i32),
                                DevicePixels(cursor.height as i32),
                            ),
                            &cursor.pixels,
                            cursor.width * 4,
                        )
                    {
                        session.cursor_texture = Some(texture);
                    }
                    session.cursor = Some(cursor);
                }
                continue;
            }
            let Some(framebuffer) = session.framebuffer.as_mut() else {
                continue;
            };
            match framebuffer.apply(&frame) {
                Ok(Some(rect)) => dirty_rects.push(rect),
                Ok(None) => {}
                Err(nyaterm_remote_desktop::FramebufferError::StaleEpoch { .. }) => {}
                Err(error) => {
                    set_rdp_view_error(
                        session,
                        RdpErrorKind::Protocol,
                        format!("invalid RDP frame: {error}"),
                    );
                    return;
                }
            }
        }
        if !dirty_rects.is_empty() {
            clear_rdp_reconnect_after_frame(session);
        }
        let (Some(framebuffer), Some(texture)) = (session.framebuffer.as_ref(), session.texture)
        else {
            return;
        };
        let framebuffer_area = u64::from(framebuffer.width()) * u64::from(framebuffer.height());
        let dirty_area = dirty_rects
            .iter()
            .map(|rect| u64::from(rect.width) * u64::from(rect.height))
            .sum::<u64>();
        if dirty_rects.len() > 64 || dirty_area.saturating_mul(100) >= framebuffer_area * 60 {
            let bounds = Bounds::new(
                Point::new(DevicePixels(0), DevicePixels(0)),
                Size::new(
                    DevicePixels(framebuffer.width() as i32),
                    DevicePixels(framebuffer.height() as i32),
                ),
            );
            let _ = window.update_dynamic_texture(
                texture,
                bounds,
                framebuffer.pixels(),
                framebuffer.width() * 4,
            );
            return;
        }
        for rect in nyaterm_remote_desktop::merge_dirty_rects(dirty_rects) {
            let _ = upload_rdp_rect(window, texture, framebuffer, rect);
        }
    }

    fn handle_rdp_certificate_request(
        &mut self,
        session_id: &str,
        request: RdpCertificateRequest,
        cx: &mut Context<Self>,
    ) {
        let policy = self
            .session
            .metadata(session_id)
            .and_then(|metadata| match &metadata.launch_config {
                crate::models::SessionLaunchConfig::Rdp(config) => Some(config.certificate_policy),
                _ => None,
            })
            .unwrap_or(RdpCertificatePolicy::Prompt);
        if policy == RdpCertificatePolicy::Insecure {
            self.apply_rdp_certificate_check(
                session_id,
                request,
                policy,
                RdpKnownHostCheck::UnknownHost,
                cx,
            );
            return;
        }
        let host = request.host.clone();
        let port = request.port;
        let fingerprint = request.sha256_fingerprint.clone();
        let request_id = request.request_id.clone();
        let failure_request_id = request_id.clone();
        let session_id = session_id.to_string();
        let submitted = self.submit_store_request(
            0,
            store_request(StoreDomain::Security, move |store| {
                store.check_rdp_known_host(&host, port, &fingerprint)
            }),
            move |this, event, cx| match event.outcome {
                Ok(check) if this.remote_desktop.sessions.contains_key(&session_id) => {
                    this.apply_rdp_certificate_check(&session_id, request, policy, check, cx);
                }
                Ok(_) => {}
                Err(error) => {
                    this.shell
                        .set_status(format!("RDP certificate verification failed: {error}"));
                    let _ = this
                        .remote_desktop
                        .manager
                        .respond_certificate(&request_id, RdpCertificateResponse::Reject);
                    cx.notify();
                }
            },
            cx,
        );
        if !submitted {
            let _ = self
                .remote_desktop
                .manager
                .respond_certificate(&failure_request_id, RdpCertificateResponse::Reject);
        }
    }

    fn apply_rdp_certificate_check(
        &mut self,
        session_id: &str,
        request: RdpCertificateRequest,
        policy: RdpCertificatePolicy,
        check: RdpKnownHostCheck,
        cx: &mut Context<Self>,
    ) {
        let match_state = match check {
            RdpKnownHostCheck::Match => CertificateMatchState::Match,
            RdpKnownHostCheck::UnknownHost => CertificateMatchState::FirstUse,
            RdpKnownHostCheck::Changed {
                remembered_fingerprint,
            } => CertificateMatchState::Changed {
                remembered_fingerprint,
            },
        };
        let expected_previous_fingerprint = match &match_state {
            CertificateMatchState::Changed {
                remembered_fingerprint,
            } => Some(remembered_fingerprint.clone()),
            CertificateMatchState::FirstUse | CertificateMatchState::Match => None,
        };
        let evaluation =
            evaluate_certificate_match(policy, match_state, &request.sha256_fingerprint);
        match evaluation.decision {
            CertificateDecision::Accept => {
                let _ = self
                    .remote_desktop
                    .manager
                    .respond_certificate(&request.request_id, RdpCertificateResponse::TrustOnce);
            }
            CertificateDecision::AcceptAndRemember => {
                self.persist_rdp_certificate_and_respond(
                    request,
                    expected_previous_fingerprint,
                    cx,
                );
            }
            CertificateDecision::Reject => {
                let _ = self
                    .remote_desktop
                    .manager
                    .respond_certificate(&request.request_id, RdpCertificateResponse::Reject);
            }
            CertificateDecision::Prompt => {
                let Some(reason) = evaluation.prompt_reason else {
                    let _ = self
                        .remote_desktop
                        .manager
                        .respond_certificate(&request.request_id, RdpCertificateResponse::Reject);
                    return;
                };
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    session.certificate_request = Some(RdpCertificatePrompt { request, reason });
                }
            }
        }
    }

    pub(in crate::features) fn resolve_rdp_certificate(
        &mut self,
        session_id: &str,
        response: RdpCertificateResponse,
        cx: &mut Context<Self>,
    ) {
        let prompt = self
            .remote_desktop
            .sessions
            .get_mut(session_id)
            .and_then(|session| session.certificate_request.take());
        let Some(prompt) = prompt else {
            return;
        };
        let expected_previous_fingerprint = match &prompt.reason {
            CertificatePromptReason::FirstUse => None,
            CertificatePromptReason::Changed {
                previous_fingerprint,
                ..
            } => Some(previous_fingerprint.clone()),
        };
        if response == RdpCertificateResponse::TrustAndRemember {
            self.persist_rdp_certificate_and_respond(
                prompt.request,
                expected_previous_fingerprint,
                cx,
            );
            return;
        }
        let response = if matches!(prompt.reason, CertificatePromptReason::Changed { .. })
            && response == RdpCertificateResponse::TrustOnce
        {
            RdpCertificateResponse::Reject
        } else {
            response
        };
        if let Err(error) = self
            .remote_desktop
            .manager
            .respond_certificate(&prompt.request.request_id, response)
        {
            self.shell.set_status(format_rdp_error(&error));
        }
    }

    fn persist_rdp_certificate_and_respond(
        &mut self,
        request: RdpCertificateRequest,
        expected_previous_fingerprint: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let request_id = request.request_id.clone();
        let failure_request_id = request_id.clone();
        let host = request.host;
        let port = request.port;
        let fingerprint = request.sha256_fingerprint;
        let metadata = RdpCertificateMetadata {
            subject: request.subject,
            issuer: request.issuer,
            valid_from: request.valid_from,
            valid_to: request.valid_to,
        };
        let submitted = self.submit_store_request(
            0,
            store_request(StoreDomain::Security, move |store| {
                store.replace_rdp_known_host_if_matches(
                    &host,
                    port,
                    expected_previous_fingerprint.as_deref(),
                    &fingerprint,
                    metadata,
                )
            }),
            move |this, event, cx| {
                let response = match event.outcome {
                    Ok(true) => RdpCertificateResponse::TrustAndRemember,
                    Ok(false) => {
                        this.shell.set_status(
                            "RDP certificate changed again before confirmation; connection rejected"
                                .to_string(),
                        );
                        RdpCertificateResponse::Reject
                    }
                    Err(error) => {
                        this.shell.set_status(format!(
                            "RDP certificate could not be remembered: {error}"
                        ));
                        RdpCertificateResponse::Reject
                    }
                };
                if let Err(error) = this
                    .remote_desktop
                    .manager
                    .respond_certificate(&request_id, response)
                {
                    this.shell.set_status(format_rdp_error(&error));
                }
                cx.notify();
            },
            cx,
        );
        if !submitted {
            let _ = self
                .remote_desktop
                .manager
                .respond_certificate(&failure_request_id, RdpCertificateResponse::Reject);
        }
    }

    pub(in crate::features) fn queue_rdp_resize(
        &mut self,
        session_id: &str,
        width: u32,
        height: u32,
    ) {
        let width = width.clamp(200, 8192) & !1;
        let height = height.clamp(200, 8192) & !1;
        if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
            let remote_size = session
                .framebuffer
                .as_ref()
                .map(|framebuffer| (framebuffer.width(), framebuffer.height()));
            if session.dynamic_resize_disabled
                || !rdp_resize_is_material(remote_size, session.last_resize, (width, height))
            {
                return;
            }
            session.pending_resize = Some((width, height, Instant::now()));
        }
    }

    fn drive_rdp_resize_debounce(&mut self) -> bool {
        let now = Instant::now();
        let mut sent = false;
        for (session_id, session) in &mut self.remote_desktop.sessions {
            let Some((width, height, queued_at)) = session.pending_resize else {
                continue;
            };
            if now.saturating_duration_since(queued_at) < RESIZE_DEBOUNCE {
                continue;
            }
            session.pending_resize = None;
            if self
                .remote_desktop
                .manager
                .resize(session_id, width, height)
                .is_ok()
            {
                session.last_resize = Some((width, height));
                session.last_resize_sent_at = Some(now);
                sent = true;
            }
        }
        sent
    }

    fn poll_active_rdp_clipboard(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(session_id) = self.session.active_id_owned() else {
            return false;
        };
        if !self.remote_desktop.is_session(&session_id) {
            return false;
        }
        if !self
            .remote_desktop
            .sessions
            .get(&session_id)
            .is_some_and(|session| matches!(session.state, RdpSessionState::Connected))
        {
            return false;
        }
        let clipboard_target =
            self.session
                .metadata(&session_id)
                .and_then(|metadata| match &metadata.launch_config {
                    crate::models::SessionLaunchConfig::Rdp(config)
                        if config.clipboard.mode == RdpClipboardMode::TextOnly =>
                    {
                        Some(RemoteDesktopClipboardTarget::Rdp)
                    }
                    crate::models::SessionLaunchConfig::Vnc(config) if config.clipboard.enabled => {
                        Some(RemoteDesktopClipboardTarget::Vnc)
                    }
                    _ => None,
                });
        let Some(clipboard_target) = clipboard_target else {
            return false;
        };
        let now = Instant::now();
        if self
            .remote_desktop
            .last_clipboard_poll
            .is_some_and(|last| now.saturating_duration_since(last) < CLIPBOARD_POLL_INTERVAL)
        {
            return false;
        }
        self.remote_desktop.last_clipboard_poll = Some(now);
        if !clipboard_has_unicode_text() {
            return false;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return false;
        };
        let Some(session) = self.remote_desktop.sessions.get_mut(&session_id) else {
            return false;
        };
        let Ok(Some(_generation)) = session.clipboard.accept(ClipboardOrigin::Local, &text) else {
            return false;
        };
        match clipboard_target {
            RemoteDesktopClipboardTarget::Rdp => {
                if let Err(error) = self
                    .remote_desktop
                    .manager
                    .set_clipboard_text(&session_id, text)
                {
                    session.error = Some(error);
                }
            }
            RemoteDesktopClipboardTarget::Vnc => {
                if let Err(error) = self
                    .remote_desktop
                    .vnc_manager
                    .set_clipboard_text(&session_id, text)
                {
                    session.error = Some(RdpError::new(
                        vnc_error_as_rdp_kind(error.kind),
                        error.message,
                    ));
                }
            }
        }
        true
    }
}

#[derive(Clone, Copy)]
enum RemoteDesktopClipboardTarget {
    Rdp,
    Vnc,
}

#[cfg(target_os = "windows")]
fn clipboard_has_unicode_text() -> bool {
    // GPUI logs every unsupported OLE clipboard format it probes. Only enter
    // that path when Windows reports the text format this bridge accepts.
    unsafe {
        windows_sys::Win32::System::DataExchange::IsClipboardFormatAvailable(
            windows_sys::Win32::System::Ole::CF_UNICODETEXT as u32,
        ) != 0
    }
}

#[cfg(not(target_os = "windows"))]
fn clipboard_has_unicode_text() -> bool {
    true
}

fn upload_rdp_rect(
    window: &mut Window,
    texture: gpui::DynamicTexture,
    framebuffer: &Framebuffer,
    rect: DirtyRect,
) -> anyhow::Result<()> {
    let stride = framebuffer.width() * 4;
    let start = (u64::from(rect.y) * u64::from(stride) + u64::from(rect.x) * 4) as usize;
    let row_bytes = rect.width * 4;
    let len = (u64::from(rect.height - 1) * u64::from(stride) + u64::from(row_bytes)) as usize;
    let pixels = &framebuffer.pixels()[start..start + len];
    window.update_dynamic_texture(
        texture,
        Bounds::new(
            point(DevicePixels(rect.x as i32), DevicePixels(rect.y as i32)),
            size(
                DevicePixels(rect.width as i32),
                DevicePixels(rect.height as i32),
            ),
        ),
        pixels,
        stride,
    )
}

fn set_rdp_view_error(
    session: &mut super::state::RemoteDesktopSessionState,
    kind: RdpErrorKind,
    message: String,
) {
    let error = RdpError::new(kind, message);
    session.error = Some(error.clone());
    session.state = RdpSessionState::Failed(error);
}

fn rdp_error_is_retryable(kind: RdpErrorKind) -> bool {
    matches!(
        kind,
        RdpErrorKind::Timeout
            | RdpErrorKind::ConnectionRefused
            | RdpErrorKind::Tls
            | RdpErrorKind::Transport
            | RdpErrorKind::Session
    )
}

/// Project a VNC error onto the shared remote-desktop error vocabulary.
///
/// The view layer renders both protocols through `RdpErrorKind`; the helper
/// lifecycle kinds map straight across because RDP already has them.
fn vnc_error_as_rdp_kind(kind: VncErrorKind) -> RdpErrorKind {
    match kind {
        VncErrorKind::Authentication => RdpErrorKind::Authentication,
        VncErrorKind::Clipboard => RdpErrorKind::Clipboard,
        VncErrorKind::Transport => RdpErrorKind::Transport,
        VncErrorKind::Encoding | VncErrorKind::Protocol => RdpErrorKind::Protocol,
        VncErrorKind::Internal => RdpErrorKind::Session,
        VncErrorKind::HelperMissing => RdpErrorKind::HelperMissing,
        VncErrorKind::HelperCrashed => RdpErrorKind::HelperCrashed,
        VncErrorKind::Ipc => RdpErrorKind::Ipc,
    }
}

fn set_button_mask(mask: &mut u8, bit: u8, pressed: bool) {
    if pressed {
        *mask |= bit;
    } else {
        *mask &= !bit;
    }
}

fn vnc_input_allowed(view_only: bool) -> bool {
    !view_only
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RemoteInputPlatform {
    MacOs,
    Windows,
    Linux,
    Other,
}

impl RemoteInputPlatform {
    fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Other
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RemoteKeyModifiers {
    control: bool,
    alt: bool,
    platform: bool,
}

fn remote_committed_text_supported(
    is_vnc: bool,
    rdp_capabilities: Option<RdpServerCapabilities>,
    vnc_capabilities: Option<VncServerCapabilities>,
) -> bool {
    if is_vnc {
        vnc_capabilities.is_some_and(|capabilities| capabilities.committed_unicode_keysyms)
    } else {
        rdp_capabilities.is_some_and(|capabilities| capabilities.committed_unicode_text)
    }
}

fn remote_key_down_should_defer_to_text(
    key_char: Option<&str>,
    committed_text_supported: bool,
    input_platform: RemoteInputPlatform,
    modifiers: RemoteKeyModifiers,
) -> bool {
    if !committed_text_supported
        || modifiers.platform
        || !key_char.is_some_and(|text| {
            !text.is_empty() && text.chars().all(|character| !character.is_control())
        })
    {
        return false;
    }

    match input_platform {
        RemoteInputPlatform::MacOs => !modifiers.control,
        RemoteInputPlatform::Windows | RemoteInputPlatform::Linux => {
            modifiers.control == modifiers.alt
        }
        RemoteInputPlatform::Other => !modifiers.control && !modifiers.alt,
    }
}

fn remote_key_down_uses_text_route(
    repeat: bool,
    key_already_uses_text: bool,
    classified_for_text: bool,
) -> bool {
    if repeat {
        key_already_uses_text
    } else {
        classified_for_text
    }
}

fn vnc_capabilities_for_state(
    state: &VncSessionState,
    capabilities: Option<VncServerCapabilities>,
) -> Option<VncServerCapabilities> {
    matches!(state, VncSessionState::Connected)
        .then_some(capabilities)
        .flatten()
}

fn remote_state_clears_input(state: &RdpSessionState) -> bool {
    matches!(
        state,
        RdpSessionState::Reconnecting
            | RdpSessionState::Disconnecting
            | RdpSessionState::Disconnected
            | RdpSessionState::Failed(_)
    )
}

pub(super) fn secure_attention_available(
    is_rdp: bool,
    state: &RdpSessionState,
    capabilities: Option<RdpServerCapabilities>,
) -> bool {
    is_rdp
        && matches!(state, RdpSessionState::Connected)
        && capabilities.is_some_and(|capabilities| capabilities.secure_attention)
}

fn vnc_keysym_for_key(key: &str, key_char: Option<&str>) -> Option<u32> {
    let key = key.to_ascii_lowercase();
    let keysym = match key.as_str() {
        "backspace" => 0xff08,
        "tab" => 0xff09,
        "enter" => 0xff0d,
        "escape" => 0xff1b,
        "insert" => 0xff63,
        "delete" => 0xffff,
        "home" => 0xff50,
        "end" => 0xff57,
        "pageup" | "page_up" | "page up" => 0xff55,
        "pagedown" | "page_down" | "page down" => 0xff56,
        "left" | "arrowleft" | "arrow_left" => 0xff51,
        "up" | "arrowup" | "arrow_up" => 0xff52,
        "right" | "arrowright" | "arrow_right" => 0xff53,
        "down" | "arrowdown" | "arrow_down" => 0xff54,
        "shift" => 0xffe1,
        "control" | "ctrl" => 0xffe3,
        "alt" => 0xffe9,
        "meta" | "platform" | "command" | "super" => 0xffeb,
        "f1" => 0xffbe,
        "f2" => 0xffbf,
        "f3" => 0xffc0,
        "f4" => 0xffc1,
        "f5" => 0xffc2,
        "f6" => 0xffc3,
        "f7" => 0xffc4,
        "f8" => 0xffc5,
        "f9" => 0xffc6,
        "f10" => 0xffc7,
        "f11" => 0xffc8,
        "f12" => 0xffc9,
        _ => {
            let text = key_char
                .filter(|text| !text.is_empty())
                .unwrap_or(key.as_str());
            let mut chars = text.chars();
            let ch = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            let codepoint = u32::from(ch);
            if codepoint <= 0xff {
                codepoint
            } else {
                0x0100_0000 | codepoint
            }
        }
    };
    Some(keysym)
}

fn rdp_reconnect_delay(attempt: u32, jitter_ms: u64) -> Duration {
    const BACKOFF_SECONDS: [u64; 6] = [1, 2, 4, 8, 15, 30];
    let index = attempt.saturating_sub(1) as usize;
    Duration::from_secs(BACKOFF_SECONDS[index.min(BACKOFF_SECONDS.len() - 1)])
        + Duration::from_millis(jitter_ms.min(249))
}

fn clear_rdp_reconnect_after_frame(session: &mut super::state::RemoteDesktopSessionState) {
    session.reconnect_attempts = 0;
    session.reconnect_at = None;
    session.error = None;
}

fn rdp_resize_is_material(
    remote_size: Option<(u32, u32)>,
    last_resize: Option<(u32, u32)>,
    requested: (u32, u32),
) -> bool {
    if last_resize == Some(requested) {
        return false;
    }
    let Some((remote_width, remote_height)) = remote_size else {
        return true;
    };
    remote_width.abs_diff(requested.0) >= RESIZE_MIN_DELTA
        || remote_height.abs_diff(requested.1) >= RESIZE_MIN_DELTA
}

fn should_disable_dynamic_resize_after_state(
    state: &RdpSessionState,
    last_resize_sent_at: Option<Instant>,
    now: Instant,
) -> bool {
    matches!(
        state,
        RdpSessionState::Reconnecting | RdpSessionState::Failed(_)
    ) && last_resize_sent_at
        .is_some_and(|sent_at| now.saturating_duration_since(sent_at) <= RESIZE_FAILURE_WINDOW)
}

pub(super) fn format_rdp_error(error: &RdpError) -> String {
    let category = match error.kind {
        RdpErrorKind::Authentication => "Authentication failed",
        RdpErrorKind::CertificateRejected => "Certificate rejected",
        RdpErrorKind::Timeout => "Connection timed out",
        RdpErrorKind::ConnectionRefused => "Connection refused",
        RdpErrorKind::Tls => "RDP TLS connection failed",
        RdpErrorKind::Transport => "RDP transport interrupted",
        RdpErrorKind::Session => "RDP session failed",
        RdpErrorKind::Clipboard => "RDP clipboard failed",
        RdpErrorKind::Negotiation => "RDP negotiation failed",
        RdpErrorKind::HelperMissing => "RDP helper is missing",
        RdpErrorKind::HelperCrashed => "RDP helper crashed",
        RdpErrorKind::Ipc => "RDP helper communication failed",
        RdpErrorKind::Protocol => "RDP protocol error",
        RdpErrorKind::Unsupported => "RDP feature is unsupported",
    };
    format!("{category}: {}", error.message)
}

fn inline_remote_desktop_password(auth: Option<&nyaterm_core::ConnectionAuth>) -> Option<String> {
    let auth = auth?;
    if auth.mode == "none" {
        return None;
    }
    if let Some(password) = auth
        .password
        .as_deref()
        .filter(|password| !password.trim().is_empty())
    {
        return (!auth.has_password).then(|| password.to_string());
    }
    None
}

fn remote_desktop_password_id(auth: Option<&nyaterm_core::ConnectionAuth>) -> Option<String> {
    let auth = auth?;
    if auth.mode == "none"
        || auth
            .password
            .as_deref()
            .is_some_and(|password| !password.trim().is_empty() && !auth.has_password)
    {
        return None;
    }
    auth.password_id
        .as_deref()
        .map(str::trim)
        .filter(|password_id| !password_id.is_empty())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use nyaterm_core::ConnectionAuth;
    use nyaterm_remote_desktop::{
        RdpError, RdpErrorKind, RdpServerCapabilities, RdpSessionState, VncServerCapabilities,
        VncSessionState,
    };

    use super::{
        MAINTENANCE_INTERVAL, POINTER_MOVE_INTERVAL, RESIZE_DEBOUNCE, RemoteInputPlatform,
        RemoteKeyModifiers, clear_rdp_reconnect_after_frame, inline_remote_desktop_password,
        rdp_error_is_retryable, rdp_reconnect_delay, rdp_resize_is_material,
        remote_committed_text_supported, remote_desktop_password_id, remote_desktop_periodic_delay,
        remote_key_down_should_defer_to_text, remote_key_down_uses_text_route,
        remote_state_clears_input, secure_attention_available,
        should_disable_dynamic_resize_after_state, vnc_capabilities_for_state, vnc_input_allowed,
        vnc_keysym_for_key,
    };
    use crate::features::remote_desktop::state::RemoteDesktopSessionState;

    #[test]
    fn reconnect_classification_only_accepts_transient_failures() {
        for kind in [
            RdpErrorKind::Timeout,
            RdpErrorKind::ConnectionRefused,
            RdpErrorKind::Tls,
            RdpErrorKind::Transport,
            RdpErrorKind::Session,
        ] {
            assert!(rdp_error_is_retryable(kind), "{kind:?}");
        }
        for kind in [
            RdpErrorKind::Authentication,
            RdpErrorKind::CertificateRejected,
            RdpErrorKind::Negotiation,
            RdpErrorKind::Clipboard,
            RdpErrorKind::HelperMissing,
            RdpErrorKind::HelperCrashed,
            RdpErrorKind::Ipc,
            RdpErrorKind::Protocol,
            RdpErrorKind::Unsupported,
        ] {
            assert!(!rdp_error_is_retryable(kind), "{kind:?}");
        }
    }

    #[test]
    fn reconnect_password_selection_resolves_locked_values_by_id() {
        let auth = ConnectionAuth {
            mode: "password".to_string(),
            password: Some("masked-or-encrypted".to_string()),
            password_id: Some("pw-rdp".to_string()),
            has_password: true,
            ..ConnectionAuth::default()
        };

        assert_eq!(inline_remote_desktop_password(Some(&auth)), None);
        assert_eq!(
            remote_desktop_password_id(Some(&auth)).as_deref(),
            Some("pw-rdp")
        );
    }

    #[test]
    fn reconnect_backoff_caps_and_bounds_jitter() {
        let expected = [1, 2, 4, 8, 15, 30, 30];
        for (index, seconds) in expected.into_iter().enumerate() {
            assert_eq!(
                rdp_reconnect_delay(index as u32 + 1, 0),
                Duration::from_secs(seconds)
            );
        }
        assert_eq!(rdp_reconnect_delay(1, 999), Duration::from_millis(1_249));
    }

    /// A waiting pointer move is the one thing here that needs a fine cadence.
    ///
    /// Its send is budgeted against `POINTER_MOVE_INTERVAL`, so a flush on any coarser
    /// schedule is a visibly late cursor -- which is what the runtime tick's 500ms
    /// quiet interval was doing, since `runtime_quiet_tick_allowed` has no
    /// remote-desktop term. Everything else here debounces or gates itself.
    #[test]
    fn a_waiting_pointer_move_gets_the_fine_cadence() {
        assert_eq!(
            remote_desktop_periodic_delay(true),
            POINTER_MOVE_INTERVAL,
            "a coalesced pointer move must not wait longer than its own send interval"
        );
        assert_eq!(remote_desktop_periodic_delay(false), MAINTENANCE_INTERVAL);
        assert!(
            MAINTENANCE_INTERVAL < RESIZE_DEBOUNCE,
            "the maintenance cadence has to be finer than the shortest thing it              services, or a resize debounce resolves late"
        );
    }

    #[test]
    fn first_frame_clears_reconnect_attempt_and_error_state() {
        let mut session = RemoteDesktopSessionState {
            reconnect_attempts: 4,
            reconnect_at: Some(Instant::now()),
            error: Some(RdpError::new(RdpErrorKind::Transport, "interrupted")),
            ..Default::default()
        };

        clear_rdp_reconnect_after_frame(&mut session);

        assert_eq!(session.reconnect_attempts, 0);
        assert_eq!(session.reconnect_at, None);
        assert_eq!(session.error, None);
    }

    #[test]
    fn resize_filter_ignores_duplicate_and_sub_threshold_changes() {
        assert!(!rdp_resize_is_material(
            Some((1280, 720)),
            None,
            (1300, 740)
        ));
        assert!(rdp_resize_is_material(Some((1280, 720)), None, (1312, 720)));
        assert!(!rdp_resize_is_material(
            None,
            Some((1280, 720)),
            (1280, 720)
        ));
        assert!(rdp_resize_is_material(None, None, (1280, 720)));
    }

    #[test]
    fn resize_related_failure_disables_dynamic_resize_only_inside_window() {
        let now = Instant::now();
        let error = RdpError::new(RdpErrorKind::Session, "resize failed");
        assert!(should_disable_dynamic_resize_after_state(
            &nyaterm_remote_desktop::RdpSessionState::Failed(error.clone()),
            Some(now - Duration::from_secs(2)),
            now,
        ));
        assert!(!should_disable_dynamic_resize_after_state(
            &nyaterm_remote_desktop::RdpSessionState::Failed(error),
            Some(now - Duration::from_secs(4)),
            now,
        ));
        assert!(!should_disable_dynamic_resize_after_state(
            &nyaterm_remote_desktop::RdpSessionState::Connected,
            Some(now),
            now,
        ));
    }

    #[test]
    fn committed_text_requires_the_confirmed_protocol_capability() {
        let rdp_supported = RdpServerCapabilities {
            committed_unicode_text: true,
            secure_attention: false,
        };
        let vnc_supported = VncServerCapabilities {
            committed_unicode_keysyms: true,
        };

        assert!(!remote_committed_text_supported(false, None, None));
        assert!(!remote_committed_text_supported(
            false,
            Some(RdpServerCapabilities::default()),
            None,
        ));
        assert!(remote_committed_text_supported(
            false,
            Some(rdp_supported),
            None,
        ));
        assert!(!remote_committed_text_supported(true, None, None));
        assert!(!remote_committed_text_supported(
            true,
            None,
            Some(VncServerCapabilities::default()),
        ));
        assert!(remote_committed_text_supported(
            true,
            None,
            Some(vnc_supported),
        ));
    }

    #[test]
    fn printable_keydown_uses_platform_text_service_policy() {
        let plain = RemoteKeyModifiers::default();
        let control = RemoteKeyModifiers {
            control: true,
            ..Default::default()
        };
        let alt = RemoteKeyModifiers {
            alt: true,
            ..Default::default()
        };
        let alt_gr = RemoteKeyModifiers {
            control: true,
            alt: true,
            ..Default::default()
        };
        let platform = RemoteKeyModifiers {
            platform: true,
            ..Default::default()
        };
        let defers = |input_platform, modifiers| {
            remote_key_down_should_defer_to_text(Some("é"), true, input_platform, modifiers)
        };

        for input_platform in [
            RemoteInputPlatform::MacOs,
            RemoteInputPlatform::Windows,
            RemoteInputPlatform::Linux,
        ] {
            assert!(defers(input_platform, plain));
            assert!(!defers(input_platform, control));
            assert!(!defers(input_platform, platform));
        }
        assert!(defers(RemoteInputPlatform::MacOs, alt));
        assert!(!defers(RemoteInputPlatform::MacOs, alt_gr));
        for input_platform in [RemoteInputPlatform::Windows, RemoteInputPlatform::Linux] {
            assert!(defers(input_platform, alt_gr));
            assert!(!defers(input_platform, alt));
        }
    }

    #[test]
    fn repeat_keydowns_keep_the_initial_route_when_modifiers_change() {
        assert!(!remote_key_down_uses_text_route(true, false, true));
        assert!(remote_key_down_uses_text_route(true, true, false));
        assert!(remote_key_down_uses_text_route(false, false, true));
        assert!(!remote_key_down_uses_text_route(false, true, false));
    }

    #[test]
    fn unsupported_or_non_printable_keys_stay_on_the_physical_path() {
        let plain = RemoteKeyModifiers::default();
        assert!(!remote_key_down_should_defer_to_text(
            Some("a"),
            false,
            RemoteInputPlatform::Windows,
            plain,
        ));
        assert!(!remote_key_down_should_defer_to_text(
            Some("\r"),
            true,
            RemoteInputPlatform::MacOs,
            plain,
        ));
        assert!(!remote_key_down_should_defer_to_text(
            None,
            true,
            RemoteInputPlatform::Linux,
            plain,
        ));
    }

    #[test]
    fn vnc_capability_cache_only_survives_connected_state() {
        let supported = Some(VncServerCapabilities {
            committed_unicode_keysyms: true,
        });

        assert_eq!(
            vnc_capabilities_for_state(&VncSessionState::Connected, supported),
            supported,
        );
        assert_eq!(
            vnc_capabilities_for_state(&VncSessionState::Reconnecting, supported),
            None,
        );
        assert_eq!(
            vnc_capabilities_for_state(&VncSessionState::Disconnected, supported),
            None,
        );
    }

    #[test]
    fn physical_vnc_key_mapping_preserves_shortcut_and_navigation_keys() {
        assert_eq!(vnc_keysym_for_key("c", Some("c")), Some(u32::from('c')));
        assert_eq!(vnc_keysym_for_key("left", None), Some(0xff51));
        assert_eq!(vnc_keysym_for_key("F12", None), Some(0xffc9));
    }

    #[test]
    fn vnc_view_only_is_rejected_before_manager_dispatch() {
        assert!(vnc_input_allowed(false));
        assert!(!vnc_input_allowed(true));
    }

    #[test]
    fn secure_attention_requires_rdp_connected_state_and_confirmed_capability() {
        let supported = Some(RdpServerCapabilities {
            committed_unicode_text: true,
            secure_attention: true,
        });
        assert!(secure_attention_available(
            true,
            &RdpSessionState::Connected,
            supported,
        ));
        assert!(!secure_attention_available(
            false,
            &RdpSessionState::Connected,
            supported,
        ));
        assert!(!secure_attention_available(
            true,
            &RdpSessionState::Connecting,
            supported,
        ));
        assert!(!secure_attention_available(
            true,
            &RdpSessionState::Connected,
            None,
        ));
        assert!(!secure_attention_available(
            true,
            &RdpSessionState::Connected,
            Some(RdpServerCapabilities::default()),
        ));
    }

    #[test]
    fn disconnecting_states_clear_local_composition_and_key_suppression() {
        assert!(!remote_state_clears_input(&RdpSessionState::Connected));
        assert!(remote_state_clears_input(&RdpSessionState::Reconnecting));
        assert!(remote_state_clears_input(&RdpSessionState::Disconnected));
        assert!(remote_state_clears_input(&RdpSessionState::Failed(
            RdpError::new(RdpErrorKind::Session, "closed")
        )));
    }
}
