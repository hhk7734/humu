use humu::id::PaneId;
use humu::shared::protocol::{
    ClientAction, ClientRequest, NavigationDirection, ServerEvent, ServerResponse,
    SessionListEntry,
};
use humu::shared::render::{DetachReason, FullSnapshot};
use uuid::Uuid;

fn pane_id(raw: &str) -> PaneId {
    PaneId(Uuid::parse_str(raw).unwrap())
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
}
