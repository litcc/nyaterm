use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

pub const PROTOCOL_VERSION: u32 = 2;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdpSessionConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub domain: String,
    pub password: Option<String>,
    pub use_nla: bool,
    pub certificate_policy: RdpCertificatePolicy,
    pub display: RdpDisplayConfig,
    pub clipboard: RdpClipboardConfig,
    pub reconnect: RdpReconnectConfig,
}

impl fmt::Debug for RdpSessionConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RdpSessionConfig")
            .field("name", &self.name)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("domain", &self.domain)
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .field("use_nla", &self.use_nla)
            .field("certificate_policy", &self.certificate_policy)
            .field("display", &self.display)
            .field("clipboard", &self.clipboard)
            .field("reconnect", &self.reconnect)
            .finish()
    }
}

impl Drop for RdpSessionConfig {
    fn drop(&mut self) {
        if let Some(password) = self.password.as_mut() {
            password.zeroize();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RdpCertificatePolicy {
    #[default]
    Prompt,
    TrustOnFirstUse,
    Strict,
    #[serde(alias = "accept-temporarily")]
    Insecure,
    #[serde(alias = "reject_changed")]
    RejectChanged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdpDisplayConfig {
    pub mode: RdpDisplayMode,
    pub width: u32,
    pub height: u32,
    pub color_depth: u8,
}

impl Default for RdpDisplayConfig {
    fn default() -> Self {
        Self {
            mode: RdpDisplayMode::FitWindow,
            width: 1920,
            height: 1080,
            color_depth: 32,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RdpDisplayMode {
    #[default]
    FitWindow,
    Fixed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdpClipboardConfig {
    pub mode: RdpClipboardMode,
}

impl Default for RdpClipboardConfig {
    fn default() -> Self {
        Self {
            mode: RdpClipboardMode::TextOnly,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RdpClipboardMode {
    Disabled,
    #[default]
    TextOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdpReconnectConfig {
    pub enabled: bool,
    pub max_attempts: u32,
}

impl Default for RdpReconnectConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 5,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RdpInputEvent {
    KeyDown {
        scan_code: u16,
        extended: bool,
        repeat: bool,
    },
    KeyUp {
        scan_code: u16,
        extended: bool,
        repeat: bool,
    },
    Unicode {
        text: String,
    },
    Pointer {
        x: u32,
        y: u32,
        button: Option<RdpPointerButton>,
        pressed: bool,
    },
    ReleaseAllKeys,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RdpPointerButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PixelFormat {
    Bgra8,
    Rgba8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RdpFrameEvent {
    Reset {
        epoch: u64,
        width: u32,
        height: u32,
    },
    Bitmap {
        epoch: u64,
        full: bool,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        stride: u32,
        format: PixelFormat,
        pixels: Vec<u8>,
    },
    Cursor(RdpCursorEvent),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RdpCursorEvent {
    pub epoch: u64,
    pub visible: bool,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub hotspot_x: u32,
    pub hotspot_y: u32,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RdpSessionState {
    Idle,
    Connecting,
    Connected,
    Reconnecting,
    Disconnecting,
    Disconnected,
    Failed(RdpError),
    #[deprecated(note = "use Connected")]
    Active,
    #[deprecated(note = "use Disconnected")]
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdpCertificateRequest {
    pub request_id: String,
    pub host: String,
    pub port: u16,
    pub sha256_fingerprint: String,
    pub subject: Option<String>,
    pub issuer: Option<String>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RdpCertificateResponse {
    Reject,
    TrustOnce,
    TrustAndRemember,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RdpCapability {
    DynamicResizeUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RdpErrorKind {
    Authentication,
    CertificateRejected,
    Timeout,
    ConnectionRefused,
    Tls,
    Transport,
    Session,
    Clipboard,
    Negotiation,
    HelperMissing,
    HelperCrashed,
    Ipc,
    Protocol,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{kind:?}: {message}")]
pub struct RdpError {
    pub kind: RdpErrorKind,
    pub message: String,
}

impl RdpError {
    pub fn new(kind: RdpErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RdpRuntimeEvent {
    State {
        session_id: String,
        state: RdpSessionState,
        message: Option<String>,
    },
    Frame {
        session_id: String,
        event: RdpFrameEvent,
    },
    Clipboard {
        session_id: String,
        text: String,
        generation: u64,
    },
    CertificateRequest(RdpCertificateRequest),
    Capability {
        session_id: String,
        capability: RdpCapability,
    },
    Error {
        session_id: String,
        error: RdpError,
        fatal: bool,
    },
}

/// Called after an event is enqueued on a session queue, so the consumer can be
/// woken instead of polling for it.
///
/// A callback rather than a channel on purpose: the session queues coalesce
/// (they keep only the newest frame), so they cannot become channels, and this
/// crate is linked by both helper processes and stays free of any async runtime.
/// The application supplies a closure that signals its own wake.
///
/// # Contract
///
/// The waker is invoked while the session queue lock is held, so it must not
/// block, must not re-enter the session manager, and must not panic. Signalling
/// an atomic and posting to an unbounded channel is the intended shape.
pub type QueueWaker = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RdpSessionDrain {
    pub control: Vec<RdpRuntimeEvent>,
    pub frames: Vec<RdpFrameEvent>,
    pub dropped_frames: usize,
    pub waiting_for_full_frame: bool,
}

pub const MAX_VNC_CLIPBOARD_TEXT_BYTES: usize = 1024 * 1024;
pub const MAX_VNC_INPUT_BATCH: usize = 256;
pub const MAX_VNC_FRAMEBUFFER_WIDTH: u32 = 7680;
pub const MAX_VNC_FRAMEBUFFER_HEIGHT: u32 = 4320;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VncSessionConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
    pub security: VncSecurityConfig,
    pub display: VncDisplayConfig,
    pub clipboard: VncClipboardConfig,
    pub reconnect: VncReconnectConfig,
    pub shared: bool,
    pub view_only: bool,
}

impl fmt::Debug for VncSessionConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VncSessionConfig")
            .field("name", &self.name)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .field("security", &self.security)
            .field("display", &self.display)
            .field("clipboard", &self.clipboard)
            .field("reconnect", &self.reconnect)
            .field("shared", &self.shared)
            .field("view_only", &self.view_only)
            .finish()
    }
}

impl Drop for VncSessionConfig {
    fn drop(&mut self) {
        if let Some(password) = self.password.as_mut() {
            password.zeroize();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VncSecurityConfig {
    pub mode: VncSecurityMode,
}

impl Default for VncSecurityConfig {
    fn default() -> Self {
        Self {
            mode: VncSecurityMode::Auto,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VncSecurityMode {
    #[default]
    Auto,
    None,
    #[serde(alias = "vnc-auth")]
    VncAuth,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VncDisplayConfig {
    pub scale_mode: VncScaleMode,
}

impl Default for VncDisplayConfig {
    fn default() -> Self {
        Self {
            scale_mode: VncScaleMode::Fit,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VncScaleMode {
    #[default]
    Fit,
    Stretch,
    Actual,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VncClipboardConfig {
    pub enabled: bool,
}

impl Default for VncClipboardConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VncReconnectConfig {
    pub enabled: bool,
    pub max_attempts: u32,
}

impl Default for VncReconnectConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 5,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VncInputEvent {
    Key { keysym: u32, pressed: bool },
    Pointer { x: u32, y: u32, button_mask: u8 },
    ReleaseAllKeys,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VncSessionState {
    Connecting,
    Authenticating,
    Negotiating,
    Connected,
    Reconnecting,
    Disconnecting,
    Disconnected,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VncErrorKind {
    Transport,
    Authentication,
    Protocol,
    Encoding,
    Clipboard,
    Internal,
    /// The helper binary was not found beside the application.
    HelperMissing,
    /// The helper exited or panicked while a session was live.
    HelperCrashed,
    /// The helper IPC channel itself failed.
    Ipc,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{kind:?}: {message}")]
pub struct VncError {
    pub kind: VncErrorKind,
    pub message: String,
}

impl VncError {
    pub fn new(kind: VncErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VncRuntimeEvent {
    State {
        session_id: String,
        state: VncSessionState,
        message: Option<String>,
    },
    Frame {
        session_id: String,
        event: RdpFrameEvent,
    },
    Clipboard {
        session_id: String,
        text: String,
    },
    Error {
        session_id: String,
        error: VncError,
        fatal: bool,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VncSessionDrain {
    pub control: Vec<VncRuntimeEvent>,
    pub frames: Vec<RdpFrameEvent>,
    pub dropped_frames: usize,
    pub waiting_for_full_frame: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RdpControlMessage {
    ClientHello {
        version: u32,
    },
    ServerHello {
        version: u32,
    },
    Connect {
        session_id: String,
        config: RdpSessionConfig,
    },
    DesktopReset {
        session_id: String,
        epoch: u64,
        width: u32,
        height: u32,
    },
    State {
        session_id: String,
        state: RdpSessionState,
        message: Option<String>,
    },
    Input {
        session_id: String,
        events: Vec<RdpInputEvent>,
    },
    Resize {
        session_id: String,
        width: u32,
        height: u32,
    },
    Clipboard {
        session_id: String,
        text: String,
        generation: u64,
    },
    CertificateRequest(RdpCertificateRequest),
    CertificateResponse {
        request_id: String,
        response: RdpCertificateResponse,
    },
    Capability {
        session_id: String,
        capability: RdpCapability,
    },
    Error {
        session_id: String,
        error: RdpError,
        fatal: bool,
    },
    RequestFullFrame {
        session_id: String,
    },
    Disconnect {
        session_id: String,
    },
}

/// Control vocabulary spoken between the application and `nyaterm-vnc-helper`.
///
/// Deliberately narrower than [`RdpControlMessage`]: this VNC path has no dynamic
/// resize, no TLS certificate prompt, and no capability negotiation. Framebuffer
/// and cursor payloads reuse the protocol-neutral binary packets in `crate::ipc`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum VncControlMessage {
    ClientHello {
        version: u32,
    },
    ServerHello {
        version: u32,
    },
    Connect {
        session_id: String,
        config: VncSessionConfig,
    },
    /// The helper owns the epoch counter and stamps every frame with it.
    DesktopReset {
        session_id: String,
        epoch: u64,
        width: u32,
        height: u32,
    },
    State {
        session_id: String,
        state: VncSessionState,
        message: Option<String>,
    },
    Input {
        session_id: String,
        events: Vec<VncInputEvent>,
    },
    Clipboard {
        session_id: String,
        text: String,
    },
    Error {
        session_id: String,
        error: VncError,
        fatal: bool,
    },
    RequestFullFrame {
        session_id: String,
    },
    Disconnect {
        session_id: String,
    },
}

pub fn parse_rdp_certificate_policy(value: &str) -> RdpCertificatePolicy {
    match value.trim() {
        "trust_on_first_use" | "trust-on-first-use" | "tofu" => {
            RdpCertificatePolicy::TrustOnFirstUse
        }
        "strict" | "reject_changed" | "reject-changed" => RdpCertificatePolicy::Strict,
        "insecure" | "accept-temporarily" => RdpCertificatePolicy::Insecure,
        _ => RdpCertificatePolicy::Prompt,
    }
}

pub fn parse_rdp_display_mode(value: &str) -> RdpDisplayMode {
    match value.trim() {
        "fixed" | "native" => RdpDisplayMode::Fixed,
        _ => RdpDisplayMode::FitWindow,
    }
}

pub fn parse_rdp_clipboard_mode(value: &str) -> RdpClipboardMode {
    match value.trim() {
        "disabled" | "off" => RdpClipboardMode::Disabled,
        _ => RdpClipboardMode::TextOnly,
    }
}

pub fn parse_vnc_security_mode(value: &str) -> VncSecurityMode {
    match value.trim() {
        "none" => VncSecurityMode::None,
        "vnc-auth" | "vnc_auth" | "password" => VncSecurityMode::VncAuth,
        _ => VncSecurityMode::Auto,
    }
}

pub fn parse_vnc_scale_mode(value: &str) -> VncScaleMode {
    match value.trim() {
        "stretch" => VncScaleMode::Stretch,
        "actual" | "actual-size" => VncScaleMode::Actual,
        _ => VncScaleMode::Fit,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RdpCertificatePolicy, RdpDisplayMode, parse_rdp_certificate_policy, parse_rdp_display_mode,
    };

    #[test]
    fn certificate_policy_accepts_public_and_legacy_names() {
        assert_eq!(
            parse_rdp_certificate_policy("accept-temporarily"),
            RdpCertificatePolicy::Insecure
        );
        assert_eq!(
            parse_rdp_certificate_policy("insecure"),
            RdpCertificatePolicy::Insecure
        );
        assert_eq!(
            parse_rdp_certificate_policy("tofu"),
            RdpCertificatePolicy::TrustOnFirstUse
        );
        assert_eq!(
            parse_rdp_certificate_policy("reject-changed"),
            RdpCertificatePolicy::Strict
        );
    }

    #[test]
    fn legacy_native_display_mode_is_fixed() {
        assert_eq!(parse_rdp_display_mode("native"), RdpDisplayMode::Fixed);
    }
}
