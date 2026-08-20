mod certificate;
mod clipboard;
mod frame;
mod helper_process;
mod input;
mod ipc;
mod protocol;
mod session;
mod vnc;

pub use certificate::{CertificateDecision, evaluate_certificate};
pub use clipboard::{ClipboardOrigin, ClipboardTracker, MAX_CLIPBOARD_TEXT_BYTES};
pub use frame::{DirtyRect, Framebuffer, FramebufferError, merge_dirty_rects};
pub use input::{KeyMapper, RemoteKey, viewport_to_remote};
pub use ipc::{
    CONTROL_PAYLOAD_LIMIT, FRAME_PAYLOAD_LIMIT, HEADER_LEN, Packet, PacketReader, PacketType,
    decode_control, decode_cursor_packet, decode_frame_packet, decode_vnc_control, encode_control,
    encode_cursor_packet, encode_frame_packet, encode_vnc_control, read_packet, write_packet,
    write_packet_into,
};
pub use protocol::{
    MAX_VNC_CLIPBOARD_TEXT_BYTES, MAX_VNC_FRAMEBUFFER_HEIGHT, MAX_VNC_FRAMEBUFFER_WIDTH,
    MAX_VNC_INPUT_BATCH, PROTOCOL_VERSION, PixelFormat, RdpCapability, RdpCertificatePolicy,
    RdpCertificateRequest, RdpCertificateResponse, RdpClipboardConfig, RdpClipboardMode,
    RdpControlMessage, RdpCursorEvent, RdpDisplayConfig, RdpDisplayMode, RdpError, RdpErrorKind,
    RdpFrameEvent, RdpInputEvent, RdpPointerButton, RdpReconnectConfig, RdpRuntimeEvent,
    RdpSessionConfig, RdpSessionDrain, RdpSessionState, VncClipboardConfig, VncControlMessage,
    VncDisplayConfig, VncError, VncErrorKind, VncInputEvent, VncReconnectConfig, VncRuntimeEvent,
    VncScaleMode, VncSecurityConfig, VncSecurityMode, VncSessionConfig, VncSessionDrain,
    VncSessionState, parse_rdp_certificate_policy, parse_rdp_clipboard_mode,
    parse_rdp_display_mode, parse_vnc_scale_mode, parse_vnc_security_mode,
};
pub use session::{RdpSessionManager, resolve_helper_path};
pub use vnc::{VncSessionManager, validate_vnc_config};
