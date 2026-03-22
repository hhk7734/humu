use humu::pty::pane::PtyPane;
use std::time::Duration;

#[test]
fn pty_spawn_and_read_output() {
    let mut pane = PtyPane::spawn("echo", &["hello".into()], None, 80, 24).unwrap();

    // Give it time to produce output
    std::thread::sleep(Duration::from_millis(500));
    pane.process_output().unwrap();

    let screen = pane.screen_snapshot();
    assert!(
        screen.contents().contains("hello"),
        "screen contents: {:?}",
        screen.contents()
    );
}

#[test]
fn pty_pane_detects_exit() {
    let mut pane = PtyPane::spawn("true", &[], None, 80, 24).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    pane.process_output().unwrap();

    assert!(pane.exit_status().is_some());
}

#[test]
fn pty_pane_replies_to_cursor_position_request() {
    let script = r#"printf '\033[6n'; IFS='[;' read -r -d R _ row col; printf 'ROW=%s COL=%s' "$row" "$col""#;
    let mut pane = PtyPane::spawn("bash", &["-lc".into(), script.into()], None, 80, 24).unwrap();

    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(100));
        pane.process_output().unwrap();
        if pane.exit_status().is_some() {
            break;
        }
    }

    let screen = pane.screen();
    assert!(
        screen.contents().contains("ROW=1 COL=1"),
        "screen contents: {:?}",
        screen.contents()
    );
}

#[test]
fn pty_pane_exposes_terminal_state_accessors() {
    let script = r#"printf '\033[?2004h\033[?1049h\033[?1000h\033[?1005h\033[?1006h\033[?1002h\033[?1003h'; sleep 0.1"#;
    let mut pane = PtyPane::spawn("bash", &["-lc".into(), script.into()], None, 80, 24).unwrap();

    std::thread::sleep(Duration::from_millis(200));
    pane.process_output().unwrap();

    assert!(pane.bracketed_paste());
    assert!(pane.alternate_screen());
    assert!(pane.should_forward_mouse_events());
    assert!(!pane.should_forward_mouse_wheel_events());
    assert!(pane.should_use_local_scrollback_for_page_keys());
    assert_eq!(
        pane.mouse_protocol_mode(),
        humu::pty::terminal::MouseProtocolMode::AnyMotion
    );
    assert_eq!(
        pane.mouse_protocol_encoding(),
        humu::pty::terminal::MouseProtocolEncoding::Sgr
    );

    let script = r#"printf '\033[?1000h'; sleep 0.1"#;
    let mut mouse_pane = PtyPane::spawn("bash", &["-lc".into(), script.into()], None, 80, 24).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    mouse_pane.process_output().unwrap();
    assert!(!mouse_pane.should_use_local_scrollback_for_page_keys());
}

#[test]
fn pty_pane_scrollback_accessors_round_trip() {
    let script = r#"for i in $(seq 1 30); do printf 'line %02d\n' "$i"; done"#;
    let mut pane = PtyPane::spawn("bash", &["-lc".into(), script.into()], None, 80, 24).unwrap();

    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(50));
        pane.process_output().unwrap();
        if pane.exit_status().is_some() {
            break;
        }
    }

    pane.set_scrollback(1);
    assert_eq!(pane.scrollback(), 1);
    pane.scrollback_up(2);
    assert_eq!(pane.scrollback(), 3);
    pane.scrollback_down(1);
    assert_eq!(pane.scrollback(), 2);
    pane.reset_scrollback();
    assert_eq!(pane.scrollback(), 0);
}

#[test]
fn pty_pane_resize_updates_runtime_and_emulator_size() {
    let script = r#"sleep 0.1; set -- $(stty size); printf 'ROWS=%s COLS=%s' "$1" "$2""#;
    let mut pane = PtyPane::spawn("bash", &["-lc".into(), script.into()], None, 80, 24).unwrap();
    pane.resize(100, 30).unwrap();

    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(50));
        pane.process_output().unwrap();
        if pane.exit_status().is_some() {
            break;
        }
    }
    pane.process_output().unwrap();

    assert_eq!(pane.cols(), 100);
    assert_eq!(pane.rows(), 30);
    assert_eq!(pane.screen_snapshot().size(), (30, 100));
    let screen = pane.screen_snapshot();
    assert!(
        screen.contents().contains("ROWS=30 COLS=100"),
        "screen contents: {:?}",
        screen.contents()
    );
}

#[test]
fn pty_pane_does_not_duplicate_queries_across_chunks() {
    let script = r#"printf '\033[6n'; IFS='[;' read -r -d R _ row col; printf 'after-query'; if read -t 0.3 -r -d R _ _ _; then dup=1; else dup=0; fi; printf ' ROW=%s COL=%s DUP=%s' "$row" "$col" "$dup""#;
    let mut pane = PtyPane::spawn("bash", &["-lc".into(), script.into()], None, 80, 24).unwrap();

    for _ in 0..15 {
        std::thread::sleep(Duration::from_millis(50));
        pane.process_output().unwrap();
        if pane.exit_status().is_some() {
            break;
        }
    }
    pane.process_output().unwrap();

    let screen = pane.screen_snapshot();
    assert!(
        screen.contents().contains("ROW=1 COL=1 DUP=0"),
        "screen contents: {:?}",
        screen.contents()
    );
}
