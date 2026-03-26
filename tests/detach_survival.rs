mod support;

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
    assert!(harness.child_is_alive());
}
