use humu::pty::pane::PtyPane;
use std::time::Duration;

#[test]
fn test_spawn_and_read_output() {
    let mut pane = PtyPane::spawn("echo", &["hello".into()], None, 80, 24).unwrap();

    // Give it time to produce output
    std::thread::sleep(Duration::from_millis(500));
    pane.process_output().unwrap();

    let screen = pane.screen();
    assert!(
        screen.contents().contains("hello"),
        "screen contents: {:?}",
        screen.contents()
    );
}

#[test]
fn test_pane_detects_exit() {
    let mut pane = PtyPane::spawn("true", &[], None, 80, 24).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    pane.process_output().unwrap();

    assert!(pane.exit_status().is_some());
}
