mod support;

#[test]
fn support_can_spawn_background_pty_fixture() {
    let mut harness = support::spawn_sleeping_shell();
    assert!(harness.child_is_alive());
}
