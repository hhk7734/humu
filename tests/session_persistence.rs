mod support;

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
