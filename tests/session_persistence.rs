mod support;

use humu::config::prune_stale_rooms_for_workspace;
use humu::git::workspace::WorkspaceManager;
use std::collections::HashSet;
use support::persistence::migrate_legacy_state;

#[test]
fn legacy_state_migrates_into_default_session_layouts() {
    let legacy = support::legacy_state_fixture();
    let migrated = migrate_legacy_state(legacy);

    assert!(migrated.sessions.iter().any(|session| session.name == "default"));
    assert!(
        support::default_session(&migrated)
            .tabs_by_room
            .contains_key(&support::room_id("main"))
    );
}

#[test]
fn app_persists_room_layouts_only_via_session_state() {
    let mut app = support::app_with_migrated_state();
    let saved = support::reload_state(&mut app);

    assert!(
        support::default_session(&saved)
            .tabs_by_room
            .contains_key(&support::room_id("main"))
    );
    assert!(saved.workspaces[0].rooms[0].tabs.is_empty());
    assert!(saved.workspaces[0].rooms[0].active_tab.is_none());
}

#[test]
fn named_sessions_persist_independent_room_layouts_and_selection() {
    let mut state = support::migrated_state_fixture();
    support::persist_named_session_layout(&mut state, "default", "main", "shell");
    support::persist_named_session_layout(&mut state, "review", "feat-x", "codex");

    let reloaded = support::round_trip_state(state);

    assert_ne!(
        reloaded.session_by_name("default").unwrap().active_room_id,
        reloaded.session_by_name("review").unwrap().active_room_id
    );
    assert!(
        reloaded
            .session_by_name("default")
            .unwrap()
            .tabs_by_room
            .contains_key(&support::room_id("main"))
    );
    assert!(
        reloaded
            .session_by_name("review")
            .unwrap()
            .tabs_by_room
            .contains_key(&support::room_id("feat-x"))
    );
}

#[test]
fn room_deletion_updates_session_state() {
    let mut state = support::migrated_state_fixture();
    support::persist_named_session_layout(&mut state, "default", "main", "shell");
    support::insert_session_room_layout(&mut state, "default", "feat-x", "scratch");
    support::persist_named_session_layout(&mut state, "review", "feat-x", "codex");

    let mut app = support::App::test_with_state(state, support::temp_state_path());
    app.test_remove_room_state(
        support::workspace_id("humu"),
        support::room_id("feat-x"),
    );

    let default_session = support::session_by_name(&app.state, "default");
    let review_session = support::session_by_name(&app.state, "review");
    assert!(default_session.tabs_by_room.contains_key(&support::room_id("main")));
    assert!(!default_session.tabs_by_room.contains_key(&support::room_id("feat-x")));
    assert_eq!(default_session.active_room_id, Some(support::room_id("main")));
    assert_eq!(review_session.active_room_id, None);
    assert!(!review_session.tabs_by_room.contains_key(&support::room_id("feat-x")));
    assert_eq!(app.state.workspaces[0].rooms.len(), 1);
}

#[test]
fn workspace_deletion_updates_session_state() {
    let mut state = support::migrated_state_fixture();
    support::persist_named_session_layout(&mut state, "default", "main", "shell");
    support::persist_named_session_layout(&mut state, "review", "feat-x", "codex");
    support::insert_session_room_layout(&mut state, "default", "feat-x", "scratch");

    WorkspaceManager::new()
        .delete(&mut state, support::workspace_id("humu"), false)
        .expect("delete workspace");

    assert!(state.workspaces.is_empty());
    assert_eq!(state.active_workspace_id, None);
    assert_eq!(state.active_room_id, None);

    let default_session = support::session_by_name(&state, "default");
    let review_session = support::session_by_name(&state, "review");
    assert_eq!(default_session.active_workspace_id, None);
    assert_eq!(default_session.active_room_id, None);
    assert!(default_session.tabs_by_room.is_empty());
    assert_eq!(review_session.active_workspace_id, None);
    assert_eq!(review_session.active_room_id, None);
    assert!(review_session.tabs_by_room.is_empty());
}

#[test]
fn stale_room_pruning_updates_session_state() {
    let mut state = support::migrated_state_fixture();
    support::persist_named_session_layout(&mut state, "default", "main", "shell");
    support::insert_session_room_layout(&mut state, "default", "feat-x", "scratch");
    support::persist_named_session_layout(&mut state, "review", "feat-x", "codex");

    let (workspace_path, _) = support::workspace_room_paths();
    let discovered = HashSet::from([workspace_path]);
    prune_stale_rooms_for_workspace(&mut state, support::workspace_id("humu"), &discovered);

    let workspace = state
        .ws_by_id(support::workspace_id("humu"))
        .expect("workspace still present");
    assert_eq!(workspace.rooms.len(), 1);
    assert_eq!(workspace.rooms[0].id, support::room_id("main"));

    let default_session = support::session_by_name(&state, "default");
    let review_session = support::session_by_name(&state, "review");
    assert!(default_session.tabs_by_room.contains_key(&support::room_id("main")));
    assert!(!default_session.tabs_by_room.contains_key(&support::room_id("feat-x")));
    assert_eq!(default_session.active_room_id, Some(support::room_id("main")));
    assert_eq!(review_session.active_room_id, None);
    assert!(!review_session.tabs_by_room.contains_key(&support::room_id("feat-x")));
}
