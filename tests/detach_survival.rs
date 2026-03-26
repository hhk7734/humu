#[path = "../src/server/mod.rs"]
mod server_impl;

mod support;
use std::time::Duration;

use humu::config::NotificationsConfig;
use humu::shared::render::FullSnapshot;

#[test]
fn attach_snapshot_replaces_local_layout_metadata() {
    use humu::config::HumuState;
    use humu::tui::layout::{PaneId, SplitTree};

    let state = HumuState::default();
    let mut app = support::App::test_with_state(state, support::temp_state_path());
    let local_pane_id = PaneId::new();
    app.tabs.add_tab("local".into(), SplitTree::leaf(local_pane_id));
    app.focused_pane = Some(local_pane_id);
    app.fullscreen_pane = Some(local_pane_id);

    let snapshot = FullSnapshot::fixture();
    let expected_focused = snapshot.focused_pane_id;
    let expected_fullscreen = snapshot.fullscreen_pane_id;
    app.test_hydrate_attached_snapshot(snapshot);

    assert_eq!(app.tabs.active_name(), "shell");
    assert_eq!(app.tabs.active_index(), 0);
    assert_eq!(app.focused_pane, expected_focused);
    assert_eq!(app.fullscreen_pane, expected_fullscreen);
}

#[test]
fn runtime_emits_snapshot_from_server_owned_terminal_state() {
    let env = support::isolated_humu_home();
    let runtime = server_impl::runtime::SessionRuntime::start(
        env.humu_dir().to_path_buf(),
        NotificationsConfig::default(),
        env.home.path().join(".codex/sessions"),
    )
    .expect("start runtime");

    let mut snapshot = FullSnapshot::fixture();
    if let Some(pane) = snapshot.panes.values_mut().next() {
        pane.screen.cells[0][0].text = "hello".to_string();
    }
    runtime.set_session_snapshot("default", snapshot.clone());

    let emitted = runtime.snapshot_for_session("default", FullSnapshot::fixture());
    assert!(
        emitted
            .panes
            .values()
            .flat_map(|pane| pane.screen.cells.iter())
            .flatten()
            .any(|cell| cell.text.contains("hello")),
        "expected runtime-owned snapshot to win over the fallback fixture"
    );
}

#[test]
fn support_can_spawn_background_pty_fixture() {
    let mut harness = support::spawn_sleeping_shell();
    assert!(harness.child_is_alive());
}

#[test]
fn support_can_spawn_terminal_backed_attach_client() {
    let _: fn(&support::TestEnv, &str) -> support::PtyHarness = support::spawn_humu_attach;

    let env = support::isolated_humu_home();
    let mut harness = support::spawn_humu_attach(&env, "default");
    assert!(harness.wait_for_output("\u{1b}[?1049h", Duration::from_secs(2)));
    assert!(harness.child_is_alive());
    assert!(env.humu_dir().join("hooks/claude-settings.json").exists());
    assert!(env.log_path().exists());
}

#[test]
fn support_can_spawn_terminal_backed_attach_client_with_explicit_size() {
    let _: fn(&support::TestEnv, &str, u16, u16) -> support::PtyHarness =
        support::spawn_humu_attach_with_size;

    let env = support::isolated_humu_home();
    let mut harness = support::spawn_humu_attach_with_size(&env, "default", 120, 40);
    assert!(harness.wait_for_output("\u{1b}[?1049h", Duration::from_secs(2)));
    assert!(harness.child_is_alive());
}

#[test]
fn pty_harness_write_input_round_trips_to_child() {
    let mut harness = support::PtyHarness::spawn(
        "bash",
        &[
            "-lc".to_string(),
            "read line; printf 'INPUT:%s\\n' \"$line\"".to_string(),
        ],
        None,
        80,
        24,
        &[],
    );

    harness.write_input(b"hello from harness\n");
    assert!(harness.wait_for_output("INPUT:hello from harness", Duration::from_secs(2)));
}

#[test]
fn pty_harness_resize_changes_child_terminal_size() {
    let mut harness = support::PtyHarness::spawn(
        "bash",
        &[
            "-lc".to_string(),
            "sleep 0.2; stty size | awk '{printf \"ROWS=%s COLS=%s\", $1, $2}'".to_string(),
        ],
        None,
        80,
        24,
        &[],
    );

    harness.resize(120, 40);
    assert!(harness.wait_for_output("ROWS=40 COLS=120", Duration::from_secs(2)));
}

#[test]
fn pty_harness_wait_for_output_detects_delayed_child_output() {
    let mut harness = support::PtyHarness::spawn(
        "bash",
        &[
            "-lc".to_string(),
            "sleep 0.2; printf 'delayed-output\\n'".to_string(),
        ],
        None,
        80,
        24,
        &[],
    );

    assert!(harness.wait_for_output("delayed-output", Duration::from_secs(2)));
}
