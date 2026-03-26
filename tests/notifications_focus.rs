mod support;

use humu::id::PaneId;
use humu::shared::protocol::{ClientRequest, FrameDecoder, ServerResponse, encode_frame};
use humu::shared::render::AgentStatus;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn read_framed_message(stream: &mut UnixStream) -> anyhow::Result<ServerResponse> {
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

fn wait_for(condition: impl Fn() -> bool, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(condition(), "condition not met before timeout");
}

fn wait_for_ping(env: &support::TestEnv) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(mut stream) = UnixStream::connect(env.server_socket_path()) {
            stream
                .write_all(&encode_frame(&ClientRequest::Ping).expect("encode ping"))
                .expect("write ping");
            if matches!(
                read_framed_message(&mut stream),
                Ok(ServerResponse::Pong { .. })
            ) {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("daemon did not answer ping before timeout");
}

fn attach_default_session(env: &support::TestEnv) -> UnixStream {
    let mut stream = UnixStream::connect(env.server_socket_path()).expect("connect daemon");
    stream
        .write_all(
            &encode_frame(&ClientRequest::AttachSession {
                name: "default".to_string(),
                cols: 120,
                rows: 40,
            })
            .expect("encode attach"),
        )
        .expect("write attach");
    let response = read_framed_message(&mut stream).expect("read attach response");
    assert!(matches!(response, ServerResponse::Attached { .. }));
    stream
}

fn send_request(stream: &mut UnixStream, request: ClientRequest) -> ServerResponse {
    stream
        .write_all(&encode_frame(&request).expect("encode request"))
        .expect("write request");
    read_framed_message(stream).expect("read response")
}

fn spawn_humu_server_with_notify_stub(
    env: &support::TestEnv,
    notify_log: &std::path::Path,
) -> support::ScopedChild {
    let bin_dir = env.home.path().join("fake-bin");
    fs::create_dir_all(&bin_dir).expect("create fake bin dir");

    let notify_script = bin_dir.join("notify-send");
    fs::write(
        &notify_script,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HUMU_NOTIFY_LOG\"\n",
    )
    .expect("write fake notify-send");
    fs::set_permissions(&notify_script, fs::Permissions::from_mode(0o755))
        .expect("chmod fake notify-send");

    fs::write(
        env.config_path(),
        "notifications:\n  os:\n    enabled: true\n    only_unfocused: true\n  sound:\n    enabled: false\n    only_unfocused: false\n  telegram:\n    enabled: false\n    only_unfocused: false\n    bot_token_encrypted: \"\"\n    chat_id_encrypted: \"\"\n",
    )
    .expect("write daemon config");

    let mut command = support::humu_server_command(env);
    let current_path = std::env::var_os("PATH").unwrap_or_default();
    command
        .env(
            "PATH",
            format!("{}:{}", bin_dir.display(), current_path.to_string_lossy()),
        )
        .env("HUMU_NOTIFY_LOG", notify_log)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    support::spawn_scoped_command(command)
}

fn notify_log_lines(path: &std::path::Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn foreground_app_leaves_daemon_hook_port_ownership_intact() {
    let env = support::isolated_humu_home();
    let _daemon = support::spawn_humu_server(&env);
    wait_for_ping(&env);

    wait_for(|| env.hook_port_path().exists(), Duration::from_secs(5));
    let daemon_port = fs::read_to_string(env.hook_port_path())
        .expect("read daemon port file")
        .trim()
        .parse::<u16>()
        .expect("parse daemon port");
    assert!(daemon_port > 0);

    let mut app = support::spawn_humu_attach(&env, "default");
    assert!(app.wait_for_output("\u{1b}[?1049h", Duration::from_secs(2)));

    let attach_port = fs::read_to_string(env.hook_port_path())
        .expect("read port file after attach")
        .trim()
        .parse::<u16>()
        .expect("parse attach port");
    assert_eq!(attach_port, daemon_port);
    assert!(TcpStream::connect(("127.0.0.1", daemon_port)).is_ok());

    drop(app);
    wait_for(|| env.hook_port_path().exists(), Duration::from_secs(2));
    let after_drop = fs::read_to_string(env.hook_port_path())
        .expect("read port file after app exit")
        .trim()
        .parse::<u16>()
        .expect("parse port after app exit");
    assert_eq!(after_drop, daemon_port);
}

#[tokio::test]
async fn daemon_session_snapshot_retains_hook_and_codex_updates_after_detach() {
    let env = support::isolated_humu_home();
    let _daemon = support::spawn_humu_server(&env);
    wait_for_ping(&env);

    let mut stream = attach_default_session(&env);

    let hook_pane_id = PaneId::new();
    let hook_registered = send_request(
        &mut stream,
        ClientRequest::RegisterPane {
            pane_id: hook_pane_id,
            preset_name: "claude".to_string(),
            cwd: None,
            session_id: Some("hook-session".to_string()),
            started_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_secs(),
        },
    );
    assert!(matches!(hook_registered, ServerResponse::Ack));

    let codex_workspace = env.home.path().join("workspace");
    fs::create_dir_all(&codex_workspace).expect("create codex workspace");
    let codex_pane_id = PaneId::new();
    let codex_registered = send_request(
        &mut stream,
        ClientRequest::RegisterPane {
            pane_id: codex_pane_id,
            preset_name: "codex".to_string(),
            cwd: Some(codex_workspace.clone()),
            session_id: None,
            started_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_secs(),
        },
    );
    assert!(matches!(codex_registered, ServerResponse::Ack));

    let detached = send_request(&mut stream, ClientRequest::Detach);
    assert!(matches!(
        detached,
        ServerResponse::Detached { ref session_name } if session_name == "default"
    ));
    drop(stream);

    let hook_port = fs::read_to_string(env.hook_port_path())
        .expect("read hook port")
        .trim()
        .parse::<u16>()
        .expect("parse hook port");
    let hook_response = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{hook_port}/hook?paneId={hook_pane_id}&eventType=PostToolUse&sessionId=hook-session"
        ))
        .send()
        .await
        .expect("send hook event");
    assert_eq!(hook_response.status(), 200);

    let codex_root = env.home.path().join(".codex/sessions/2026/03/27");
    fs::create_dir_all(&codex_root).expect("create codex sessions root");
    let codex_session_id = "019d015a-ab86-7680-84a1-f48751186599";
    fs::write(
        codex_root.join(format!("task-2026-03-27T00-00-00-{codex_session_id}.jsonl")),
        format!(
            "{{\"timestamp\":\"2026-03-27T00:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{codex_session_id}\",\"cwd\":\"{}\"}}}}\n\
{{\"timestamp\":\"2026-03-27T00:00:01.000Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_started\"}}}}\n",
            codex_workspace.display(),
        ),
    )
    .expect("write codex session file");

    wait_for(
        || {
            let mut stream = attach_default_session(&env);
            let response = send_request(&mut stream, ClientRequest::Detach);
            drop(stream);
            matches!(response, ServerResponse::Detached { .. })
        },
        Duration::from_secs(1),
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let mut stream = attach_default_session(&env);
        let response = send_request(&mut stream, ClientRequest::Detach);
        assert!(matches!(response, ServerResponse::Detached { .. }));
        drop(stream);

        let mut stream = UnixStream::connect(env.server_socket_path()).expect("connect daemon");
        stream
            .write_all(
                &encode_frame(&ClientRequest::AttachSession {
                    name: "default".to_string(),
                    cols: 120,
                    rows: 40,
                })
                .expect("encode attach"),
            )
            .expect("write attach");
        let attach = read_framed_message(&mut stream).expect("read attach");
        let snapshot = match attach {
            ServerResponse::Attached { snapshot, .. } => snapshot,
            other => panic!("unexpected attach response: {other:?}"),
        };

        let hook_ready = snapshot
            .panes
            .get(&hook_pane_id)
            .and_then(|pane| pane.agent_state.as_ref())
            .is_some_and(|state| {
                state.status == AgentStatus::Working
                    && state.session_id.as_deref() == Some("hook-session")
            });
        let codex_ready = snapshot
            .panes
            .get(&codex_pane_id)
            .and_then(|pane| pane.agent_state.as_ref())
            .is_some_and(|state| {
                state.status == AgentStatus::Working
                    && state.session_id.as_deref() == Some(codex_session_id)
            });

        let detach = send_request(&mut stream, ClientRequest::Detach);
        assert!(matches!(detach, ServerResponse::Detached { .. }));
        if hook_ready && codex_ready {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "daemon snapshot did not retain hook/codex updates before timeout"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[tokio::test]
async fn only_unfocused_notifications_fire_after_focus_lost_and_detach() {
    let env = support::isolated_humu_home();
    let notify_log = env.home.path().join("notify.log");
    let _daemon = spawn_humu_server_with_notify_stub(&env, &notify_log);
    wait_for_ping(&env);

    let mut stream = attach_default_session(&env);
    let pane_id = PaneId::new();
    let registered = send_request(
        &mut stream,
        ClientRequest::RegisterPane {
            pane_id,
            preset_name: "claude".to_string(),
            cwd: None,
            session_id: Some("notify-session".to_string()),
            started_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_secs(),
        },
    );
    assert!(matches!(registered, ServerResponse::Ack));

    let hook_port = fs::read_to_string(env.hook_port_path())
        .expect("read hook port")
        .trim()
        .parse::<u16>()
        .expect("parse hook port");
    let client = reqwest::Client::new();

    client
        .post(format!(
            "http://127.0.0.1:{hook_port}/hook?paneId={pane_id}&eventType=PostToolUse&sessionId=notify-session"
        ))
        .send()
        .await
        .expect("send working event while focused");
    client
        .post(format!(
            "http://127.0.0.1:{hook_port}/hook?paneId={pane_id}&eventType=PermissionRequest&sessionId=notify-session"
        ))
        .send()
        .await
        .expect("send needs-input event while focused");
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        notify_log_lines(&notify_log).is_empty(),
        "focused attached session should suppress only_unfocused notifications"
    );

    let focus_lost = send_request(&mut stream, ClientRequest::FocusChanged { focused: false });
    assert!(matches!(focus_lost, ServerResponse::Ack));
    client
        .post(format!(
            "http://127.0.0.1:{hook_port}/hook?paneId={pane_id}&eventType=PostToolUse&sessionId=notify-session"
        ))
        .send()
        .await
        .expect("send working event while unfocused");
    client
        .post(format!(
            "http://127.0.0.1:{hook_port}/hook?paneId={pane_id}&eventType=PermissionRequest&sessionId=notify-session"
        ))
        .send()
        .await
        .expect("send needs-input event while unfocused");
    wait_for(
        || notify_log_lines(&notify_log).len() == 1,
        Duration::from_secs(2),
    );

    let detached = send_request(&mut stream, ClientRequest::Detach);
    assert!(matches!(
        detached,
        ServerResponse::Detached { ref session_name } if session_name == "default"
    ));
    drop(stream);

    client
        .post(format!(
            "http://127.0.0.1:{hook_port}/hook?paneId={pane_id}&eventType=PostToolUse&sessionId=notify-session"
        ))
        .send()
        .await
        .expect("send working event while detached");
    client
        .post(format!(
            "http://127.0.0.1:{hook_port}/hook?paneId={pane_id}&eventType=PermissionRequest&sessionId=notify-session"
        ))
        .send()
        .await
        .expect("send needs-input event while detached");
    wait_for(
        || notify_log_lines(&notify_log).len() == 2,
        Duration::from_secs(2),
    );

    let notifications = notify_log_lines(&notify_log);
    assert!(
        notifications
            .iter()
            .all(|line| line.contains("[unknown/unknown] Agent needs input")),
        "unexpected notification payloads: {notifications:?}"
    );
}
