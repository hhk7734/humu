use humu::config::{
    HumuState, PersistedRoomLayout, RoomEntry, SessionState, SplitNode, TabLayout, WorkspaceEntry,
};
use humu::id::{RoomId, WorkspaceId};
use humu::tui::layout::{PaneId, SplitTree};
use std::path::PathBuf;
use uuid::Uuid;

#[path = "../../src/app.rs"]
#[allow(dead_code)]
mod app_impl;
#[path = "../../src/server/persistence.rs"]
pub mod persistence;

pub use app_impl::App;

pub fn workspace_id(name: &str) -> WorkspaceId {
    match name {
        "humu" => WorkspaceId(Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()),
        other => panic!("unknown workspace fixture: {other}"),
    }
}

pub fn room_id(name: &str) -> RoomId {
    match name {
        "main" => RoomId(Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap()),
        "feat-x" => RoomId(Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap()),
        other => panic!("unknown room fixture: {other}"),
    }
}

pub fn legacy_state_fixture() -> HumuState {
    let ws_id = workspace_id("humu");
    let main_room_id = room_id("main");
    let feature_room_id = room_id("feat-x");
    let workspace_path = PathBuf::from("/tmp/humu");

    HumuState {
        active_workspace_id: Some(ws_id),
        active_room_id: Some(main_room_id),
        workspaces: vec![WorkspaceEntry {
            name: "humu".to_string(),
            id: ws_id,
            path: workspace_path.clone(),
            last_room_id: Some(main_room_id),
            rooms: vec![
                RoomEntry {
                    name: "main".to_string(),
                    id: main_room_id,
                    path: workspace_path.clone(),
                    active_tab: Some(0),
                    tabs: vec![TabLayout {
                        name: "shell".to_string(),
                        split: SplitNode::Leaf {
                            preset: "shell".to_string(),
                            session_id: None,
                        },
                    }],
                },
                RoomEntry {
                    name: "feat-x".to_string(),
                    id: feature_room_id,
                    path: workspace_path.join("feat-x"),
                    active_tab: None,
                    tabs: vec![],
                },
            ],
        }],
        panel_widths: Some([24, 20]),
        sessions: vec![],
    }
}

pub fn migrated_state_fixture() -> HumuState {
    persistence::migrate_legacy_state(legacy_state_fixture())
}

pub fn app_with_migrated_state() -> App {
    let mut app = App::test_with_state(migrated_state_fixture(), temp_state_path());
    let pane_id = PaneId::new();
    app.tabs.add_tab("shell".into(), SplitTree::leaf(pane_id));
    app.pane_presets.insert(pane_id, "shell".to_string());
    app.focused_pane = Some(pane_id);
    app
}

pub fn reload_state(app: &mut App) -> HumuState {
    app.test_persist_layout();
    persistence::load_state(app.test_state_path()).expect("reload state")
}

pub fn round_trip_state(state: HumuState) -> HumuState {
    let path = temp_state_path();
    persistence::save_state(&path, &state).expect("save state");
    persistence::load_state(&path).expect("load state")
}

pub fn persist_named_session_layout(
    state: &mut HumuState,
    session_name: &str,
    room_name: &str,
    tab_name: &str,
) {
    let ws_id = workspace_id("humu");
    let room_id = room_id(room_name);
    let session = state.ensure_session(session_name);
    session.active_workspace_id = Some(ws_id);
    session.active_room_id = Some(room_id);
    session.tabs_by_room.insert(
        room_id,
        PersistedRoomLayout {
            active_tab: 0,
            tabs: vec![TabLayout {
                name: tab_name.to_string(),
                split: SplitNode::Leaf {
                    preset: "shell".to_string(),
                    session_id: None,
                },
            }],
        },
    );
}

pub fn default_session(state: &HumuState) -> &SessionState {
    state
        .session_by_name(persistence::DEFAULT_SESSION_NAME)
        .expect("default session")
}

fn temp_state_path() -> PathBuf {
    std::env::temp_dir().join(format!("humu-session-persistence-{}.yaml", Uuid::new_v4()))
}
