use humu::id::{WorkspaceId, RoomId, TabId, PaneId};

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
fn tab_id_sequential() {
    let a = TabId(0);
    let b = TabId(1);
    assert_ne!(a, b);
    assert_eq!(a.0, 0);
    assert_eq!(b.0, 1);
}

#[test]
fn pane_id_sequential() {
    let a = PaneId(0);
    let b = PaneId(1);
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
    let id = PaneId(42);
    assert_eq!(format!("{id}"), "42");
}

#[test]
fn workspace_id_display() {
    let id = WorkspaceId::new();
    let s = format!("{id}");
    // UUID format: 8-4-4-4-12
    assert_eq!(s.len(), 36);
}
