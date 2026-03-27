#[path = "../src/server/mod.rs"]
mod server_impl;

mod support;

use humu::config::{HumuConfig, HumuState, NotificationsConfig};
use humu::id::PaneId;
use humu::pty::pane::PtyPane;
use humu::shared::protocol::{ClientRequest, FrameDecoder, ServerResponse, encode_frame};
use humu::shared::render::{FullSnapshot, PaneRuntimeState, SessionGeometrySnapshot};
use serde::de::DeserializeOwned;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
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

fn wait_for_path(path: &Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "path was not created before timeout: {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(target_os = "linux")]
fn wait_for_process_exit(pid: u32, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while support::process_is_alive(pid) {
        assert!(
            Instant::now() < deadline,
            "process did not exit before timeout: {pid}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
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
fn runtime_register_pane_applies_claude_env_and_resume_args() {
    let env = support::isolated_humu_home();
    let state = support::migrated_state_fixture();
    support::persistence::save_state(&env.state_path(), &state).expect("save state");

    let mut config = HumuConfig::default();
    let claude = config.presets.get_mut("claude").expect("claude preset");
    claude.command = "python3".to_string();
    claude.args = vec![
        "-c".to_string(),
        "import os, sys, time; print(f\"port={os.getenv('HUMU_PORT', '')}\"); print(f\"ws={os.getenv('HUMU_WORKSPACE_ID', '')}\"); print(f\"room={os.getenv('HUMU_ROOM_ID', '')}\"); print(f\"pane={os.getenv('HUMU_PANE_ID', '')}\"); print(f\"arg1={sys.argv[1] if len(sys.argv) > 1 else ''}\"); print(f\"arg2={sys.argv[2] if len(sys.argv) > 2 else ''}\"); print(f\"arg3={sys.argv[3] if len(sys.argv) > 3 else ''}\"); print(f\"arg4={sys.argv[4] if len(sys.argv) > 4 else ''}\"); sys.stdout.flush(); time.sleep(60)".to_string(),
    ];

    let runtime = server_impl::runtime::SessionRuntime::start(
        env.humu_dir().to_path_buf(),
        config,
        NotificationsConfig::default(),
        env.home.path().join(".codex/sessions"),
    )
    .expect("start runtime");

    let pane = pane_id("22222222-2222-2222-2222-222222222222");
    let hook_port = runtime.hook_port();
    runtime.attach_session("default");
    runtime
        .register_pane(
            "default",
            pane,
            "claude",
            None,
            Some("resume-claude".to_string()),
            SystemTime::now(),
        )
        .expect("register claude runtime pane");

    let expected_settings = env.humu_dir().join("hooks/claude-settings.json");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut snapshot = runtime.snapshot_for_session("default", FullSnapshot::fixture());
    while !(snapshot_contains(&snapshot, &format!("port={hook_port}"))
        && snapshot_contains(
            &snapshot,
            &format!("ws={}", support::workspace_id("humu")),
        )
        && snapshot_contains(&snapshot, &format!("room={}", support::room_id("main")))
        && snapshot_contains(&snapshot, &format!("pane={pane}"))
        && snapshot_contains(&snapshot, "arg1=--settings")
        && snapshot_contains(
            &snapshot,
            &format!("arg2={}", expected_settings.display()),
        )
        && snapshot_contains(&snapshot, "arg3=--resume")
        && snapshot_contains(&snapshot, "arg4=resume-claude"))
    {
        assert!(
            Instant::now() < deadline,
            "server-owned claude spawn contract never appeared in snapshot: {:?}",
            snapshot.panes
        );
        std::thread::sleep(Duration::from_millis(50));
        snapshot = runtime.snapshot_for_session("default", FullSnapshot::fixture());
    }
}

#[test]
fn runtime_register_pane_applies_codex_resume_args() {
    let env = support::isolated_humu_home();
    let mut config = HumuConfig::default();
    let codex = config.presets.get_mut("codex").expect("codex preset");
    codex.command = "python3".to_string();
    codex.args = vec![
        "-c".to_string(),
        "import sys, time; print(f\"arg1={sys.argv[1] if len(sys.argv) > 1 else ''}\"); print(f\"arg2={sys.argv[2] if len(sys.argv) > 2 else ''}\"); sys.stdout.flush(); time.sleep(60)".to_string(),
    ];

    let runtime = server_impl::runtime::SessionRuntime::start(
        env.humu_dir().to_path_buf(),
        config,
        NotificationsConfig::default(),
        env.home.path().join(".codex/sessions"),
    )
    .expect("start runtime");

    let pane = pane_id("33333333-3333-3333-3333-333333333333");
    runtime.attach_session("default");
    runtime
        .register_pane(
            "default",
            pane,
            "codex",
            None,
            Some("resume-codex".to_string()),
            SystemTime::now(),
        )
        .expect("register codex runtime pane");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut snapshot = runtime.snapshot_for_session("default", FullSnapshot::fixture());
    while !(snapshot_contains(&snapshot, "arg1=resume")
        && snapshot_contains(&snapshot, "arg2=resume-codex"))
    {
        assert!(
            Instant::now() < deadline,
            "server-owned codex spawn contract never appeared in snapshot: {:?}",
            snapshot.panes
        );
        std::thread::sleep(Duration::from_millis(50));
        snapshot = runtime.snapshot_for_session("default", FullSnapshot::fixture());
    }
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

    let deadline = Instant::now() + Duration::from_secs(1);
    let (mut stream, response) = loop {
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
        if matches!(response, ServerResponse::Attached { .. }) {
            break (stream, response);
        }
        assert!(
            Instant::now() < deadline,
            "session lock was not released before timeout: {response:?}"
        );
        std::thread::sleep(Duration::from_millis(25));
    };
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

#[test]
fn app_attached_snapshot_drives_main_pane_state_without_local_pty() {
    let mut app = support::App::test_with_state(HumuState::default(), std::env::temp_dir());
    let snapshot = FullSnapshot::fixture();
    let pane_id = snapshot.focused_pane_id.expect("focused pane");

    app.test_hydrate_attached_snapshot(snapshot);
    app.local_panes.clear();

    assert_eq!(app.test_attached_screen_contents(pane_id).as_deref(), Some("hu\ns"));

    let input_state = app
        .test_pane_input_state(pane_id)
        .expect("attached pane input state");
    assert!(input_state.alternate_screen);
    assert!(input_state.bracketed_paste);
    assert_eq!(input_state.rows, 24);

    let matches = app.test_search_matches_for_query(pane_id, "hu");
    assert_eq!(matches, vec![(0, 0, 2)]);
}

#[cfg(target_os = "linux")]
#[test]
fn floating_editor_exits_when_client_exits() {
    let mut app = support::App::test_with_state(HumuState::default(), std::env::temp_dir());
    let pid_file = std::env::temp_dir().join(format!("humu-floating-editor-pid-{}", Uuid::new_v4()));
    let _ = fs::remove_file(&pid_file);

    let pane_id = PaneId::new();
    let pane = PtyPane::spawn(
        "sh",
        &[String::from("-c"), format!("echo $$ > '{}'; exec cat >/dev/null", pid_file.display())],
        None,
        80,
        24,
    )
    .expect("spawn floating editor pane");
    app.local_panes.insert(pane_id, pane);
    app.pane_presets.insert(pane_id, "_editor".to_string());
    app.popup = support::PopupState::FloatingPane {
        pane_id,
        title: "editor".to_string(),
    };

    wait_for_path(&pid_file, Duration::from_secs(5));
    let pid: u32 = fs::read_to_string(&pid_file)
        .expect("read editor pid")
        .trim()
        .parse()
        .expect("parse editor pid");
    assert!(support::process_is_alive(pid));

    drop(app);

    wait_for_process_exit(pid, Duration::from_secs(5));
    let _ = fs::remove_file(pid_file);
}

#[cfg(target_os = "linux")]
#[test]
fn diff_popup_exits_when_client_exits() {
    let mut app = support::App::test_with_state(HumuState::default(), std::env::temp_dir());
    let pid_file = std::env::temp_dir().join(format!("humu-floating-diff-pid-{}", Uuid::new_v4()));
    let _ = fs::remove_file(&pid_file);

    let pane_id = PaneId::new();
    let pane = PtyPane::spawn(
        "sh",
        &[String::from("-c"), format!("echo $$ > '{}'; exec cat >/dev/null", pid_file.display())],
        None,
        80,
        24,
    )
    .expect("spawn diff popup pane");
    app.local_panes.insert(pane_id, pane);
    app.pane_presets.insert(pane_id, "_diff".to_string());
    app.popup = support::PopupState::FloatingPane {
        pane_id,
        title: "diff".to_string(),
    };

    wait_for_path(&pid_file, Duration::from_secs(5));
    let pid: u32 = fs::read_to_string(&pid_file)
        .expect("read diff pid")
        .trim()
        .parse()
        .expect("parse diff pid");
    assert!(support::process_is_alive(pid));

    drop(app);

    wait_for_process_exit(pid, Duration::from_secs(5));
    let _ = fs::remove_file(pid_file);
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
