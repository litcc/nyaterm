use super::{
    AiExecutionProfile, ConnectionAuth, ConnectionType, DecryptedOtpEntry,
    DecryptedSavedCredential, MAX_SFTP_SHELL_DETECTION_TIMEOUT_MS,
    MIN_SFTP_SHELL_DETECTION_TIMEOUT_MS, ProxyConfig, QuickCommand, QuickCommandCategory,
    QuickCommandCategoryPosition, QuickCommandRelativePosition, QuickCommandsConfig, RecordingMode,
    RecordingRotationPolicy, RestorableOpenTab, SavedConnection, SessionsConfig, SftpCwdFollowMode,
    SftpSettings, SshAgentEndpoint, SshAlgorithmMode, SshAlgorithmPreferences, SshKey, SshProfile,
    SshTerminalType, default_sftp_shell_detection_timeout_ms, quick_command_category_move_neighbor,
    quick_command_category_sibling_order, resolve_ssh_terminal_type, validate_sftp_settings,
};

#[test]
fn secret_bearing_model_debug_output_is_redacted() {
    let secret = "nya-test-secret-never-log";
    let values = [
        format!(
            "{:?}",
            ConnectionAuth {
                password: Some(secret.to_string()),
                ..ConnectionAuth::default()
            }
        ),
        format!(
            "{:?}",
            SshKey {
                id: "key-1".to_string(),
                name: "Test key".to_string(),
                key: Some(secret.to_string()),
                cert: Some(secret.to_string()),
                passphrase: Some(secret.to_string()),
                key_file_path: None,
                cert_file_path: None,
                has_key_data: true,
                has_cert_data: true,
            }
        ),
        format!(
            "{:?}",
            DecryptedSavedCredential {
                id: "credential-1".to_string(),
                sort_order: 0,
                name: "Test credential".to_string(),
                username: "tester".to_string(),
                password: Some(secret.to_string()),
                username_prompt_regex: None,
                password_prompt_regex: None,
                enabled: true,
            }
        ),
        format!(
            "{:?}",
            DecryptedOtpEntry {
                id: "otp-1".to_string(),
                otp_type: "totp".to_string(),
                issuer: "NyaTerm".to_string(),
                username: "tester".to_string(),
                secret: Some(secret.to_string()),
                algorithm: "SHA1".to_string(),
                digits: 6,
                period: 30,
                counter: 0,
            }
        ),
        format!(
            "{:?}",
            ProxyConfig {
                command: Some(secret.to_string()),
                password: Some(secret.to_string()),
                ..ProxyConfig::default()
            }
        ),
    ];

    for output in values {
        assert!(!output.contains(secret));
        assert!(output.contains("<redacted>"));
    }
}

#[test]
fn parses_legacy_ssh_connection_shape() {
    let json = r#"{
            "sessions": [{
                "id": "conn-1",
                "name": "Production",
                "type": "ssh",
                "host": "10.0.0.8",
                "port": 2222,
                "username": "deploy",
                "auth": {
                    "mode": "password",
                    "password_id": "pw-1",
                    "has_password": true
                },
                "post_login": {
                    "enabled": true,
                    "command": "uptime",
                    "delay_ms": 1500
                }
            }],
            "groups": [{
                "id": "group-1",
                "name": "Servers"
            }]
        }"#;

    let config: SessionsConfig = serde_json::from_str(json).expect("valid sessions config");

    assert_eq!(config.groups.len(), 1);
    assert_eq!(config.connections.len(), 1);
    assert_eq!(config.connections[0].kind_label(), "SSH");
    assert_eq!(config.connections[0].endpoint(), "deploy@10.0.0.8:2222");
    match &config.connections[0].config {
        ConnectionType::Ssh {
            ai_execution_profile,
            ..
        } => assert_eq!(*ai_execution_profile, AiExecutionProfile::Auto),
        other => panic!("expected SSH connection, got {other:?}"),
    }
}

#[test]
fn legacy_connections_default_new_tauri_compatibility_fields() {
    let json = r#"{
            "sessions": [
                {"id":"ssh","name":"SSH","type":"ssh","host":"10.0.0.8"},
                {"id":"local","name":"Local","type":"local_terminal"},
                {"id":"telnet","name":"Telnet","type":"telnet","host":"10.0.0.9"},
                {"id":"serial","name":"Serial","type":"serial","port_name":"COM1"}
            ],
            "groups": []
        }"#;

    let config: SessionsConfig = serde_json::from_str(json).expect("valid sessions config");

    assert_eq!(config.connections.len(), 4);
    for connection in &config.connections {
        assert!(connection.recording.is_none());
        assert!(connection.ssh_algorithms.is_none());
        assert_eq!(connection.sftp, SftpSettings::default());
    }
    match &config.connections[0].config {
        ConnectionType::Ssh { encoding, .. } => assert_eq!(encoding, ""),
        other => panic!("expected SSH connection, got {other:?}"),
    }
    match &config.connections[1].config {
        ConnectionType::LocalTerminal { encoding, .. } => assert_eq!(encoding, ""),
        other => panic!("expected local connection, got {other:?}"),
    }
    match &config.connections[2].config {
        ConnectionType::Telnet {
            username, encoding, ..
        } => {
            assert_eq!(username, "");
            assert_eq!(encoding, "");
        }
        other => panic!("expected Telnet connection, got {other:?}"),
    }
    match &config.connections[3].config {
        ConnectionType::Serial { encoding, .. } => assert_eq!(encoding, ""),
        other => panic!("expected serial connection, got {other:?}"),
    }
}

#[test]
fn saved_connection_recording_override_round_trips() {
    let json = r#"{
            "id":"recording-override",
            "name":"Recorded SSH",
            "type":"ssh",
            "host":"example.com",
            "recording":{
                "auto_start":true,
                "mode":"raw",
                "path_template":"{session}.raw",
                "include_timestamps":false,
                "rotation":{"type":"size","max_bytes":1048576}
            }
        }"#;

    let connection: SavedConnection = serde_json::from_str(json).expect("valid connection");
    let recording = connection.recording.as_ref().expect("recording override");
    assert_eq!(recording.auto_start, Some(true));
    assert_eq!(recording.mode, Some(RecordingMode::Raw));
    assert_eq!(recording.path_template.as_deref(), Some("{session}.raw"));
    assert_eq!(recording.include_timestamps, Some(false));
    assert_eq!(
        recording.rotation,
        Some(RecordingRotationPolicy::Size {
            max_bytes: 1_048_576
        })
    );

    let round_trip: SavedConnection =
        serde_json::from_str(&serde_json::to_string(&connection).expect("serialize"))
            .expect("reload");
    assert_eq!(round_trip, connection);
}

#[test]
fn rdp_connection_defaults_and_endpoint_match_tauri_shape() {
    let json = r#"{
            "id":"rdp-1",
            "name":"Windows",
            "type":"rdp",
            "host":"192.168.1.20",
            "username":"Administrator"
        }"#;

    let connection: SavedConnection = serde_json::from_str(json).expect("valid connection");
    assert_eq!(connection.kind_label(), "RDP");
    assert_eq!(connection.endpoint(), "Administrator@192.168.1.20:3389");

    let ConnectionType::Rdp {
        port,
        domain,
        security,
        display,
        clipboard,
        reconnect,
        ..
    } = &connection.config
    else {
        panic!("expected RDP connection");
    };

    assert_eq!(*port, 3389);
    assert!(domain.is_empty());
    assert!(security.use_nla);
    assert_eq!(security.certificate_policy, "prompt");
    assert_eq!(display.mode, "fit-window");
    assert_eq!(display.width, 1920);
    assert_eq!(display.height, 1080);
    assert_eq!(display.color_depth, 32);
    assert_eq!(clipboard.mode, "text-only");
    assert!(reconnect.enabled);
    assert_eq!(reconnect.max_attempts, 5);
}

#[test]
fn vnc_connection_defaults_and_endpoint_match_tauri_shape() {
    let json = r#"{
            "id":"vnc-1",
            "name":"Remote X",
            "type":"vnc",
            "host":"192.168.1.30"
        }"#;

    let connection: SavedConnection = serde_json::from_str(json).expect("valid connection");
    assert_eq!(connection.kind_label(), "VNC");
    assert_eq!(connection.endpoint(), "192.168.1.30:5900");

    let ConnectionType::Vnc {
        port,
        security,
        display,
        clipboard,
        reconnect,
        shared,
        view_only,
        ..
    } = &connection.config
    else {
        panic!("expected VNC connection");
    };

    assert_eq!(*port, 5900);
    assert_eq!(security.mode, "auto");
    assert_eq!(display.scale_mode, "fit");
    assert!(clipboard.enabled);
    assert!(reconnect.enabled);
    assert_eq!(reconnect.max_attempts, 5);
    assert!(*shared);
    assert!(!*view_only);

    let round_trip: SavedConnection =
        serde_json::from_str(&serde_json::to_string(&connection).expect("serialize"))
            .expect("reload");
    assert_eq!(round_trip, connection);
}

#[test]
fn tauri_ssh_algorithm_sftp_and_encoding_fields_round_trip() {
    let json = r#"{
            "id":"ssh-tauri",
            "name":"SSH",
            "type":"ssh",
            "host":"example.com",
            "port":2222,
            "username":"deploy",
            "encoding":"GBK",
            "ssh_algorithms":{
                "mode":"custom",
                "kex":["curve25519-sha256"],
                "ciphers":["aes128-ctr"],
                "macs":["hmac-sha2-256"],
                "host_keys":["ssh-ed25519"]
            },
            "sftp":{
                "enabled":false,
                "cwd_follow_mode":"rc_file",
                "shell_detection_timeout_ms":5000,
                "filename_encoding":"GB18030"
            }
        }"#;

    let connection: SavedConnection = serde_json::from_str(json).expect("valid connection");

    match &connection.config {
        ConnectionType::Ssh { encoding, .. } => assert_eq!(encoding, "GBK"),
        other => panic!("expected SSH connection, got {other:?}"),
    }
    assert_eq!(
        connection.ssh_algorithms,
        Some(SshAlgorithmPreferences {
            mode: SshAlgorithmMode::Custom,
            kex: vec!["curve25519-sha256".to_string()],
            ciphers: vec!["aes128-ctr".to_string()],
            macs: vec!["hmac-sha2-256".to_string()],
            host_keys: vec!["ssh-ed25519".to_string()],
        })
    );
    assert_eq!(
        connection.sftp,
        SftpSettings {
            enabled: false,
            cwd_follow_mode: SftpCwdFollowMode::RcFile,
            shell_detection_timeout_ms: 5000,
            filename_encoding: "GB18030".to_string(),
        }
    );

    let round_trip: SavedConnection =
        serde_json::from_str(&serde_json::to_string(&connection).expect("serialize"))
            .expect("reload");
    assert_eq!(round_trip, connection);
}

#[test]
fn tauri_telnet_username_auth_and_encoding_fields_round_trip() {
    let json = r#"{
            "id":"telnet-tauri",
            "name":"Telnet",
            "type":"telnet",
            "host":"10.0.0.9",
            "port":23,
            "username":"operator",
            "encoding":"GB18030",
            "local_echo":true,
            "local_line_edit":true,
            "auth":{
                "mode":"password",
                "password":"secret"
            }
        }"#;

    let connection: SavedConnection = serde_json::from_str(json).expect("valid connection");

    match &connection.config {
        ConnectionType::Telnet {
            username,
            encoding,
            local_echo,
            local_line_edit,
            ..
        } => {
            assert_eq!(username, "operator");
            assert_eq!(encoding, "GB18030");
            assert!(*local_echo);
            assert!(*local_line_edit);
        }
        other => panic!("expected Telnet connection, got {other:?}"),
    }
    assert_eq!(
        connection
            .auth
            .as_ref()
            .and_then(|auth| auth.password.as_deref()),
        Some("secret")
    );

    let round_trip: SavedConnection =
        serde_json::from_str(&serde_json::to_string(&connection).expect("serialize"))
            .expect("reload");
    assert_eq!(round_trip, connection);
}

#[test]
fn validates_sftp_shell_detection_timeout_range() {
    for value in [
        MIN_SFTP_SHELL_DETECTION_TIMEOUT_MS,
        default_sftp_shell_detection_timeout_ms(),
        MAX_SFTP_SHELL_DETECTION_TIMEOUT_MS,
    ] {
        let settings = SftpSettings {
            shell_detection_timeout_ms: value,
            ..Default::default()
        };
        assert!(validate_sftp_settings(&settings).is_ok());
    }

    for value in [
        MIN_SFTP_SHELL_DETECTION_TIMEOUT_MS - 1,
        MAX_SFTP_SHELL_DETECTION_TIMEOUT_MS + 1,
    ] {
        let settings = SftpSettings {
            shell_detection_timeout_ms: value,
            ..Default::default()
        };
        assert!(validate_sftp_settings(&settings).is_err());
    }
}

#[test]
fn local_terminal_endpoint_uses_shell_and_working_dir() {
    let connection = SavedConnection {
        id: "local-1".to_string(),
        name: "Local".to_string(),
        config: ConnectionType::LocalTerminal {
            shell_path: "zsh".to_string(),
            shell_args: String::new(),
            working_dir: Some("/data".to_string()),
            ai_execution_profile: AiExecutionProfile::Auto,
            encoding: String::new(),
        },
        group_id: None,
        description: None,
        sort_order: 0,
        icon: None,
        icon_auto_detect: None,
        auth: None,
        ssh_algorithms: None,
        ssh_profile: Default::default(),
        terminal_type: None,
        sftp: Default::default(),
        network: None,
        post_login: None,
        recording: None,
        created_at_ms: None,
        updated_at_ms: None,
        last_used_at_ms: None,
    };

    assert_eq!(connection.endpoint(), "zsh in /data");
}

#[test]
fn icon_auto_detect_defaults_to_filling_in_a_blank_only() {
    let mut connection: SavedConnection = serde_json::from_str(
        r#"{"id":"c1","name":"Box","type":"ssh","host":"h","port":22,"username":"root"}"#,
    )
    .expect("valid connection");

    // Unset flag, no icon: detection may fill one in.
    assert!(connection.icon_auto_detect_enabled());

    // Unset flag but an icon chosen by an older build: leave it alone.
    connection.icon = Some("ubuntu".to_string());
    assert!(!connection.icon_auto_detect_enabled());

    // An explicit flag always wins over the heuristic, either way.
    connection.icon_auto_detect = Some(true);
    assert!(connection.icon_auto_detect_enabled());
    connection.icon = None;
    connection.icon_auto_detect = Some(false);
    assert!(!connection.icon_auto_detect_enabled());
}

#[test]
fn icon_auto_detect_round_trips_and_stays_absent_when_unset() {
    let connection: SavedConnection = serde_json::from_str(
        r#"{"id":"c1","name":"Box","type":"ssh","host":"h","port":22,"username":"root"}"#,
    )
    .expect("valid connection");

    // Files written by builds predating the field must round-trip byte-for-
    // byte, so an unset flag is never serialized.
    let json = serde_json::to_string(&connection).expect("serializes");
    assert!(!json.contains("icon_auto_detect"), "{json}");

    let explicit = SavedConnection {
        icon_auto_detect: Some(false),
        ..connection
    };
    let reloaded: SavedConnection =
        serde_json::from_str(&serde_json::to_string(&explicit).expect("serializes"))
            .expect("reloads");
    assert_eq!(reloaded.icon_auto_detect, Some(false));
}

#[test]
fn restorable_open_tab_lock_is_backward_compatible_and_sparse() {
    let legacy = r#"{
            "title":"Local",
            "session_type":"Local",
            "connection_id":null,
            "custom_name":null,
            "tab_color":null,
            "active_pane_id":null,
            "root":null
        }"#;
    let tab: RestorableOpenTab = serde_json::from_str(legacy).expect("legacy tab loads");
    assert!(!tab.locked);
    let json = serde_json::to_string(&tab).expect("tab serializes");
    assert!(!json.contains("locked"), "{json}");

    let locked = RestorableOpenTab {
        locked: true,
        ..tab
    };
    let json = serde_json::to_string(&locked).expect("locked tab serializes");
    assert!(json.contains(r#""locked":true"#), "{json}");
    let reloaded: RestorableOpenTab = serde_json::from_str(&json).expect("locked tab reloads");
    assert!(reloaded.locked);
}

#[test]
fn legacy_ssh_profile_defaults_without_rewriting_sparse_json() {
    let json = r#"{
            "id":"legacy-ssh",
            "name":"Legacy SSH",
            "type":"ssh",
            "host":"example.com"
        }"#;
    let connection: SavedConnection = serde_json::from_str(json).expect("legacy SSH loads");
    assert_eq!(connection.ssh_profile, SshProfile::Standard);
    assert_eq!(connection.terminal_type, None);
    assert_eq!(
        resolve_ssh_terminal_type(connection.ssh_profile, connection.terminal_type),
        SshTerminalType::Xterm256Color
    );
    let serialized = serde_json::to_string(&connection).expect("serialize");
    assert!(!serialized.contains("ssh_profile"), "{serialized}");
    assert!(!serialized.contains("terminal_type"), "{serialized}");
    assert!(!serialized.contains("agent_endpoint"), "{serialized}");
    assert!(!serialized.contains("agent_forwarding"), "{serialized}");
}

#[test]
fn ssh_agent_endpoint_and_forwarding_round_trip_without_secrets() {
    let json = r#"{
            "id":"agent-ssh","name":"Agent SSH","type":"ssh","host":"example.com",
            "auth":{"mode":"agent"},
            "agent_endpoint":{"type":"unix_socket","path":"/run/user/1000/agent.sock"},
            "agent_forwarding":true
        }"#;
    let connection: SavedConnection = serde_json::from_str(json).expect("agent SSH loads");
    assert_eq!(
        connection.auth.as_ref().map(|auth| auth.mode.as_str()),
        Some("agent")
    );
    let ConnectionType::Ssh {
        agent_endpoint,
        agent_forwarding,
        ..
    } = &connection.config
    else {
        panic!("SSH expected");
    };
    assert_eq!(
        agent_endpoint,
        &SshAgentEndpoint::UnixSocket {
            path: "/run/user/1000/agent.sock".to_string()
        }
    );
    assert!(*agent_forwarding);
    let serialized = serde_json::to_string(&connection).expect("agent SSH serializes");
    assert!(serialized.contains("unix_socket"), "{serialized}");
    assert!(serialized.contains("agent_forwarding"), "{serialized}");
}

#[test]
fn network_device_profile_and_explicit_terminal_round_trip() {
    let json = r#"{
            "id":"switch",
            "name":"Core switch",
            "type":"ssh",
            "host":"10.0.0.2",
            "ssh_profile":"network_device"
        }"#;
    let mut connection: SavedConnection = serde_json::from_str(json).expect("profile loads");
    assert_eq!(connection.ssh_profile, SshProfile::NetworkDevice);
    assert_eq!(
        resolve_ssh_terminal_type(connection.ssh_profile, connection.terminal_type),
        SshTerminalType::Vt100
    );
    connection.terminal_type = Some(SshTerminalType::Ansi);
    assert_eq!(
        resolve_ssh_terminal_type(connection.ssh_profile, connection.terminal_type),
        SshTerminalType::Ansi
    );
    let reloaded: SavedConnection =
        serde_json::from_str(&serde_json::to_string(&connection).expect("serialize profile"))
            .expect("reload profile");
    assert_eq!(reloaded, connection);
}

fn quick_command(id: &str, category: Option<&str>, pinned: bool, order: i32) -> QuickCommand {
    QuickCommand {
        id: id.to_string(),
        label: id.to_string(),
        command: id.to_string(),
        category_id: category.map(ToString::to_string),
        description: None,
        color_tag: None,
        icon_tag: None,
        pinned: pinned.then_some(true),
        execution_mode: None,
        source: None,
        risk_level: None,
        updated_at: None,
        created_at: None,
        use_count: None,
        sort_order: Some(order),
    }
}

fn quick_category(id: &str, parent: Option<&str>, order: i32) -> QuickCommandCategory {
    QuickCommandCategory {
        id: id.to_string(),
        name: id.to_string(),
        parent_id: parent.map(ToString::to_string),
        sort_order: order,
    }
}

#[test]
fn quick_command_reorder_adopts_target_partition_and_normalizes_order() {
    let mut config = QuickCommandsConfig {
        commands: vec![
            quick_command("a", Some("one"), false, 0),
            quick_command("b", Some("two"), true, 0),
            quick_command("c", Some("two"), true, 1),
        ],
        categories: vec![],
    };
    assert!(config.reorder_command_relative("a", "c", QuickCommandRelativePosition::Before));
    let a = config
        .commands
        .iter()
        .find(|item| item.id == "a")
        .expect("a");
    assert_eq!(a.category_id.as_deref(), Some("two"));
    assert!(a.pinned.unwrap_or_default());
    assert_eq!(a.sort_order, Some(1));
    assert_eq!(
        config
            .commands
            .iter()
            .find(|item| item.id == "c")
            .and_then(|item| item.sort_order),
        Some(2)
    );
}

#[test]
fn category_sibling_order_follows_sort_order_then_name() {
    let categories = vec![
        quick_category("zeta", None, 0),
        quick_category("alpha", None, 0),
        quick_category("later", None, 5),
        quick_category("nested", Some("alpha"), 0),
    ];
    let roots = quick_command_category_sibling_order(&categories, None)
        .into_iter()
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    // Equal sort_order falls back to name, so "alpha" precedes "zeta".
    assert_eq!(roots, vec!["alpha", "zeta", "later"]);
    let children = quick_command_category_sibling_order(&categories, Some("alpha"))
        .into_iter()
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(children, vec!["nested"]);
}

#[test]
fn category_sibling_order_treats_orphaned_parent_as_root() {
    let categories = vec![
        quick_category("kept", None, 0),
        quick_category("orphan", Some("missing"), 1),
    ];
    let roots = quick_command_category_sibling_order(&categories, None)
        .into_iter()
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(roots, vec!["kept", "orphan"]);
    assert!(quick_command_category_sibling_order(&categories, Some("missing")).is_empty());
}

#[test]
fn category_move_neighbor_is_none_at_each_end_of_a_sibling_run() {
    let categories = vec![
        quick_category("first", None, 0),
        quick_category("middle", None, 1),
        quick_category("last", None, 2),
        quick_category("child", Some("first"), 0),
    ];
    assert_eq!(
        quick_command_category_move_neighbor(&categories, "first", true),
        None
    );
    assert_eq!(
        quick_command_category_move_neighbor(&categories, "last", false),
        None
    );
    assert_eq!(
        quick_command_category_move_neighbor(&categories, "middle", true).as_deref(),
        Some("first")
    );
    assert_eq!(
        quick_command_category_move_neighbor(&categories, "middle", false).as_deref(),
        Some("last")
    );
    // An only child cannot move within its own parent.
    assert_eq!(
        quick_command_category_move_neighbor(&categories, "child", true),
        None
    );
    assert_eq!(
        quick_command_category_move_neighbor(&categories, "child", false),
        None
    );
    assert_eq!(
        quick_command_category_move_neighbor(&categories, "absent", true),
        None
    );
}

#[test]
fn category_move_up_and_down_swap_adjacent_siblings() {
    let mut config = QuickCommandsConfig {
        commands: vec![],
        categories: vec![
            quick_category("first", None, 0),
            quick_category("middle", None, 1),
            quick_category("last", None, 2),
        ],
    };

    let order = |config: &QuickCommandsConfig| {
        quick_command_category_sibling_order(&config.categories, None)
            .into_iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>()
    };

    let target = quick_command_category_move_neighbor(&config.categories, "last", true)
        .expect("last has a neighbor above");
    assert!(config.move_category("last", &target, QuickCommandCategoryPosition::Before));
    assert_eq!(order(&config), vec!["first", "last", "middle"]);

    let target = quick_command_category_move_neighbor(&config.categories, "first", false)
        .expect("first has a neighbor below");
    assert!(config.move_category("first", &target, QuickCommandCategoryPosition::After));
    assert_eq!(order(&config), vec!["last", "first", "middle"]);

    // Round trip restores the original order.
    let target = quick_command_category_move_neighbor(&config.categories, "last", false)
        .expect("last is no longer at the bottom");
    assert!(config.move_category("last", &target, QuickCommandCategoryPosition::After));
    assert_eq!(order(&config), vec!["first", "last", "middle"]);
}

#[test]
fn category_move_rejects_descendant_cycles_and_normalizes_siblings() {
    let mut config = QuickCommandsConfig {
        commands: vec![],
        categories: vec![
            quick_category("root", None, 0),
            quick_category("child", Some("root"), 0),
            quick_category("peer", None, 1),
        ],
    };
    assert!(!config.move_category("root", "child", QuickCommandCategoryPosition::Inside));
    assert!(config.move_category("child", "peer", QuickCommandCategoryPosition::Before));
    let child = config
        .categories
        .iter()
        .find(|item| item.id == "child")
        .expect("child");
    assert_eq!(child.parent_id, None);
    assert_eq!(child.sort_order, 1);
    assert_eq!(
        config
            .categories
            .iter()
            .find(|item| item.id == "peer")
            .map(|item| item.sort_order),
        Some(2)
    );
}
