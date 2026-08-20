use std::io::Write;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use nyaterm_remote_desktop::{
    PROTOCOL_VERSION, VncClipboardConfig, VncControlMessage, VncDisplayConfig, VncErrorKind,
    VncReconnectConfig, VncSecurityConfig, VncSessionConfig, VncSessionState, decode_vnc_control,
    encode_vnc_control, read_packet, write_packet,
};

fn helper_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nyaterm-vnc-helper"))
}

fn spawn_helper() -> Child {
    helper_command()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

fn send(stdin: &mut impl Write, message: &VncControlMessage) {
    write_packet(stdin, &encode_vnc_control(message).unwrap()).unwrap();
}

fn recv(stdout: &mut impl std::io::Read) -> VncControlMessage {
    decode_vnc_control(&read_packet(stdout).unwrap().unwrap()).unwrap()
}

/// A port with nothing listening on it: bind, read the port, then release it.
fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn config(port: u16) -> VncSessionConfig {
    VncSessionConfig {
        name: "lifecycle".to_string(),
        host: "127.0.0.1".to_string(),
        port,
        password: None,
        security: VncSecurityConfig::default(),
        display: VncDisplayConfig::default(),
        clipboard: VncClipboardConfig::default(),
        // Fail on the first attempt instead of walking the reconnect ladder.
        reconnect: VncReconnectConfig {
            enabled: false,
            max_attempts: 0,
        },
        shared: true,
        view_only: false,
    }
}

#[test]
fn helper_handshake_disconnects_and_reaps_cleanly() {
    let mut child = spawn_helper();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    send(
        &mut stdin,
        &VncControlMessage::ClientHello {
            version: PROTOCOL_VERSION,
        },
    );
    assert!(matches!(
        recv(&mut stdout),
        VncControlMessage::ServerHello {
            version: PROTOCOL_VERSION
        }
    ));

    send(
        &mut stdin,
        &VncControlMessage::Disconnect {
            session_id: "test-session".to_string(),
        },
    );
    assert!(matches!(
        recv(&mut stdout),
        VncControlMessage::State {
            state: VncSessionState::Disconnecting,
            ..
        }
    ));
    assert!(matches!(
        recv(&mut stdout),
        VncControlMessage::State {
            state: VncSessionState::Disconnected,
            ..
        }
    ));
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn unreachable_server_reports_a_fatal_transport_error_and_still_disconnects() {
    let mut child = spawn_helper();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    send(
        &mut stdin,
        &VncControlMessage::ClientHello {
            version: PROTOCOL_VERSION,
        },
    );
    assert!(matches!(
        recv(&mut stdout),
        VncControlMessage::ServerHello { .. }
    ));

    send(
        &mut stdin,
        &VncControlMessage::Connect {
            session_id: "unreachable".to_string(),
            config: config(closed_port()),
        },
    );
    assert!(matches!(
        recv(&mut stdout),
        VncControlMessage::State {
            state: VncSessionState::Connecting,
            ..
        }
    ));
    let VncControlMessage::Error {
        session_id,
        error,
        fatal,
    } = recv(&mut stdout)
    else {
        panic!("expected a fatal connection error");
    };
    assert_eq!(session_id, "unreachable");
    assert_eq!(error.kind, VncErrorKind::Transport);
    assert!(fatal);

    // A dead worker must not wedge the control channel.
    send(
        &mut stdin,
        &VncControlMessage::Disconnect {
            session_id: "unreachable".to_string(),
        },
    );
    assert!(matches!(
        recv(&mut stdout),
        VncControlMessage::State {
            state: VncSessionState::Disconnecting,
            ..
        }
    ));
    assert!(matches!(
        recv(&mut stdout),
        VncControlMessage::State {
            state: VncSessionState::Disconnected,
            ..
        }
    ));
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn a_second_connect_is_rejected_without_replacing_the_session() {
    let mut child = spawn_helper();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    let port = closed_port();
    send(
        &mut stdin,
        &VncControlMessage::Connect {
            session_id: "first".to_string(),
            config: config(port),
        },
    );
    send(
        &mut stdin,
        &VncControlMessage::Connect {
            session_id: "second".to_string(),
            config: config(port),
        },
    );

    // The rejection names the second session and is fatal for it.
    let mut rejected = None;
    for _ in 0..6 {
        if let VncControlMessage::Error {
            session_id,
            error,
            fatal,
        } = recv(&mut stdout)
            && session_id == "second"
        {
            rejected = Some((error, fatal));
            break;
        }
    }
    let (error, fatal) = rejected.expect("the second connect should be rejected");
    assert_eq!(error.kind, VncErrorKind::Protocol);
    assert!(fatal);
    assert!(error.message.contains("already owns"));

    drop(stdin);
    child.wait().unwrap();
}

#[test]
fn helper_crash_and_hang_processes_can_always_be_reaped() {
    let crash = helper_command()
        .env("NYATERM_VNC_HELPER_TEST_MODE", "crash")
        .status()
        .unwrap();
    assert_eq!(crash.code(), Some(91));

    let mut hung = helper_command()
        .env("NYATERM_VNC_HELPER_TEST_MODE", "hang")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_millis(100);
    while Instant::now() < deadline {
        assert!(hung.try_wait().unwrap().is_none());
        std::thread::sleep(Duration::from_millis(10));
    }
    hung.kill().unwrap();
    let status = hung.wait().unwrap();
    assert!(!status.success());
}
