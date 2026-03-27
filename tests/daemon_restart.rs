mod support;

use humu::config::{
    HumuConfig, HumuState, Preset, RoomEntry, SessionState, WorkspaceEntry,
};
use humu::id::PaneId;
use humu::shared::protocol::{ClientRequest, FrameDecoder, ServerResponse, encode_frame};
use humu::shared::render::FullSnapshot;
use serde::de::DeserializeOwned;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::SystemTime;
use std::time::{Duration, Instant};

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

fn restart_state_fixture(env: &support::TestEnv) -> HumuState {
    let workspace_id = support::workspace_id("humu");
    let room_id = support::room_id("main");
    let workspace_path = env.cwd().join("workspace");
    fs::create_dir_all(&workspace_path).expect("create workspace path");

    HumuState {
        active_workspace_id: Some(workspace_id),
        active_room_id: Some(room_id),
        workspaces: vec![WorkspaceEntry {
            name: "humu".to_string(),
            id: workspace_id,
            path: workspace_path.clone(),
            last_room_id: Some(room_id),
            rooms: vec![RoomEntry {
                name: "main".to_string(),
                id: room_id,
                path: workspace_path.clone(),
                active_tab: None,
                tabs: vec![],
            }],
        }],
        sessions: vec![SessionState {
            name: HumuState::DEFAULT_SESSION_NAME.to_string(),
            active_workspace_id: Some(workspace_id),
            active_room_id: Some(room_id),
            tabs_by_room: Default::default(),
            attached: false,
            last_size: None,
        }],
        panel_widths: None,
    }
}

fn restart_config_fixture() -> HumuConfig {
    let mut config = HumuConfig::default();
    config.presets.insert(
        "shell".to_string(),
        Preset {
            command: "bash".to_string(),
            args: vec!["-lc".to_string(), "printf 'restored\\n'; sleep 60".to_string()],
        },
    );
    config
}

fn attach_snapshot(env: &support::TestEnv) -> FullSnapshot {
    match send_request::<ServerResponse>(
        env,
        &ClientRequest::AttachSession {
            name: "default".to_string(),
            cols: 100,
            rows: 30,
        },
    )
    .expect("attach default session")
    {
        ServerResponse::Attached { snapshot, .. } => snapshot,
        other => panic!("unexpected attach response: {other:?}"),
    }
}

fn snapshot_has_restored_shell(snapshot: &FullSnapshot) -> bool {
    snapshot.panes.len() == 1
        && snapshot
            .panes
            .values()
            .next()
            .is_some_and(|pane| pane.preset_name == "shell")
}

fn wait_for_restored_snapshot(env: &support::TestEnv, stage: &str) -> FullSnapshot {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let snapshot = attach_snapshot(env);
        if snapshot_has_restored_shell(&snapshot) {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "daemon never restored shell snapshot during {stage}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn daemon_restart_cold_restores_session_layout() {
    let env = support::isolated_humu_home();
    support::write_config(&env, &restart_config_fixture());
    support::write_state(&env, &restart_state_fixture(&env));

    let mut daemon = support::spawn_humu_server(&env);
    wait_for_ping(&env, Duration::from_secs(5)).expect("daemon ping");

    let pane_id = PaneId::new();
    let mut stream = connect_server(&env).expect("connect server");
    match send_request_on_stream::<ServerResponse>(
        &mut stream,
        &ClientRequest::AttachSession {
            name: "default".to_string(),
            cols: 100,
            rows: 30,
        },
    )
    .expect("attach default session")
    {
        ServerResponse::Attached { session_name, .. } => assert_eq!(session_name, "default"),
        other => panic!("unexpected attach response: {other:?}"),
    }
    match send_request_on_stream::<ServerResponse>(
        &mut stream,
        &ClientRequest::RegisterPane {
            pane_id,
            preset_name: "shell".to_string(),
            cwd: Some(env.cwd().join("workspace")),
            session_id: None,
            started_at_unix_secs: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("unix epoch")
                .as_secs(),
        },
    )
    .expect("register runtime pane")
    {
        ServerResponse::Ack => {}
        other => panic!("unexpected register response: {other:?}"),
    }
    match send_request_on_stream::<ServerResponse>(&mut stream, &ClientRequest::Detach)
        .expect("detach session")
    {
        ServerResponse::Detached { session_name } => assert_eq!(session_name, "default"),
        other => panic!("unexpected detach response: {other:?}"),
    }
    drop(stream);

    let persisted_state = support::persistence::load_state(&env.state_path()).expect("load state");
    let persisted_session = persisted_state
        .session_by_name("default")
        .expect("persisted default session");
    let persisted_layout = persisted_session
        .tabs_by_room
        .get(&support::room_id("main"))
        .expect("persisted room layout");
    assert_eq!(persisted_layout.tabs.len(), 1);
    assert_eq!(persisted_layout.tabs[0].name, "runtime");
    match &persisted_layout.tabs[0].split {
        humu::config::SplitNode::Leaf { preset, .. } => assert_eq!(preset, "shell"),
        other => panic!("unexpected persisted split: {other:?}"),
    }
    assert_eq!(
        persisted_session.last_size,
        Some(humu::config::SessionSize { cols: 100, rows: 30 })
    );

    let first_snapshot = wait_for_restored_snapshot(&env, "first attach");
    assert_eq!(first_snapshot.panes.len(), 1);
    assert_eq!(
        first_snapshot.session_geometry,
        Some(humu::shared::render::SessionGeometrySnapshot {
            cols: 100,
            rows: 30,
        })
    );

    daemon.kill();

    let _daemon = support::spawn_humu_server(&env);
    wait_for_ping(&env, Duration::from_secs(5)).expect("daemon restart ping");

    let restored_snapshot = wait_for_restored_snapshot(&env, "daemon restart");
    assert_eq!(restored_snapshot.panes.len(), 1);
    assert_eq!(
        restored_snapshot
            .panes
            .values()
            .next()
            .expect("restored pane")
            .preset_name,
        "shell"
    );
    assert_eq!(
        restored_snapshot.session_geometry,
        Some(humu::shared::render::SessionGeometrySnapshot {
            cols: 100,
            rows: 30,
        })
    );
}
