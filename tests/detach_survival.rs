#[path = "../src/server/mod.rs"]
mod server_impl;

mod support;

use humu::config::{HumuConfig, NotificationsConfig};
use humu::id::PaneId;
use humu::shared::protocol::{ClientRequest, FrameDecoder, ServerResponse, encode_frame};
use humu::shared::render::{FullSnapshot, PaneRuntimeState, SessionGeometrySnapshot};
use serde::de::DeserializeOwned;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

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

fn send_request<T: DeserializeOwned>(
    env: &support::TestEnv,
    request: &ClientRequest,
) -> anyhow::Result<T> {
    let mut stream = connect_server(env)?;
    send_request_on_stream(&mut stream, request)
}

fn wait_for_ping(env: &support::TestEnv, timeout: Duration) -> anyhow::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        match send_request::<ServerResponse>(env, &ClientRequest::Ping) {
            Ok(ServerResponse::Pong { .. }) => return Ok(()),
            Ok(other) => anyhow::bail!("unexpected ping response: {other:?}"),
            Err(err) if Instant::now() < deadline => {
                let _ = err;
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(err) => return Err(err),
        }
    }
}

fn pane_id(raw: &str) -> PaneId {
    PaneId(Uuid::parse_str(raw).unwrap())
}

fn shell_config(command: &str, args: &[&str]) -> HumuConfig {
    let mut config = HumuConfig::default();
    let preset = config.presets.get_mut("shell").expect("shell preset");
    preset.command = command.to_string();
    preset.args = args.iter().map(|arg| arg.to_string()).collect();
    config
}

fn write_config(env: &support::TestEnv, config: &HumuConfig) {
    config.save(&env.config_path()).expect("write config");
}

fn snapshot_contains(snapshot: &FullSnapshot, needle: &str) -> bool {
    snapshot
        .panes
        .values()
        .any(|pane| pane.screen.contents().contains(needle))
}

#[test]
fn runtime_emits_snapshot_from_server_owned_terminal_state() {
    let env = support::isolated_humu_home();
    let config = shell_config("bash", &["-lc", "printf 'hello\\n'; sleep 60"]);
    let runtime = server_impl::runtime::SessionRuntime::start(
        env.humu_dir().to_path_buf(),
        config,
        NotificationsConfig::default(),
        env.home.path().join(".codex/sessions"),
    )
    .expect("start runtime");

    let pane = pane_id("11111111-1111-1111-1111-111111111111");
    runtime.attach_session("default");
    runtime
        .register_pane("default", pane, "shell", None, None, SystemTime::now())
        .expect("register runtime pane");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut snapshot = runtime.snapshot_for_session("default", FullSnapshot::fixture());
    while !snapshot_contains(&snapshot, "hello") {
        assert!(
            Instant::now() < deadline,
            "server-owned PTY output never appeared in the snapshot"
        );
        std::thread::sleep(Duration::from_millis(50));
        snapshot = runtime.snapshot_for_session("default", FullSnapshot::fixture());
    }
    let pane = snapshot.panes.values().next().expect("runtime pane");
    assert!(matches!(pane.state, PaneRuntimeState::Running));
    assert!(snapshot_contains(&snapshot, "hello"));
    assert_eq!(
        snapshot.session_geometry,
        Some(SessionGeometrySnapshot {
            cols: 80,
            rows: 24,
        })
    );
}

#[test]
fn client_disconnect_does_not_kill_session_pty() {
    let env = support::isolated_humu_home();
    write_config(
        &env,
        &shell_config(
            "bash",
            &["-lc", "cat; sleep 60"],
        ),
    );

    let _daemon = support::spawn_humu_server(&env);
    wait_for_ping(&env, Duration::from_secs(5)).expect("daemon ping");

    let pane = pane_id("22222222-2222-2222-2222-222222222222");
    let mut stream = connect_server(&env).expect("connect daemon");
    let attached = send_request_on_stream::<ServerResponse>(
        &mut stream,
        &ClientRequest::AttachSession {
            name: "default".to_string(),
            cols: 80,
            rows: 24,
        },
    )
    .expect("attach session");
    assert!(matches!(attached, ServerResponse::Attached { .. }));

    let registered = send_request_on_stream::<ServerResponse>(
        &mut stream,
        &ClientRequest::RegisterPane {
            pane_id: pane,
            preset_name: "shell".to_string(),
            cwd: None,
            session_id: None,
            started_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        },
    )
    .expect("register pane");
    assert!(matches!(registered, ServerResponse::Ack));

    let echoed = send_request_on_stream::<ServerResponse>(
        &mut stream,
        &ClientRequest::SendInput {
            pane_id: pane,
            bytes: b"world\r".to_vec(),
        },
    )
    .expect("send input");
    assert!(matches!(echoed, ServerResponse::Ack));

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let snapshot = match send_request_on_stream::<ServerResponse>(
            &mut stream,
            &ClientRequest::AttachSession {
                name: "default".to_string(),
                cols: 80,
                rows: 24,
            },
        )
        .expect("refresh attach snapshot")
        {
            ServerResponse::Attached { snapshot, .. } => snapshot,
            other => panic!("unexpected attach response: {other:?}"),
        };
        if snapshot_contains(&snapshot, "world") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "daemon send_input did not reach the server-owned PTY"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    drop(stream);
    std::thread::sleep(Duration::from_millis(500));

    let reattached = match attach_session(&env, "default", 80, 24).expect("reattach session") {
        ServerResponse::Attached { snapshot, .. } => snapshot,
        other => panic!("unexpected reattach response: {other:?}"),
    };
    let pane = reattached.panes.values().next().expect("reattached pane");
    assert!(matches!(pane.state, PaneRuntimeState::Running));
}

#[test]
fn reattach_resizes_session_to_new_client_geometry() {
    let env = support::isolated_humu_home();
    write_config(
        &env,
        &shell_config("bash", &["-lc", "sleep 60"]),
    );

    let _daemon = support::spawn_humu_server(&env);
    wait_for_ping(&env, Duration::from_secs(5)).expect("daemon ping");

    let pane = pane_id("33333333-3333-3333-3333-333333333333");
    let mut stream = connect_server(&env).expect("connect daemon");
    let attached = send_request_on_stream::<ServerResponse>(
        &mut stream,
        &ClientRequest::AttachSession {
            name: "default".to_string(),
            cols: 80,
            rows: 24,
        },
    )
    .expect("attach session");
    assert!(matches!(attached, ServerResponse::Attached { .. }));

    let registered = send_request_on_stream::<ServerResponse>(
        &mut stream,
        &ClientRequest::RegisterPane {
            pane_id: pane,
            preset_name: "shell".to_string(),
            cwd: None,
            session_id: None,
            started_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        },
    )
    .expect("register pane");
    assert!(matches!(registered, ServerResponse::Ack));

    drop(stream);

    let mut stream = connect_server(&env).expect("reconnect daemon");
    let response = send_request_on_stream::<ServerResponse>(
        &mut stream,
        &ClientRequest::AttachSession {
            name: "default".to_string(),
            cols: 120,
            rows: 40,
        },
    )
    .expect("reattach session");
    let snapshot = match response {
        ServerResponse::Attached { snapshot, .. } => snapshot,
        other => panic!("unexpected reattach response: {other:?}"),
    };

    assert_eq!(
        snapshot.session_geometry,
        Some(SessionGeometrySnapshot {
            cols: 120,
            rows: 40,
        })
    );

    let pane = snapshot.panes.values().next().expect("reattached pane");
    assert_eq!(pane.screen.cols, 120);
    assert_eq!(pane.screen.rows, 40);
    assert_eq!(
        pane.geometry,
        Some(humu::shared::render::PaneGeometrySnapshot {
            x: 0,
            y: 0,
            width: 120,
            height: 40,
        })
    );

    let resized = send_request_on_stream::<ServerResponse>(
        &mut stream,
        &ClientRequest::ResizeSession {
            cols: 120,
            rows: 40,
        },
    )
    .expect("resize response");
    assert!(matches!(resized, ServerResponse::Ack));

    let sessions = send_request_on_stream::<ServerResponse>(&mut stream, &ClientRequest::ListSessions)
        .expect("list sessions");
    match sessions {
        ServerResponse::Sessions { sessions } => {
            let default = sessions
                .into_iter()
                .find(|session| session.name == "default")
                .expect("default session");
            assert_eq!(
                default.last_size,
                Some(SessionGeometrySnapshot {
                    cols: 120,
                    rows: 40,
                })
            );
        }
        other => panic!("unexpected list-sessions response: {other:?}"),
    }
}

fn attach_session(
    env: &support::TestEnv,
    name: &str,
    cols: u16,
    rows: u16,
) -> anyhow::Result<ServerResponse> {
    send_request(
        env,
        &ClientRequest::AttachSession {
            name: name.to_string(),
            cols,
            rows,
        },
    )
}
