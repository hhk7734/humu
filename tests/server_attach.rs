mod support;

use humu::id::PaneId;
use humu::shared::protocol::{
    encode_frame, decode_frame, ClientAction, ClientRequest, FrameDecoder, NavigationDirection,
    ServerEvent, ServerResponse, SessionListEntry,
};
use humu::shared::render::{ColorSnapshot, DetachReason, FullSnapshot};
use std::process::Command;
use uuid::Uuid;

fn pane_id(raw: &str) -> PaneId {
    PaneId(Uuid::parse_str(raw).unwrap())
}

#[test]
fn support_can_spawn_isolated_humu_home() {
    let env = support::isolated_humu_home();
    assert!(env.home.path().exists());
}

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

    let environ = std::fs::read(format!("/proc/{pid}/environ")).expect("server environ");
    let environ = String::from_utf8_lossy(&environ);
    assert!(environ.contains(&format!("HOME={}", env.humu_dir().display())));
    assert!(environ.contains(&format!("HUMU_DIR={}", env.humu_dir().display())));
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
        ServerEvent::LayoutUpdated { pane_geometries, .. } => {
            let geometry = pane_geometries.get(&pane_id).unwrap();
            assert_eq!(geometry.x, 5);
            assert_eq!(geometry.width, 77);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
