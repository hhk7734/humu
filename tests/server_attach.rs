#[path = "../src/server/mod.rs"]
#[allow(dead_code)]
mod server_impl;
mod support;

use humu::id::PaneId;
use humu::shared::protocol::{
    ClientAction, ClientRequest, FrameDecoder, NavigationDirection, ServerEvent, ServerResponse,
    SessionListEntry, decode_frame, encode_frame,
};
use humu::shared::render::{ColorSnapshot, DetachReason, FullSnapshot};
use serde::de::DeserializeOwned;
use serde_json::json;
use server_impl::session::{AttachError, AttachOwner, SessionManager};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::Command;
use std::time::{Duration, Instant};
use uuid::Uuid;

fn pane_id(raw: &str) -> PaneId {
    PaneId(Uuid::parse_str(raw).unwrap())
}

fn read_framed_message<T: DeserializeOwned>(stream: &mut UnixStream) -> anyhow::Result<T> {
    let mut decoder = FrameDecoder::new();
    let mut buf = [0u8; 4096];
    loop {
        let read = stream.read(&mut buf)?;
        if read == 0 {
            anyhow::bail!("stream closed before a full frame was received");
        }
        decoder.push(&buf[..read]);
        if let Some(message) = decoder.try_decode()? {
            return Ok(message);
        }
    }
}

fn send_request<T: DeserializeOwned>(
    env: &support::TestEnv,
    request: &ClientRequest,
) -> anyhow::Result<T> {
    let mut stream = connect_server(env)?;
    send_request_on_stream(&mut stream, request)
}

fn connect_server(env: &support::TestEnv) -> anyhow::Result<UnixStream> {
    Ok(UnixStream::connect(env.server_socket_path())?)
}

fn send_request_on_stream<T: DeserializeOwned>(
    stream: &mut UnixStream,
    request: &ClientRequest,
) -> anyhow::Result<T> {
    stream.write_all(&encode_frame(request)?)?;
    read_framed_message(stream)
}

fn ping_server(env: &support::TestEnv) -> anyhow::Result<ServerResponse> {
    send_request(env, &ClientRequest::Ping)
}

fn wait_for_ping(env: &support::TestEnv, timeout: Duration) -> anyhow::Result<ServerResponse> {
    let deadline = Instant::now() + timeout;
    loop {
        match ping_server(env) {
            Ok(response) => return Ok(response),
            Err(err) if Instant::now() < deadline => {
                let _ = err;
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(err) => return Err(err),
        }
    }
}

fn write_stale_server_files(env: &support::TestEnv) {
    let _ = fs::remove_file(env.server_socket_path());
    let listener = UnixListener::bind(env.server_socket_path()).expect("bind stale socket");
    drop(listener);

    let metadata = json!({
        "pid": u32::MAX,
        "started_at": 1u64,
        "socket_path": env.server_socket_path(),
        "protocol_version": humu::shared::protocol::PROTOCOL_VERSION,
    });
    fs::write(
        env.server_metadata_path(),
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .expect("write stale metadata");
}

fn spawn_version_mismatched_server(
    env: &support::TestEnv,
) -> std::thread::JoinHandle<anyhow::Result<()>> {
    let _ = fs::remove_file(env.server_socket_path());
    let listener = UnixListener::bind(env.server_socket_path()).expect("bind fake server socket");
    let metadata = json!({
        "pid": std::process::id(),
        "started_at": 1u64,
        "socket_path": env.server_socket_path(),
        "protocol_version": humu::shared::protocol::PROTOCOL_VERSION + 1,
    });
    fs::write(
        env.server_metadata_path(),
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .expect("write fake metadata");

    std::thread::spawn(move || -> anyhow::Result<()> {
        let (mut stream, _) = listener.accept()?;
        let _: ClientRequest = read_framed_message(&mut stream)?;
        stream.write_all(&encode_frame(&ServerResponse::Pong {
            protocol_version: humu::shared::protocol::PROTOCOL_VERSION + 1,
        })?)?;
        Ok(())
    })
}

#[test]
fn support_can_spawn_isolated_humu_home() {
    let env = support::isolated_humu_home();
    assert!(env.home.path().exists());
    assert!(env.humu_dir().exists());
    assert_ne!(env.home.path(), env.humu_dir());
    assert!(!env.humu_dir().starts_with(env.home.path()));
}

// Linux-only: verifies scoped process cleanup via pid liveness.
#[cfg(target_os = "linux")]
#[test]
fn support_scopes_background_process_cleanup() {
    let _: fn(&support::TestEnv) -> support::ScopedChild = support::spawn_humu_server;

    let pid = {
        let mut command = Command::new("bash");
        command.arg("-lc").arg("sleep 60");
        let child = support::spawn_scoped_command(command);
        let pid = child.process_id().expect("scoped child pid");
        assert!(support::process_is_alive(pid));
        pid
    };

    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(!support::process_is_alive(pid));
}

// Linux-only: inspects procfs for cwd and validates server runtime artifacts land
// in the isolated humu home used by the helper.
#[cfg(target_os = "linux")]
#[test]
fn support_spawn_humu_server_applies_isolated_stdio_contract() {
    let env = support::isolated_humu_home();
    let mut child = support::spawn_humu_server(&env);
    let pid = child.process_id().expect("server helper pid");

    assert!(child.stdin().is_none());
    assert!(child.stdout().is_some());
    assert!(child.stderr().is_some());

    let cwd = std::fs::read_link(format!("/proc/{pid}/cwd")).expect("server cwd");
    assert_eq!(cwd, env.cwd());
    let stdin_target = std::fs::read_link(format!("/proc/{pid}/fd/0")).expect("server stdin fd");
    assert_eq!(stdin_target, std::path::PathBuf::from("/dev/null"));

    let hook_file = env.humu_dir().join("hooks/claude-settings.json");
    let log_file = env.log_path();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if hook_file.exists() && log_file.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    assert!(
        hook_file.exists(),
        "expected isolated hook file at {:?}",
        hook_file
    );
    assert!(
        log_file.exists(),
        "expected isolated log file at {:?}",
        log_file
    );
    assert!(env.hook_port_path().starts_with(env.humu_dir()));
    assert!(env.server_socket_path().starts_with(env.humu_dir()));
    assert!(env.server_lock_path().starts_with(env.humu_dir()));
    assert!(env.server_metadata_path().starts_with(env.humu_dir()));
}

#[test]
fn server_ping_works_after_daemon_start() {
    let env = support::isolated_humu_home();
    let _child = support::spawn_humu_server(&env);

    let response = wait_for_ping(&env, Duration::from_secs(5)).expect("daemon ping");
    assert_eq!(
        response,
        ServerResponse::Pong {
            protocol_version: humu::shared::protocol::PROTOCOL_VERSION,
        }
    );

    let metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(env.server_metadata_path()).expect("read metadata"))
            .expect("parse metadata");
    assert_eq!(
        metadata["protocol_version"],
        humu::shared::protocol::PROTOCOL_VERSION
    );
    assert_eq!(
        metadata["socket_path"],
        env.server_socket_path().to_string_lossy().as_ref()
    );
}

#[test]
fn stale_socket_and_metadata_are_cleaned_when_pid_is_dead() {
    let env = support::isolated_humu_home();
    write_stale_server_files(&env);

    let _child = support::spawn_humu_server(&env);
    let response =
        wait_for_ping(&env, Duration::from_secs(5)).expect("daemon ping after stale cleanup");
    assert_eq!(
        response,
        ServerResponse::Pong {
            protocol_version: humu::shared::protocol::PROTOCOL_VERSION,
        }
    );

    let metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(env.server_metadata_path()).expect("read metadata"))
            .expect("parse metadata");
    assert_ne!(metadata["pid"], u32::MAX);
}

#[test]
fn session_manager_enforces_idempotent_create_and_single_client_attach_lock() {
    let mut manager = SessionManager::default();

    manager.create("default");
    manager.create("default");
    assert_eq!(manager.list().len(), 1);

    let owner_a = AttachOwner::new("client-a")
        .with_pid(4242)
        .with_attached_at("2026-03-26T10:15:00Z");
    manager
        .attach("default", owner_a.clone())
        .expect("first attach succeeds");
    manager
        .attach("default", owner_a)
        .expect("same owner reattach is idempotent");

    match manager.attach("default", AttachOwner::new("client-b").with_pid(5252)) {
        Err(AttachError::AlreadyAttached {
            session_name,
            owner_pid,
            attached_at,
        }) => {
            assert_eq!(session_name, "default");
            assert_eq!(owner_pid, Some(4242));
            assert_eq!(attached_at.as_deref(), Some("2026-03-26T10:15:00Z"));
        }
        other => panic!("unexpected attach result: {other:?}"),
    }
}

#[test]
fn list_sessions_refuses_protocol_version_mismatch() {
    let env = support::isolated_humu_home();
    let handle = spawn_version_mismatched_server(&env);

    let status = support::humu_command(&env)
        .arg("list-sessions")
        .status()
        .expect("run humu list-sessions");
    assert!(!status.success());

    handle
        .join()
        .expect("fake server thread")
        .expect("fake server run");
}

#[test]
fn attach_fallback_rejects_named_sessions() {
    let env = support::isolated_humu_home();
    let output = support::humu_command(&env)
        .arg("attach")
        .arg("review")
        .output()
        .expect("run humu attach review");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("only supports the default session"));
}

#[test]
fn server_startup_refuses_protocol_version_mismatch() {
    let env = support::isolated_humu_home();
    let handle = spawn_version_mismatched_server(&env);

    let output = support::humu_command(&env)
        .arg("server")
        .arg("--daemon")
        .output()
        .expect("run humu server --daemon");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("protocol version mismatch"));

    handle
        .join()
        .expect("fake server thread")
        .expect("fake server run");
}

#[test]
fn daemon_attach_rejects_second_connection_for_same_session() {
    let env = support::isolated_humu_home();
    let _child = support::spawn_humu_server(&env);
    wait_for_ping(&env, Duration::from_secs(5)).expect("daemon ping");

    let mut first = connect_server(&env).expect("first stream");
    let first_response = send_request_on_stream::<ServerResponse>(
        &mut first,
        &ClientRequest::AttachSession {
            name: "default".to_string(),
            cols: 120,
            rows: 40,
        },
    )
    .expect("first attach response");
    assert!(matches!(first_response, ServerResponse::Attached { .. }));

    let second = send_request::<ServerResponse>(
        &env,
        &ClientRequest::AttachSession {
            name: "default".to_string(),
            cols: 120,
            rows: 40,
        },
    )
    .expect("second attach response");
    match second {
        ServerResponse::AlreadyAttached {
            session_name,
            owner_pid,
            ..
        } => {
            assert_eq!(session_name, "default");
            assert_eq!(owner_pid, Some(std::process::id()));
        }
        other => panic!("unexpected second attach response: {other:?}"),
    }
}

#[test]
fn rejected_attach_keeps_existing_session_lock() {
    let env = support::isolated_humu_home();
    let _child = support::spawn_humu_server(&env);
    wait_for_ping(&env, Duration::from_secs(5)).expect("daemon ping");

    let mut alpha_owner = connect_server(&env).expect("alpha owner stream");
    let alpha_attach = send_request_on_stream::<ServerResponse>(
        &mut alpha_owner,
        &ClientRequest::AttachSession {
            name: "alpha".to_string(),
            cols: 80,
            rows: 24,
        },
    )
    .expect("attach alpha");
    assert!(matches!(alpha_attach, ServerResponse::Attached { .. }));

    let mut beta_owner = connect_server(&env).expect("beta owner stream");
    let beta_attach = send_request_on_stream::<ServerResponse>(
        &mut beta_owner,
        &ClientRequest::AttachSession {
            name: "beta".to_string(),
            cols: 100,
            rows: 30,
        },
    )
    .expect("attach beta");
    assert!(matches!(beta_attach, ServerResponse::Attached { .. }));

    let rejected = send_request_on_stream::<ServerResponse>(
        &mut alpha_owner,
        &ClientRequest::AttachSession {
            name: "beta".to_string(),
            cols: 120,
            rows: 40,
        },
    )
    .expect("rejected attach");
    assert!(matches!(rejected, ServerResponse::AlreadyAttached { .. }));

    let alpha_retry = send_request::<ServerResponse>(
        &env,
        &ClientRequest::AttachSession {
            name: "alpha".to_string(),
            cols: 90,
            rows: 28,
        },
    )
    .expect("retry alpha");
    assert!(matches!(
        alpha_retry,
        ServerResponse::AlreadyAttached {
            session_name,
            ..
        } if session_name == "alpha"
    ));
}

#[test]
fn rejected_attach_does_not_overwrite_target_session_size() {
    let env = support::isolated_humu_home();
    let _child = support::spawn_humu_server(&env);
    wait_for_ping(&env, Duration::from_secs(5)).expect("daemon ping");

    let mut alpha_owner = connect_server(&env).expect("alpha owner stream");
    let alpha_attach = send_request_on_stream::<ServerResponse>(
        &mut alpha_owner,
        &ClientRequest::AttachSession {
            name: "alpha".to_string(),
            cols: 80,
            rows: 24,
        },
    )
    .expect("attach alpha");
    assert!(matches!(alpha_attach, ServerResponse::Attached { .. }));

    let mut beta_owner = connect_server(&env).expect("beta owner stream");
    let beta_attach = send_request_on_stream::<ServerResponse>(
        &mut beta_owner,
        &ClientRequest::AttachSession {
            name: "beta".to_string(),
            cols: 100,
            rows: 30,
        },
    )
    .expect("attach beta");
    assert!(matches!(beta_attach, ServerResponse::Attached { .. }));

    let rejected = send_request_on_stream::<ServerResponse>(
        &mut alpha_owner,
        &ClientRequest::AttachSession {
            name: "beta".to_string(),
            cols: 120,
            rows: 40,
        },
    )
    .expect("rejected attach");
    assert!(matches!(rejected, ServerResponse::AlreadyAttached { .. }));

    let sessions =
        send_request::<ServerResponse>(&env, &ClientRequest::ListSessions).expect("list sessions");
    match sessions {
        ServerResponse::Sessions { sessions } => {
            let beta = sessions
                .into_iter()
                .find(|session| session.name == "beta")
                .expect("beta session");
            let size = beta.last_size.expect("beta last size");
            assert_eq!(size.cols, 100);
            assert_eq!(size.rows, 30);
        }
        other => panic!("unexpected sessions response: {other:?}"),
    }
}

#[test]
fn daemon_disconnect_releases_session_lock() {
    let env = support::isolated_humu_home();
    let _child = support::spawn_humu_server(&env);
    wait_for_ping(&env, Duration::from_secs(5)).expect("daemon ping");

    {
        let mut first = connect_server(&env).expect("first stream");
        let first_attach = send_request_on_stream::<ServerResponse>(
            &mut first,
            &ClientRequest::AttachSession {
                name: "default".to_string(),
                cols: 120,
                rows: 40,
            },
        )
        .expect("first attach");
        assert!(matches!(first_attach, ServerResponse::Attached { .. }));
    }

    let second = send_request::<ServerResponse>(
        &env,
        &ClientRequest::AttachSession {
            name: "default".to_string(),
            cols: 120,
            rows: 40,
        },
    )
    .expect("attach after disconnect");
    assert!(matches!(second, ServerResponse::Attached { .. }));
}

#[test]
fn daemon_detach_request_releases_session_lock() {
    let env = support::isolated_humu_home();
    let _child = support::spawn_humu_server(&env);
    wait_for_ping(&env, Duration::from_secs(5)).expect("daemon ping");

    let mut first = connect_server(&env).expect("first stream");
    let first_attach = send_request_on_stream::<ServerResponse>(
        &mut first,
        &ClientRequest::AttachSession {
            name: "default".to_string(),
            cols: 120,
            rows: 40,
        },
    )
    .expect("first attach");
    assert!(matches!(first_attach, ServerResponse::Attached { .. }));

    let detach = send_request_on_stream::<ServerResponse>(&mut first, &ClientRequest::Detach)
        .expect("detach response");
    assert!(matches!(
        detach,
        ServerResponse::Detached { ref session_name } if session_name == "default"
    ));

    let second = send_request::<ServerResponse>(
        &env,
        &ClientRequest::AttachSession {
            name: "default".to_string(),
            cols: 120,
            rows: 40,
        },
    )
    .expect("attach after detach");
    assert!(matches!(second, ServerResponse::Attached { .. }));
}

#[test]
fn daemonized_server_command_returns_after_background_startup() {
    let env = support::isolated_humu_home();
    let mut command = support::humu_command(&env);
    command.arg("server").arg("--daemon");

    let status = command.status().expect("run daemonized server");
    assert!(status.success());

    let response = wait_for_ping(&env, Duration::from_secs(5)).expect("daemon ping");
    assert_eq!(
        response,
        ServerResponse::Pong {
            protocol_version: humu::shared::protocol::PROTOCOL_VERSION,
        }
    );

    let metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(env.server_metadata_path()).expect("read metadata"))
            .expect("parse metadata");
    let daemon_pid = metadata["pid"].as_u64().expect("daemon pid") as u32;
    assert_ne!(daemon_pid, std::process::id());
    assert!(support::process_is_alive(daemon_pid));

    let _ = Command::new("kill").arg(daemon_pid.to_string()).status();
}

#[test]
fn list_sessions_reports_owner_pid_for_attached_session() {
    let env = support::isolated_humu_home();
    let _child = support::spawn_humu_server(&env);
    wait_for_ping(&env, Duration::from_secs(5)).expect("daemon ping");

    let mut first = connect_server(&env).expect("first stream");
    let attach = send_request_on_stream::<ServerResponse>(
        &mut first,
        &ClientRequest::AttachSession {
            name: "default".to_string(),
            cols: 120,
            rows: 40,
        },
    )
    .expect("attach response");
    assert!(matches!(attach, ServerResponse::Attached { .. }));

    let response = send_request::<ServerResponse>(&env, &ClientRequest::ListSessions)
        .expect("list sessions response");
    match response {
        ServerResponse::Sessions { sessions } => {
            let default = sessions
                .into_iter()
                .find(|session| session.name == "default")
                .expect("default session present");
            assert_eq!(default.owner_pid, Some(std::process::id()));
        }
        other => panic!("unexpected list-sessions response: {other:?}"),
    }

    let output = support::humu_command(&env)
        .arg("list-sessions")
        .output()
        .expect("run list-sessions");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&std::process::id().to_string()));
}

#[test]
fn client_request_round_trips_with_all_core_variants() {
    let requests = vec![
        ClientRequest::Ping,
        ClientRequest::ListSessions,
        ClientRequest::CreateSession {
            name: "default".to_string(),
        },
        ClientRequest::AttachSession {
            name: "default".to_string(),
            cols: 120,
            rows: 40,
        },
        ClientRequest::Detach,
        ClientRequest::ForceDetachSession {
            name: "default".to_string(),
        },
        ClientRequest::RegisterPane {
            pane_id: pane_id("33333333-3333-3333-3333-333333333333"),
            preset_name: "codex".to_string(),
            cwd: Some(std::path::PathBuf::from("/tmp/humu")),
            session_id: Some("agent-session".to_string()),
            started_at_unix_secs: 1_742_963_200,
        },
        ClientRequest::UnregisterPane {
            pane_id: pane_id("33333333-3333-3333-3333-333333333333"),
        },
        ClientRequest::SendInput {
            pane_id: pane_id("11111111-1111-1111-1111-111111111111"),
            bytes: b"ls -la\n".to_vec(),
        },
        ClientRequest::ResizeSession {
            cols: 180,
            rows: 52,
        },
        ClientRequest::RunAction {
            action: ClientAction::MoveFocus {
                direction: NavigationDirection::Right,
            },
        },
        ClientRequest::SubscribeUpdates,
        ClientRequest::FocusChanged { focused: true },
    ];

    for request in requests {
        let bytes = serde_json::to_vec(&request).unwrap();
        let decoded: ClientRequest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(request, decoded);
    }
}

#[test]
fn server_response_round_trips_machine_readable_variants() {
    let snapshot = FullSnapshot::fixture();
    let responses = vec![
        ServerResponse::Pong {
            protocol_version: humu::shared::protocol::PROTOCOL_VERSION,
        },
        ServerResponse::Sessions {
            sessions: vec![SessionListEntry {
                name: "default".to_string(),
                attached: true,
                owner_pid: Some(4242),
                attached_at: Some("2026-03-26T10:15:00Z".to_string()),
                last_size: Some(snapshot.session_geometry.clone().unwrap()),
            }],
        },
        ServerResponse::SessionCreated {
            session: SessionListEntry {
                name: "review".to_string(),
                attached: false,
                owner_pid: None,
                attached_at: None,
                last_size: None,
            },
        },
        ServerResponse::Attached {
            session_name: snapshot.session_name.clone(),
            snapshot: snapshot.clone(),
        },
        ServerResponse::Detached {
            session_name: snapshot.session_name.clone(),
        },
        ServerResponse::Subscribed {
            session_name: snapshot.session_name.clone(),
        },
        ServerResponse::Ack,
        ServerResponse::AlreadyAttached {
            session_name: "default".to_string(),
            owner_pid: Some(4242),
            attached_at: Some("2026-03-26T10:15:00Z".to_string()),
        },
        ServerResponse::VersionMismatch {
            client_protocol_version: 1,
            server_protocol_version: 2,
        },
        ServerResponse::Error {
            message: "boom".to_string(),
        },
    ];

    for response in responses {
        let bytes = serde_json::to_vec(&response).unwrap();
        let decoded: ServerResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(response, decoded);
    }
}

#[test]
fn server_event_round_trips_snapshot_and_incremental_variants() {
    let snapshot = FullSnapshot::fixture();
    let pane_id = *snapshot.panes.keys().next().unwrap();
    let pane = snapshot.panes.get(&pane_id).unwrap().clone();
    let mut pane_geometries = std::collections::HashMap::new();
    pane_geometries.insert(pane_id, pane.geometry.clone().unwrap());
    let events = vec![
        ServerEvent::FullSnapshot(snapshot.clone()),
        ServerEvent::PaneUpdated {
            pane_id,
            pane: pane.clone(),
        },
        ServerEvent::LayoutUpdated {
            tabs: snapshot.tabs.clone(),
            active_tab_index: snapshot.active_tab_index,
            split_tree: snapshot.split_tree.clone(),
            session_geometry: snapshot.session_geometry.clone(),
            focused_pane_id: snapshot.focused_pane_id,
            fullscreen_pane_id: snapshot.fullscreen_pane_id,
            pane_geometries,
        },
        ServerEvent::AgentStateUpdated {
            pane_id,
            agent_state: pane.agent_state.clone(),
        },
        ServerEvent::SessionMetadataUpdated {
            session_name: snapshot.session_name.clone(),
            active_workspace_id: snapshot.active_workspace_id,
            active_room_id: snapshot.active_room_id,
            explorer_root: snapshot.explorer_root.clone(),
            attached: true,
            client_focused: true,
            owner_pid: Some(4242),
            attached_at: Some("2026-03-26T10:15:00Z".to_string()),
            last_size: snapshot.session_geometry.clone(),
        },
        ServerEvent::Error {
            message: "oops".to_string(),
        },
        ServerEvent::Detached {
            session_name: snapshot.session_name.clone(),
            reason: DetachReason::ForceDetached,
        },
    ];

    for event in events {
        let bytes = serde_json::to_vec(&event).unwrap();
        let decoded: ServerEvent = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(event, decoded);
    }
}

#[test]
fn full_snapshot_exposes_all_spec_fields() {
    let snapshot = FullSnapshot::fixture();

    assert_eq!(snapshot.session_name, "default");
    assert!(snapshot.active_workspace_id.is_some());
    assert!(snapshot.active_room_id.is_some());
    assert!(!snapshot.tabs.is_empty());
    assert_eq!(snapshot.active_tab_index, Some(0));
    assert!(snapshot.split_tree.is_some());
    assert!(snapshot.session_geometry.is_some());
    assert!(snapshot.focused_pane_id.is_some());
    assert!(snapshot.fullscreen_pane_id.is_some());
    assert!(!snapshot.panes.is_empty());
    assert!(snapshot.explorer_root.is_some());

    let pane = snapshot
        .panes
        .get(&snapshot.focused_pane_id.expect("focused pane"))
        .unwrap();
    assert_eq!(pane.preset_name, "shell");
    assert!(pane.geometry.is_some());
    assert!(pane.capabilities.alternate_screen);
    assert!(pane.capabilities.bracketed_paste);
    assert!(pane.capabilities.mouse_protocol_mode.is_some());
    assert!(pane.capabilities.mouse_protocol_encoding.is_some());
    assert_eq!(pane.capabilities.scrollback_offset, 12);
    assert!(pane.agent_state.is_some());
    assert!(!pane.screen.cells.is_empty());

    let styled = &pane.screen.cells[0][0];
    assert_eq!(styled.text, "h");
    assert_eq!(styled.fg, ColorSnapshot::Rgb(12, 34, 56));
    assert_eq!(styled.bg, ColorSnapshot::Rgb(60, 60, 60));
    assert!(styled.bold);
    assert!(styled.dim);
    assert!(styled.italic);
    assert!(styled.underline);
    assert!(styled.inverse);
    assert!(styled.hidden);
    assert!(styled.strike);
}

#[test]
fn framed_wire_helpers_support_back_to_back_messages_on_one_stream() {
    let first = ClientRequest::Ping;
    let second = ClientRequest::FocusChanged { focused: true };

    let mut stream = encode_frame(&first).unwrap();
    stream.extend(encode_frame(&second).unwrap());

    let mut decoder = FrameDecoder::new();
    let split = stream.len() / 2;
    decoder.push(&stream[..split]);
    let decoded_first: ClientRequest = decoder.try_decode().unwrap().unwrap();
    assert_eq!(decoded_first, first);
    assert!(decoder.try_decode::<ClientRequest>().unwrap().is_none());

    decoder.push(&stream[split..]);
    let decoded_second: ClientRequest = decoder.try_decode().unwrap().unwrap();
    assert_eq!(decoded_second, second);
    assert!(decoder.try_decode::<ClientRequest>().unwrap().is_none());
}

#[test]
fn framed_wire_helpers_round_trip_single_response() {
    let response = ServerResponse::Pong {
        protocol_version: humu::shared::protocol::PROTOCOL_VERSION,
    };

    let bytes = encode_frame(&response).unwrap();
    let decoded: ServerResponse = decode_frame(&bytes).unwrap();
    assert_eq!(decoded, response);
}

#[test]
fn layout_updates_include_pane_geometry_changes() {
    let snapshot = FullSnapshot::fixture();
    let pane_id = snapshot.focused_pane_id.unwrap();
    let event = ServerEvent::LayoutUpdated {
        tabs: snapshot.tabs.clone(),
        active_tab_index: snapshot.active_tab_index,
        split_tree: snapshot.split_tree.clone(),
        session_geometry: snapshot.session_geometry.clone(),
        focused_pane_id: snapshot.focused_pane_id,
        fullscreen_pane_id: snapshot.fullscreen_pane_id,
        pane_geometries: std::collections::HashMap::from([(
            pane_id,
            humu::shared::render::PaneGeometrySnapshot {
                x: 5,
                y: 3,
                width: 77,
                height: 19,
            },
        )]),
    };

    let bytes = serde_json::to_vec(&event).unwrap();
    let decoded: ServerEvent = serde_json::from_slice(&bytes).unwrap();
    match decoded {
        ServerEvent::LayoutUpdated {
            pane_geometries, ..
        } => {
            let geometry = pane_geometries.get(&pane_id).unwrap();
            assert_eq!(geometry.x, 5);
            assert_eq!(geometry.width, 77);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
