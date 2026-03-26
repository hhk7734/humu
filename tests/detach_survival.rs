mod support;
use std::time::Duration;

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
