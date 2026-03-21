use humu::id::{PaneId, RoomId, TabId, WorkspaceId};

#[test]
fn workspace_id_new_is_unique() {
    let a = WorkspaceId::new();
    let b = WorkspaceId::new();
    assert_ne!(a, b);
}

#[test]
fn room_id_new_is_unique() {
    let a = RoomId::new();
    let b = RoomId::new();
    assert_ne!(a, b);
}

#[test]
fn tab_id_new_is_unique() {
    let a = TabId::new();
    let b = TabId::new();
    assert_ne!(a, b);
}

#[test]
fn pane_id_new_is_unique() {
    let a = PaneId::new();
    let b = PaneId::new();
    assert_ne!(a, b);
}

#[test]
fn workspace_id_serde_round_trip() {
    let id = WorkspaceId::new();
    let json = serde_json::to_string(&id).unwrap();
    let parsed: WorkspaceId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, parsed);
}

#[test]
fn room_id_serde_round_trip() {
    let id = RoomId::new();
    let json = serde_json::to_string(&id).unwrap();
    let parsed: RoomId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, parsed);
}

#[test]
fn pane_id_display() {
    let id = PaneId::new();
    let s = format!("{id}");
    assert_eq!(s.len(), 36); // UUID format
}

#[test]
fn workspace_id_display() {
    let id = WorkspaceId::new();
    let s = format!("{id}");
    assert_eq!(s.len(), 36);
}
