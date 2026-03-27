use anyhow::Result;
use std::path::Path;

use humu::config::{HumuState, PersistedRoomLayout, SessionSize};
use humu::id::{RoomId, WorkspaceId};
use humu::shared::render::SessionGeometrySnapshot;

pub const DEFAULT_SESSION_NAME: &str = HumuState::DEFAULT_SESSION_NAME;

pub fn migrate_legacy_state(state: HumuState) -> HumuState {
    state.migrate_legacy_layout_state()
}

pub fn load_state(path: &Path) -> Result<HumuState> {
    HumuState::load(path)
}

pub fn save_state(path: &Path, state: &HumuState) -> Result<()> {
    state.save(path)
}

pub fn persist_session_runtime_state(
    path: &Path,
    session_name: &str,
    active_workspace_id: Option<WorkspaceId>,
    active_room_id: Option<RoomId>,
    layout: Option<PersistedRoomLayout>,
    last_size: Option<SessionGeometrySnapshot>,
) -> Result<()> {
    let mut state = if path.exists() {
        load_state(path)?
    } else {
        HumuState::default()
    };
    let session = state.ensure_session(session_name);
    if active_workspace_id.is_some() {
        session.active_workspace_id = active_workspace_id;
    }
    if active_room_id.is_some() {
        session.active_room_id = active_room_id;
    }
    if let Some(room_id) = session.active_room_id {
        match layout {
            Some(layout) => {
                session.tabs_by_room.insert(room_id, layout);
            }
            None => {
                session.tabs_by_room.remove(&room_id);
            }
        }
    }
    session.last_size = last_size.map(|size| SessionSize {
        cols: size.cols,
        rows: size.rows,
    });
    save_state(path, &state)
}

pub fn persist_session_size(
    path: &Path,
    session_name: &str,
    active_workspace_id: Option<WorkspaceId>,
    active_room_id: Option<RoomId>,
    last_size: Option<SessionGeometrySnapshot>,
) -> Result<()> {
    let mut state = if path.exists() {
        load_state(path)?
    } else {
        HumuState::default()
    };
    let session = state.ensure_session(session_name);
    if active_workspace_id.is_some() {
        session.active_workspace_id = active_workspace_id;
    }
    if active_room_id.is_some() {
        session.active_room_id = active_room_id;
    }
    session.last_size = last_size.map(|size| SessionSize {
        cols: size.cols,
        rows: size.rows,
    });
    save_state(path, &state)
}
