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

fn snapshot_default_session(env: &support::TestEnv) -> humu::shared::render::FullSnapshot {
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
    let snapshot = match response {
        ServerResponse::Attached { snapshot, .. } => snapshot,
        other => panic!("unexpected attach response: {other:?}"),
    };
    let detached = send_request(&mut stream, ClientRequest::Detach);
    assert!(matches!(
        detached,
        ServerResponse::Detached { ref session_name } if session_name == "default"
    ));
    snapshot
}

fn wait_for_app_exit(app: &mut support::PtyHarness, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !app.child_is_alive() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(
        !app.child_is_alive(),
        "app did not exit before timeout; output: {}",
        app.output_string()
    );
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

fn daemon_auth_token(env: &support::TestEnv) -> String {
    serde_json::from_str::<serde_json::Value>(
        &fs::read_to_string(env.server_metadata_path()).expect("read daemon metadata"),
    )
    .expect("parse daemon metadata")
    .get("auth_token")
    .and_then(serde_json::Value::as_str)
    .expect("daemon auth token")
    .to_string()
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

#[test]
fn second_foreground_attach_is_refused_cleanly() {
    let env = support::isolated_humu_home();
    let _daemon = support::spawn_humu_server(&env);
    wait_for_ping(&env);

    let mut first = support::spawn_humu_attach(&env, "default");
    assert!(first.wait_for_output("\u{1b}[?1049h", Duration::from_secs(2)));
    wait_for(
        || {
            support::humu_command(&env)
                .arg("list-sessions")
                .output()
                .ok()
                .and_then(|output| String::from_utf8(output.stdout).ok())
                .is_some_and(|stdout| stdout.contains("default\tattached"))
        },
        Duration::from_secs(2),
    );

    let mut second = support::spawn_humu_attach(&env, "default");
    assert!(second.wait_for_output("already attached", Duration::from_secs(2)));
    wait_for_app_exit(&mut second, Duration::from_secs(2));
    let output = second.output_string();
    assert!(
        output.contains("already attached"),
        "expected already-attached error, got: {output}"
    );
}

#[test]
fn shell_force_detach_reclaims_attached_session() {
    let env = support::isolated_humu_home();
    let _daemon = support::spawn_humu_server(&env);
    wait_for_ping(&env);

    let mut first = support::spawn_humu_attach(&env, "default");
    assert!(first.wait_for_output("\u{1b}[?1049h", Duration::from_secs(2)));

    let output = support::humu_command(&env)
        .arg("detach")
        .arg("default")
        .arg("--force")
        .output()
        .expect("run force detach shell");
    assert!(
        output.status.success(),
        "force detach failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let mut second = support::spawn_humu_attach(&env, "default");
    assert!(
        second.wait_for_output("\u{1b}[?1049h", Duration::from_secs(2)),
        "expected attach after forced detach, got output: {}",
        second.output_string()
    );
    drop(second);
    drop(first);
}

#[test]
fn unauthorized_unregister_is_rejected_without_session_ownership() {
    let env = support::isolated_humu_home();
    let _daemon = support::spawn_humu_server(&env);
    wait_for_ping(&env);

    let mut owner = attach_default_session(&env);
    let pane_id = PaneId::new();
    let registered = send_request(
        &mut owner,
        ClientRequest::RegisterPane {
            pane_id,
            preset_name: "shell".to_string(),
            cwd: None,
            session_id: Some("owned-session".to_string()),
            started_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_secs(),
        },
    );
    assert!(matches!(registered, ServerResponse::Ack));

    let mut refused = UnixStream::connect(env.server_socket_path()).expect("connect refused client");
    refused
        .write_all(
            &encode_frame(&ClientRequest::AttachSession {
                name: "default".to_string(),
                cols: 120,
                rows: 40,
            })
            .expect("encode attach"),
        )
        .expect("write refused attach");
    let refused_attach = read_framed_message(&mut refused).expect("read refused attach");
    assert!(matches!(refused_attach, ServerResponse::AlreadyAttached { .. }));

    let unregister = send_request(&mut refused, ClientRequest::UnregisterPane { pane_id });
    assert!(matches!(
        unregister,
        ServerResponse::Error { ref message }
            if message.contains("attached session") || message.contains("not attached")
    ));

    let detached = send_request(&mut owner, ClientRequest::Detach);
    assert!(matches!(detached, ServerResponse::Detached { .. }));
    drop(owner);

    let snapshot = snapshot_default_session(&env);
    assert!(
        snapshot.panes.contains_key(&pane_id),
        "unauthorized unregister removed another session's pane: {:?}",
        snapshot.panes
    );
}

#[test]
fn unauthorized_force_detach_is_rejected_without_session_ownership() {
    let env = support::isolated_humu_home();
    let _daemon = support::spawn_humu_server(&env);
    wait_for_ping(&env);

    let mut owner = attach_default_session(&env);

    let mut attacker = UnixStream::connect(env.server_socket_path()).expect("connect attacker");
    let force_detach = send_request(
        &mut attacker,
        ClientRequest::ForceDetachSession {
            name: "default".to_string(),
            auth_token: None,
        },
    );
    assert!(matches!(force_detach, ServerResponse::Error { .. }));

    let mut probe = UnixStream::connect(env.server_socket_path()).expect("connect probe");
    probe.write_all(
        &encode_frame(&ClientRequest::AttachSession {
            name: "default".to_string(),
            cols: 120,
            rows: 40,
        })
        .expect("encode probe attach"),
    )
    .expect("write probe attach");
    let probe_attach = read_framed_message(&mut probe).expect("read probe attach");
    assert!(matches!(probe_attach, ServerResponse::AlreadyAttached { .. }));

    let detached = send_request(&mut owner, ClientRequest::Detach);
    assert!(matches!(detached, ServerResponse::Detached { .. }));
}

#[test]
fn cross_session_pane_id_reuse_is_rejected() {
    let env = support::isolated_humu_home();
    let _daemon = support::spawn_humu_server(&env);
    wait_for_ping(&env);

    let mut owner = attach_default_session(&env);
    let pane_id = PaneId::new();
    let registered = send_request(
        &mut owner,
        ClientRequest::RegisterPane {
            pane_id,
            preset_name: "shell".to_string(),
            cwd: None,
            session_id: Some("owned-session".to_string()),
            started_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_secs(),
        },
    );
    assert!(matches!(registered, ServerResponse::Ack));

    let mut other = UnixStream::connect(env.server_socket_path()).expect("connect other client");
    let created = send_request(
        &mut other,
        ClientRequest::CreateSession {
            name: "other".to_string(),
        },
    );
    assert!(matches!(created, ServerResponse::SessionCreated { .. }));
    other
        .write_all(
            &encode_frame(&ClientRequest::AttachSession {
                name: "other".to_string(),
                cols: 120,
                rows: 40,
            })
            .expect("encode other attach"),
        )
        .expect("write other attach");
    let other_attach = read_framed_message(&mut other).expect("read other attach");
    assert!(matches!(other_attach, ServerResponse::Attached { .. }));

    let hijack = send_request(
        &mut other,
        ClientRequest::RegisterPane {
            pane_id,
            preset_name: "shell".to_string(),
            cwd: None,
            session_id: Some("stolen-session".to_string()),
            started_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_secs(),
        },
    );
    assert!(matches!(hijack, ServerResponse::Error { .. }));

    let detached_other = send_request(&mut other, ClientRequest::Detach);
    assert!(matches!(detached_other, ServerResponse::Detached { .. }));

    let detached_owner = send_request(&mut owner, ClientRequest::Detach);
    assert!(matches!(detached_owner, ServerResponse::Detached { .. }));
    drop(owner);

    let snapshot = snapshot_default_session(&env);
    assert!(
        snapshot.panes.contains_key(&pane_id),
        "cross-session pane-id reuse replaced the original pane: {:?}",
        snapshot.panes
    );
}

#[test]
fn force_detached_owner_cannot_mutate_session_after_revocation() {
    let env = support::isolated_humu_home();
    let _daemon = support::spawn_humu_server(&env);
    wait_for_ping(&env);

    let mut old_owner = attach_default_session(&env);
    let force_detach = send_request(
        &mut UnixStream::connect(env.server_socket_path()).expect("connect force-detach client"),
        ClientRequest::ForceDetachSession {
            name: "default".to_string(),
            auth_token: Some(daemon_auth_token(&env)),
        },
    );
    assert!(matches!(force_detach, ServerResponse::Detached { .. }));

    let register = send_request(
        &mut old_owner,
        ClientRequest::RegisterPane {
            pane_id: PaneId::new(),
            preset_name: "shell".to_string(),
            cwd: None,
            session_id: Some("stale-owner".to_string()),
            started_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_secs(),
        },
    );
    assert!(matches!(register, ServerResponse::Error { .. }));
}

#[tokio::test]
async fn stale_owner_disconnect_does_not_poison_replacement_focus_state() {
    let env = support::isolated_humu_home();
    let notify_log = env.home.path().join("notify.log");
    let _daemon = spawn_humu_server_with_notify_stub(&env, &notify_log);
    support::persistence::save_state(&env.state_path(), &support::migrated_state_fixture())
        .expect("save state fixture");
    wait_for_ping(&env);

    let old_owner = attach_default_session(&env);
    let force_detach = send_request(
        &mut UnixStream::connect(env.server_socket_path()).expect("connect force-detach client"),
        ClientRequest::ForceDetachSession {
            name: "default".to_string(),
            auth_token: Some(daemon_auth_token(&env)),
        },
    );
    assert!(matches!(force_detach, ServerResponse::Detached { .. }));

    let mut new_owner = attach_default_session(&env);
    let pane_id = PaneId::new();
    let registered = send_request(
        &mut new_owner,
        ClientRequest::RegisterPane {
            pane_id,
            preset_name: "claude".to_string(),
            cwd: None,
            session_id: Some("replacement-session".to_string()),
            started_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_secs(),
        },
    );
    assert!(matches!(registered, ServerResponse::Ack));

    drop(old_owner);
    std::thread::sleep(Duration::from_millis(100));

    let hook_port = fs::read_to_string(env.hook_port_path())
        .expect("read hook port")
        .trim()
        .parse::<u16>()
        .expect("parse hook port");
    let client = reqwest::Client::new();
    client
        .post(format!(
            "http://127.0.0.1:{hook_port}/hook?workspaceId={}&roomId={}&paneId={pane_id}&eventType=PostToolUse&sessionId=replacement-session",
            support::workspace_id("humu"),
            support::room_id("main"),
        ))
        .send()
        .await
        .expect("send working event");
    client
        .post(format!(
            "http://127.0.0.1:{hook_port}/hook?workspaceId={}&roomId={}&paneId={pane_id}&eventType=PermissionRequest&sessionId=replacement-session",
            support::workspace_id("humu"),
            support::room_id("main"),
        ))
        .send()
        .await
        .expect("send needs-input event");
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        notify_log_lines(&notify_log).is_empty(),
        "stale owner disconnect poisoned replacement focus state: {:?}",
        notify_log_lines(&notify_log)
    );

    let probe = send_request(
        &mut new_owner,
        ClientRequest::FocusChanged { focused: true },
    );
    assert!(matches!(probe, ServerResponse::Ack));
}

#[tokio::test]
async fn stale_force_detached_owner_attaching_elsewhere_does_not_poison_replacement_focus() {
    let env = support::isolated_humu_home();
    let notify_log = env.home.path().join("notify.log");
    let _daemon = spawn_humu_server_with_notify_stub(&env, &notify_log);
    support::persistence::save_state(&env.state_path(), &support::migrated_state_fixture())
        .expect("save state fixture");
    wait_for_ping(&env);

    let mut old_owner = attach_default_session(&env);
    let force_detach = send_request(
        &mut UnixStream::connect(env.server_socket_path()).expect("connect force-detach client"),
        ClientRequest::ForceDetachSession {
            name: "default".to_string(),
            auth_token: Some(daemon_auth_token(&env)),
        },
    );
    assert!(matches!(force_detach, ServerResponse::Detached { .. }));

    let mut new_owner = attach_default_session(&env);
    let pane_id = PaneId::new();
    let registered = send_request(
        &mut new_owner,
        ClientRequest::RegisterPane {
            pane_id,
            preset_name: "claude".to_string(),
            cwd: None,
            session_id: Some("replacement-session".to_string()),
            started_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_secs(),
        },
    );
    assert!(matches!(registered, ServerResponse::Ack));

    let reattach_elsewhere = send_request(
        &mut old_owner,
        ClientRequest::AttachSession {
            name: "other".to_string(),
            cols: 120,
            rows: 40,
        },
    );
    assert!(matches!(reattach_elsewhere, ServerResponse::Attached { .. }));

    let hook_port = fs::read_to_string(env.hook_port_path())
        .expect("read hook port")
        .trim()
        .parse::<u16>()
        .expect("parse hook port");
    let client = reqwest::Client::new();
    client
        .post(format!(
            "http://127.0.0.1:{hook_port}/hook?workspaceId={}&roomId={}&paneId={pane_id}&eventType=PostToolUse&sessionId=replacement-session",
            support::workspace_id("humu"),
            support::room_id("main"),
        ))
        .send()
        .await
        .expect("send working event");
    client
        .post(format!(
            "http://127.0.0.1:{hook_port}/hook?workspaceId={}&roomId={}&paneId={pane_id}&eventType=PermissionRequest&sessionId=replacement-session",
            support::workspace_id("humu"),
            support::room_id("main"),
        ))
        .send()
        .await
        .expect("send needs-input event");
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        notify_log_lines(&notify_log).is_empty(),
        "stale owner attach poisoned replacement focus state: {:?}",
        notify_log_lines(&notify_log)
    );
}

#[test]
fn force_detach_clears_runtime_panes_from_later_session_snapshots() {
    let env = support::isolated_humu_home();
    let _daemon = support::spawn_humu_server(&env);
    wait_for_ping(&env);

    let mut owner = attach_default_session(&env);
    let pane_id = PaneId::new();
    let registered = send_request(
        &mut owner,
        ClientRequest::RegisterPane {
            pane_id,
            preset_name: "shell".to_string(),
            cwd: None,
            session_id: Some("stale-pane".to_string()),
            started_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_secs(),
        },
    );
    assert!(matches!(registered, ServerResponse::Ack));

    let force_detach = send_request(
        &mut UnixStream::connect(env.server_socket_path()).expect("connect force-detach client"),
        ClientRequest::ForceDetachSession {
            name: "default".to_string(),
            auth_token: Some(daemon_auth_token(&env)),
        },
    );
    assert!(matches!(force_detach, ServerResponse::Detached { .. }));

    let snapshot = snapshot_default_session(&env);
    assert!(
        !snapshot.panes.contains_key(&pane_id),
        "force detach left stale runtime pane behind: {:?}",
        snapshot.panes
    );
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
async fn foreground_attach_rehydrates_runtime_agent_state_from_daemon_snapshot() {
    let env = support::isolated_humu_home();
    fs::write(
        env.config_path(),
        "presets:\n  shell:\n    command: /bin/sh\n    args:\n      - -lc\n      - sleep 60\n",
    )
    .expect("write quiet shell config");

    let mut state = support::migrated_state_fixture();
    let default_session = state.ensure_session("default");
    default_session.active_workspace_id = Some(support::workspace_id("humu"));
    default_session.active_room_id = Some(support::room_id("main"));
    default_session.tabs_by_room.insert(
        support::room_id("main"),
        humu::config::PersistedRoomLayout {
            active_tab: 0,
            tabs: vec![humu::config::TabLayout {
                name: "shell".to_string(),
                split: humu::config::SplitNode::Leaf {
                    preset: "shell".to_string(),
                    session_id: Some("rehydrate-session".to_string()),
                },
            }],
        },
    );
    support::persistence::save_state(&env.state_path(), &state).expect("save state fixture");

    let _daemon = support::spawn_humu_server(&env);
    wait_for_ping(&env);

    let mut stream = attach_default_session(&env);
    let pane_id = PaneId::new();
    let registered = send_request(
        &mut stream,
        ClientRequest::RegisterPane {
            pane_id,
            preset_name: "shell".to_string(),
            cwd: None,
            session_id: Some("rehydrate-session".to_string()),
            started_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_secs(),
        },
    );
    assert!(matches!(registered, ServerResponse::Ack));
    let detached = send_request(&mut stream, ClientRequest::Detach);
    assert!(matches!(detached, ServerResponse::Detached { .. }));
    drop(stream);

    let hook_port = fs::read_to_string(env.hook_port_path())
        .expect("read hook port")
        .trim()
        .parse::<u16>()
        .expect("parse hook port");
    let hook_response = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{hook_port}/hook?paneId={pane_id}&eventType=PostToolUse&sessionId=rehydrate-session"
        ))
        .send()
        .await
        .expect("send hook event");
    assert_eq!(hook_response.status(), 200);

    wait_for(
        || {
            let mut stream = attach_default_session(&env);
            let response = send_request(&mut stream, ClientRequest::Detach);
            let attach = if let ServerResponse::Detached { .. } = response {
                Some(())
            } else {
                None
            };
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
            let snapshot = match read_framed_message(&mut stream).expect("read attach") {
                ServerResponse::Attached { snapshot, .. } => snapshot,
                other => panic!("unexpected attach response: {other:?}"),
            };
            let detach = send_request(&mut stream, ClientRequest::Detach);
            assert!(attach.is_some());
            assert!(matches!(detach, ServerResponse::Detached { .. }));
            snapshot.panes.values().any(|pane| {
                pane.preset_name == "shell"
                    && pane.agent_state.as_ref().is_some_and(|state| {
                        state.status == AgentStatus::Working
                            && state.session_id.as_deref() == Some("rehydrate-session")
                    })
            })
        },
        Duration::from_secs(5),
    );

    let mut app = support::spawn_humu_attach(&env, "default");
    assert!(app.wait_for_output("\u{1b}[?1049h", Duration::from_secs(2)));
    assert!(
        app.wait_for_output("shell", Duration::from_secs(2))
            && app.wait_for_output("⠋", Duration::from_secs(2)),
        "expected rehydrated spinner in attach UI, got output: {}",
        app.output_string()
    );
}

#[tokio::test]
async fn foreground_attach_rehydrates_daemon_learned_hook_session_id_without_local_seed() {
    let env = support::isolated_humu_home();
    fs::write(
        env.config_path(),
        "presets:\n  shell:\n    command: /bin/sh\n    args:\n      - -lc\n      - sleep 60\n",
    )
    .expect("write quiet shell config");

    let mut state = support::migrated_state_fixture();
    let default_session = state.ensure_session("default");
    default_session.active_workspace_id = Some(support::workspace_id("humu"));
    default_session.active_room_id = Some(support::room_id("main"));
    default_session.tabs_by_room.insert(
        support::room_id("main"),
        humu::config::PersistedRoomLayout {
            active_tab: 0,
            tabs: vec![humu::config::TabLayout {
                name: "shell".to_string(),
                split: humu::config::SplitNode::Leaf {
                    preset: "shell".to_string(),
                    session_id: None,
                },
            }],
        },
    );
    support::persistence::save_state(&env.state_path(), &state).expect("save state fixture");

    let _daemon = support::spawn_humu_server(&env);
    wait_for_ping(&env);

    let mut stream = attach_default_session(&env);
    let pane_id = PaneId::new();
    let registered = send_request(
        &mut stream,
        ClientRequest::RegisterPane {
            pane_id,
            preset_name: "shell".to_string(),
            cwd: None,
            session_id: None,
            started_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_secs(),
        },
    );
    assert!(matches!(registered, ServerResponse::Ack));
    let detached = send_request(&mut stream, ClientRequest::Detach);
    assert!(matches!(detached, ServerResponse::Detached { .. }));
    drop(stream);

    let hook_port = fs::read_to_string(env.hook_port_path())
        .expect("read hook port")
        .trim()
        .parse::<u16>()
        .expect("parse hook port");
    let hook_response = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{hook_port}/hook?workspaceId={}&roomId={}&paneId={pane_id}&eventType=PostToolUse&sessionId=daemon-learned-session",
            support::workspace_id("humu"),
            support::room_id("main"),
        ))
        .send()
        .await
        .expect("send hook event");
    assert_eq!(hook_response.status(), 200);

    wait_for(
        || {
            snapshot_default_session(&env).panes.values().any(|pane| {
                pane.preset_name == "shell"
                    && pane.agent_state.as_ref().is_some_and(|state| {
                        state.status == AgentStatus::Working
                            && state.session_id.as_deref() == Some("daemon-learned-session")
                    })
            })
        },
        Duration::from_secs(5),
    );

    let mut app = support::spawn_humu_attach(&env, "default");
    assert!(app.wait_for_output("\u{1b}[?1049h", Duration::from_secs(2)));
    assert!(
        app.wait_for_output("shell", Duration::from_secs(2))
            && app.wait_for_output("⠋", Duration::from_secs(2)),
        "expected daemon-learned shell spinner in attach UI, got output: {}",
        app.output_string()
    );
}

#[tokio::test]
async fn foreground_attach_rehydrates_daemon_discovered_codex_session_id() {
    let env = support::isolated_humu_home();
    fs::write(
        env.config_path(),
        "presets:\n  codex:\n    command: /bin/sh\n    args:\n      - -lc\n      - sleep 60\n",
    )
    .expect("write quiet codex config");

    let codex_workspace = env.home.path().join("workspace");
    fs::create_dir_all(&codex_workspace).expect("create codex workspace");

    let mut state = support::migrated_state_fixture();
    let default_session = state.ensure_session("default");
    default_session.active_workspace_id = Some(support::workspace_id("humu"));
    default_session.active_room_id = Some(support::room_id("main"));
    default_session.tabs_by_room.insert(
        support::room_id("main"),
        humu::config::PersistedRoomLayout {
            active_tab: 0,
            tabs: vec![humu::config::TabLayout {
                name: "codex".to_string(),
                split: humu::config::SplitNode::Leaf {
                    preset: "codex".to_string(),
                    session_id: None,
                },
            }],
        },
    );
    support::persistence::save_state(&env.state_path(), &state).expect("save state fixture");

    let _daemon = support::spawn_humu_server(&env);
    wait_for_ping(&env);

    let mut stream = attach_default_session(&env);
    let pane_id = PaneId::new();
    let registered = send_request(
        &mut stream,
        ClientRequest::RegisterPane {
            pane_id,
            preset_name: "codex".to_string(),
            cwd: Some(codex_workspace.clone()),
            session_id: None,
            started_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_secs(),
        },
    );
    assert!(matches!(registered, ServerResponse::Ack));
    let detached = send_request(&mut stream, ClientRequest::Detach);
    assert!(matches!(detached, ServerResponse::Detached { .. }));
    drop(stream);

    let codex_root = env.home.path().join(".codex/sessions/2026/03/27");
    fs::create_dir_all(&codex_root).expect("create codex sessions root");
    let codex_session_id = "019d015a-ab86-7680-84a1-f487511865aa";
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
            snapshot_default_session(&env).panes.values().any(|pane| {
                pane.preset_name == "codex"
                    && pane.agent_state.as_ref().is_some_and(|state| {
                        state.status == AgentStatus::Working
                            && state.session_id.as_deref() == Some(codex_session_id)
                    })
            })
        },
        Duration::from_secs(5),
    );

    let mut app = support::spawn_humu_attach(&env, "default");
    assert!(app.wait_for_output("\u{1b}[?1049h", Duration::from_secs(2)));
    assert!(
        app.wait_for_output("codex", Duration::from_secs(2))
            && app.wait_for_output("⠋", Duration::from_secs(2)),
        "expected codex spinner in attach UI, got output: {}",
        app.output_string()
    );
}

#[tokio::test]
async fn graceful_foreground_exit_cleans_runtime_registrations_between_attach_cycles() {
    let env = support::isolated_humu_home();
    fs::write(
        env.config_path(),
        "presets:\n  shell:\n    command: /bin/sh\n    args:\n      - -lc\n      - sleep 60\n",
    )
    .expect("write quiet shell config");

    let mut state = support::migrated_state_fixture();
    let default_session = state.ensure_session("default");
    default_session.active_workspace_id = Some(support::workspace_id("humu"));
    default_session.active_room_id = Some(support::room_id("main"));
    default_session.tabs_by_room.insert(
        support::room_id("main"),
        humu::config::PersistedRoomLayout {
            active_tab: 0,
            tabs: vec![humu::config::TabLayout {
                name: "shell".to_string(),
                split: humu::config::SplitNode::Leaf {
                    preset: "shell".to_string(),
                    session_id: Some("cycle-session".to_string()),
                },
            }],
        },
    );
    support::persistence::save_state(&env.state_path(), &state).expect("save state fixture");

    let _daemon = support::spawn_humu_server(&env);
    wait_for_ping(&env);

    let mut first = support::spawn_humu_attach(&env, "default");
    assert!(first.wait_for_output("\u{1b}[?1049h", Duration::from_secs(2)));
    first.write_input(b"\x11");
    wait_for_app_exit(&mut first, Duration::from_secs(2));

    let snapshot = snapshot_default_session(&env);
    assert!(
        !snapshot.panes.values().any(|pane| {
            pane.preset_name == "shell"
                && pane.agent_state.as_ref().is_some_and(|state| {
                    state.session_id.as_deref() == Some("cycle-session")
                })
        }),
        "graceful exit left stale runtime pane registrations behind: {:?}",
        snapshot.panes
    );

    let hook_port = fs::read_to_string(env.hook_port_path())
        .expect("read hook port")
        .trim()
        .parse::<u16>()
        .expect("parse hook port");
    let seeded_pane_id = PaneId::new();
    let mut seed_stream = attach_default_session(&env);
    let registered = send_request(
        &mut seed_stream,
        ClientRequest::RegisterPane {
            pane_id: seeded_pane_id,
            preset_name: "shell".to_string(),
            cwd: None,
            session_id: Some("cycle-session".to_string()),
            started_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_secs(),
        },
    );
    assert!(matches!(registered, ServerResponse::Ack));
    let detached = send_request(&mut seed_stream, ClientRequest::Detach);
    assert!(matches!(detached, ServerResponse::Detached { .. }));
    drop(seed_stream);

    let hook_response = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{hook_port}/hook?paneId={seeded_pane_id}&eventType=PostToolUse&sessionId=cycle-session"
        ))
        .send()
        .await
        .expect("send seeded hook event");
    assert_eq!(hook_response.status(), 200);

    wait_for(
        || {
            snapshot_default_session(&env).panes.values().any(|pane| {
                pane.preset_name == "shell"
                    && pane.agent_state.as_ref().is_some_and(|state| {
                        state.status == AgentStatus::Working
                            && state.session_id.as_deref() == Some("cycle-session")
                    })
            })
        },
        Duration::from_secs(5),
    );

    let mut second = support::spawn_humu_attach(&env, "default");
    assert!(second.wait_for_output("\u{1b}[?1049h", Duration::from_secs(2)));
    assert!(
        second.wait_for_output("shell", Duration::from_secs(2))
            && second.wait_for_output("⠋", Duration::from_secs(2)),
        "expected rehydrated spinner after repeated attach cycle, got output: {}",
        second.output_string()
    );
    second.write_input(b"\x11");
    wait_for_app_exit(&mut second, Duration::from_secs(2));

    let final_snapshot = snapshot_default_session(&env);
    let remaining_cycle_panes = final_snapshot
        .panes
        .iter()
        .filter(|(_, pane)| {
            pane.preset_name == "shell"
                && pane.agent_state.as_ref().is_some_and(|state| {
                    state.session_id.as_deref() == Some("cycle-session")
                })
        })
        .collect::<Vec<_>>();
    assert!(
        remaining_cycle_panes.len() == 1
            && remaining_cycle_panes[0].0 == &seeded_pane_id
            && remaining_cycle_panes[0]
                .1
                .agent_state
                .as_ref()
                .is_some_and(|state| state.status == AgentStatus::Working),
        "second graceful exit left stale runtime pane registrations behind: {:?}",
        final_snapshot.panes
    );
}

#[tokio::test]
async fn only_unfocused_notifications_fire_after_focus_lost_and_detach() {
    let env = support::isolated_humu_home();
    let notify_log = env.home.path().join("notify.log");
    let _daemon = spawn_humu_server_with_notify_stub(&env, &notify_log);
    wait_for_ping(&env);
    let workspace_id = support::workspace_id("humu").to_string();
    let room_id = support::room_id("main").to_string();
    support::persistence::save_state(&env.state_path(), &support::migrated_state_fixture())
        .expect("save state fixture");

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
            "http://127.0.0.1:{hook_port}/hook?workspaceId={workspace_id}&roomId={room_id}&paneId={pane_id}&eventType=PostToolUse&sessionId=notify-session"
        ))
        .send()
        .await
        .expect("send working event while focused");
    client
        .post(format!(
            "http://127.0.0.1:{hook_port}/hook?workspaceId={workspace_id}&roomId={room_id}&paneId={pane_id}&eventType=PermissionRequest&sessionId=notify-session"
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
            "http://127.0.0.1:{hook_port}/hook?workspaceId={workspace_id}&roomId={room_id}&paneId={pane_id}&eventType=PostToolUse&sessionId=notify-session"
        ))
        .send()
        .await
        .expect("send working event while unfocused");
    client
        .post(format!(
            "http://127.0.0.1:{hook_port}/hook?workspaceId={workspace_id}&roomId={room_id}&paneId={pane_id}&eventType=PermissionRequest&sessionId=notify-session"
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
            "http://127.0.0.1:{hook_port}/hook?workspaceId={workspace_id}&roomId={room_id}&paneId={pane_id}&eventType=PostToolUse&sessionId=notify-session"
        ))
        .send()
        .await
        .expect("send working event while detached");
    client
        .post(format!(
            "http://127.0.0.1:{hook_port}/hook?workspaceId={workspace_id}&roomId={room_id}&paneId={pane_id}&eventType=PermissionRequest&sessionId=notify-session"
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
            .all(|line| line.contains("[humu/main] Agent needs input")),
        "unexpected notification payloads: {notifications:?}"
    );
}
