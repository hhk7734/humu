use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::Rect;
use humu::pty::input::{
    InputAction, InputRoute, PaneInputState, route_floating_mouse, route_mouse, route_paste,
    route_passthrough,
};
use humu::pty::terminal::{MouseProtocolEncoding, MouseProtocolMode};
use humu::tui::input::{Action, Direction, Mode, handle_key};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent {
        code: KeyCode::Char(c),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn alt_arrow(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::ALT,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn shift_arrow(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn pane_state(
    mouse_mode: MouseProtocolMode,
    alternate_screen: bool,
    bracketed_paste: bool,
) -> PaneInputState {
    PaneInputState {
        mouse_mode,
        mouse_encoding: MouseProtocolEncoding::Sgr,
        alternate_screen,
        bracketed_paste,
        rows: 24,
    }
}

fn assert_handled(route: InputRoute) -> Vec<InputAction> {
    match route {
        InputRoute::Handled(actions) => actions,
        InputRoute::NotHandled => panic!("expected handled route"),
    }
}

// ── Terminal mode ───────────────────────────────────────────────────────────

#[test]
fn terminal_ctrl_g_enters_locked() {
    assert!(matches!(
        handle_key(Mode::Terminal, ctrl('g')),
        Action::EnterMode(Mode::Locked)
    ));
}

#[test]
fn terminal_ctrl_p_enters_pane() {
    assert!(matches!(
        handle_key(Mode::Terminal, ctrl('p')),
        Action::EnterMode(Mode::Pane)
    ));
}

#[test]
fn terminal_ctrl_w_enters_workspace() {
    assert!(matches!(
        handle_key(Mode::Terminal, ctrl('w')),
        Action::EnterMode(Mode::Workspace)
    ));
}

#[test]
fn terminal_ctrl_q_quits() {
    assert!(matches!(
        handle_key(Mode::Terminal, ctrl('q')),
        Action::Quit
    ));
}

#[test]
fn terminal_plain_key_passes_through() {
    assert!(matches!(
        handle_key(Mode::Terminal, key(KeyCode::Char('a'))),
        Action::PassThrough(_)
    ));
}

// ── PTY input routing ──────────────────────────────────────────────────────

#[test]
fn mouse_reporting_app_writes_mouse_sequence() {
    let route = route_mouse(
        mouse(MouseEventKind::Down(MouseButton::Left), 1, 1),
        Rect::new(0, 0, 10, 10),
        &pane_state(MouseProtocolMode::AnyMotion, false, false),
        false,
        false,
    );

    assert_eq!(
        assert_handled(route),
        vec![InputAction::Write(b"\x1b[<0;1;1M".to_vec())]
    );
}

#[test]
fn non_mouse_app_starts_local_selection_on_left_down() {
    let route = route_mouse(
        mouse(MouseEventKind::Down(MouseButton::Left), 2, 3),
        Rect::new(0, 0, 10, 10),
        &pane_state(MouseProtocolMode::None, false, false),
        false,
        false,
    );

    assert_eq!(
        assert_handled(route),
        vec![InputAction::StartSelection { row: 2, col: 1 }]
    );
}

#[test]
fn non_mouse_app_updates_local_selection_on_drag() {
    let route = route_mouse(
        mouse(MouseEventKind::Drag(MouseButton::Left), 4, 5),
        Rect::new(0, 0, 10, 10),
        &pane_state(MouseProtocolMode::None, false, false),
        false,
        true,
    );

    assert_eq!(
        assert_handled(route),
        vec![InputAction::UpdateSelection { row: 4, col: 3 }]
    );
}

#[test]
fn non_mouse_app_finishes_local_selection_on_release() {
    let route = route_mouse(
        mouse(MouseEventKind::Up(MouseButton::Left), 4, 5),
        Rect::new(0, 0, 10, 10),
        &pane_state(MouseProtocolMode::None, false, false),
        false,
        true,
    );

    assert_eq!(assert_handled(route), vec![InputAction::FinishSelection]);
}

#[test]
fn alternate_screen_app_keeps_page_keys_local() {
    let route = route_passthrough(
        KeyEvent {
            code: KeyCode::PageUp,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        },
        &pane_state(MouseProtocolMode::AnyMotion, true, false),
    );

    assert_eq!(
        assert_handled(route),
        vec![InputAction::AdjustScrollback { lines: 24, up: true }]
    );
}

#[test]
fn non_mouse_app_keeps_page_keys_local() {
    let route = route_passthrough(
        KeyEvent {
            code: KeyCode::PageDown,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        },
        &pane_state(MouseProtocolMode::None, false, false),
    );

    assert_eq!(
        assert_handled(route),
        vec![InputAction::AdjustScrollback { lines: 24, up: false }]
    );
}

#[test]
fn unmapped_passthrough_key_still_resets_scrollback() {
    let route = route_passthrough(
        KeyEvent {
            code: KeyCode::Insert,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        },
        &pane_state(MouseProtocolMode::None, false, false),
    );

    assert_eq!(assert_handled(route), vec![InputAction::ResetScrollback]);
}

#[test]
fn mouse_reporting_app_forwards_wheel_events() {
    let route = route_mouse(
        mouse(MouseEventKind::ScrollUp, 1, 1),
        Rect::new(0, 0, 10, 10),
        &pane_state(MouseProtocolMode::AnyMotion, false, false),
        false,
        false,
    );

    assert_eq!(
        assert_handled(route),
        vec![InputAction::Write(b"\x1b[<64;1;1M".to_vec())]
    );
}

#[test]
fn non_mouse_app_uses_local_scrollback_for_wheel_events() {
    let route = route_mouse(
        mouse(MouseEventKind::ScrollDown, 1, 1),
        Rect::new(0, 0, 10, 10),
        &pane_state(MouseProtocolMode::None, false, false),
        false,
        false,
    );

    assert_eq!(
        assert_handled(route),
        vec![InputAction::AdjustScrollback { lines: 3, up: false }]
    );
}

#[test]
fn alternate_screen_app_uses_local_scrollback_for_wheel_events() {
    let route = route_mouse(
        mouse(MouseEventKind::ScrollUp, 1, 1),
        Rect::new(0, 0, 10, 10),
        &pane_state(MouseProtocolMode::AnyMotion, true, false),
        false,
        false,
    );

    assert_eq!(
        assert_handled(route),
        vec![InputAction::AdjustScrollback { lines: 3, up: true }]
    );
}

#[test]
fn floating_non_mouse_wheel_is_translated_to_jk() {
    let route = route_floating_mouse(
        mouse(MouseEventKind::ScrollDown, 1, 1),
        Rect::new(0, 0, 10, 10),
        &pane_state(MouseProtocolMode::None, false, false),
    );

    assert_eq!(
        assert_handled(route),
        vec![InputAction::Write(b"jjj".to_vec())]
    );
}

#[test]
fn bracketed_paste_is_wrapped() {
    let route = route_paste("hello", &pane_state(MouseProtocolMode::None, false, true));

    assert_eq!(
        assert_handled(route),
        vec![
            InputAction::ResetScrollback,
            InputAction::Write(b"\x1b[200~hello\x1b[201~".to_vec()),
        ]
    );
}

#[test]
fn plain_paste_is_forwarded_raw() {
    let route = route_paste("hello", &pane_state(MouseProtocolMode::None, false, false));

    assert_eq!(
        assert_handled(route),
        vec![InputAction::ResetScrollback, InputAction::Write(b"hello".to_vec())]
    );
}

#[test]
fn terminal_alt_arrows() {
    assert!(matches!(
        handle_key(Mode::Terminal, alt_arrow(KeyCode::Left)),
        Action::MoveFocus(Direction::Left)
    ));
    assert!(matches!(
        handle_key(Mode::Terminal, alt_arrow(KeyCode::Right)),
        Action::MoveFocus(Direction::Right)
    ));
    assert!(matches!(
        handle_key(Mode::Terminal, alt_arrow(KeyCode::Down)),
        Action::NavigateDown
    ));
    assert!(matches!(
        handle_key(Mode::Terminal, alt_arrow(KeyCode::Up)),
        Action::NavigateUp
    ));
}

// ── Locked mode ─────────────────────────────────────────────────────────────

#[test]
fn locked_ctrl_g_unlocks() {
    assert!(matches!(
        handle_key(Mode::Locked, ctrl('g')),
        Action::EnterMode(Mode::Terminal)
    ));
}

#[test]
fn locked_other_keys_pass_through() {
    assert!(matches!(
        handle_key(Mode::Locked, key(KeyCode::Char('a'))),
        Action::PassThrough(_)
    ));
}

// ── Pane mode ───────────────────────────────────────────────────────────────

#[test]
fn pane_n_creates_new() {
    assert!(matches!(
        handle_key(Mode::Pane, key(KeyCode::Char('n'))),
        Action::NewPane
    ));
}

#[test]
fn pane_d_deletes() {
    assert!(matches!(
        handle_key(Mode::Pane, key(KeyCode::Char('d'))),
        Action::ClosePane
    ));
}

#[test]
fn pane_arrows_move_focus() {
    assert!(matches!(
        handle_key(Mode::Pane, key(KeyCode::Left)),
        Action::MoveFocus(Direction::Left)
    ));
    assert!(matches!(
        handle_key(Mode::Pane, key(KeyCode::Down)),
        Action::MoveFocus(Direction::Down)
    ));
    assert!(matches!(
        handle_key(Mode::Pane, key(KeyCode::Up)),
        Action::MoveFocus(Direction::Up)
    ));
    assert!(matches!(
        handle_key(Mode::Pane, key(KeyCode::Right)),
        Action::MoveFocus(Direction::Right)
    ));
}

#[test]
fn pane_shift_arrows_resize() {
    assert!(matches!(
        handle_key(Mode::Pane, shift_arrow(KeyCode::Left)),
        Action::Resize(Direction::Left)
    ));
    assert!(matches!(
        handle_key(Mode::Pane, shift_arrow(KeyCode::Down)),
        Action::Resize(Direction::Down)
    ));
    assert!(matches!(
        handle_key(Mode::Pane, shift_arrow(KeyCode::Up)),
        Action::Resize(Direction::Up)
    ));
    assert!(matches!(
        handle_key(Mode::Pane, shift_arrow(KeyCode::Right)),
        Action::Resize(Direction::Right)
    ));
}

#[test]
fn pane_esc_exits() {
    assert!(matches!(
        handle_key(Mode::Pane, key(KeyCode::Esc)),
        Action::EnterMode(Mode::Terminal)
    ));
}

#[test]
fn pane_ctrl_p_exits() {
    assert!(matches!(
        handle_key(Mode::Pane, ctrl('p')),
        Action::EnterMode(Mode::Terminal)
    ));
}

#[test]
fn pane_f_toggles_fullscreen() {
    assert!(matches!(
        handle_key(Mode::Pane, key(KeyCode::Char('f'))),
        Action::ToggleFullscreen
    ));
}

// ── Tab mode ────────────────────────────────────────────────────────────────

#[test]
fn terminal_ctrl_t_enters_tab() {
    assert!(matches!(
        handle_key(Mode::Terminal, ctrl('t')),
        Action::EnterMode(Mode::Tab)
    ));
}

#[test]
fn tab_n_creates_new() {
    assert!(matches!(
        handle_key(Mode::Tab, key(KeyCode::Char('n'))),
        Action::NewTab
    ));
}

#[test]
fn tab_d_deletes() {
    assert!(matches!(
        handle_key(Mode::Tab, key(KeyCode::Char('d'))),
        Action::CloseTab
    ));
}

#[test]
fn tab_arrows_prev_next() {
    assert!(matches!(
        handle_key(Mode::Tab, key(KeyCode::Left)),
        Action::PrevTab
    ));
    assert!(matches!(
        handle_key(Mode::Tab, key(KeyCode::Right)),
        Action::NextTab
    ));
}

#[test]
fn tab_digits_goto() {
    assert!(matches!(
        handle_key(Mode::Tab, key(KeyCode::Char('1'))),
        Action::GoToTab(0)
    ));
    assert!(matches!(
        handle_key(Mode::Tab, key(KeyCode::Char('3'))),
        Action::GoToTab(2)
    ));
}

#[test]
fn tab_esc_exits() {
    assert!(matches!(
        handle_key(Mode::Tab, key(KeyCode::Esc)),
        Action::EnterMode(Mode::Terminal)
    ));
}

#[test]
fn tab_ctrl_t_exits() {
    assert!(matches!(
        handle_key(Mode::Tab, ctrl('t')),
        Action::EnterMode(Mode::Terminal)
    ));
}

// ── Workspace mode ──────────────────────────────────────────────────────────

#[test]
fn workspace_arrows_navigate() {
    assert!(matches!(
        handle_key(Mode::Workspace, key(KeyCode::Down)),
        Action::NavigateDown
    ));
    assert!(matches!(
        handle_key(Mode::Workspace, key(KeyCode::Up)),
        Action::NavigateUp
    ));
}

#[test]
fn workspace_enter_selects() {
    assert!(matches!(
        handle_key(Mode::Workspace, key(KeyCode::Enter)),
        Action::Select
    ));
}

#[test]
fn workspace_n_creates() {
    assert!(matches!(
        handle_key(Mode::Workspace, key(KeyCode::Char('n'))),
        Action::Create
    ));
}

#[test]
fn workspace_d_deletes() {
    assert!(matches!(
        handle_key(Mode::Workspace, key(KeyCode::Char('d'))),
        Action::Delete
    ));
}

#[test]
fn workspace_shift_arrows_resize() {
    assert!(matches!(
        handle_key(Mode::Workspace, shift_arrow(KeyCode::Left)),
        Action::Resize(Direction::Left)
    ));
    assert!(matches!(
        handle_key(Mode::Workspace, shift_arrow(KeyCode::Right)),
        Action::Resize(Direction::Right)
    ));
}

#[test]
fn workspace_esc_exits() {
    assert!(matches!(
        handle_key(Mode::Workspace, key(KeyCode::Esc)),
        Action::EnterMode(Mode::Terminal)
    ));
}

// ── Cross-mode switching ────────────────────────────────────────────────────

#[test]
fn cross_mode_switching() {
    // From Pane: Ctrl+w → Workspace, Ctrl+t → Terminal
    assert!(matches!(
        handle_key(Mode::Pane, ctrl('w')),
        Action::EnterMode(Mode::Workspace)
    ));
    assert!(matches!(
        handle_key(Mode::Pane, ctrl('t')),
        Action::EnterMode(Mode::Terminal)
    ));

    // From Tab: Ctrl+w → Workspace, Ctrl+p → Pane
    assert!(matches!(
        handle_key(Mode::Tab, ctrl('w')),
        Action::EnterMode(Mode::Workspace)
    ));
    assert!(matches!(
        handle_key(Mode::Tab, ctrl('p')),
        Action::EnterMode(Mode::Pane)
    ));
}

// ── Shared Alt bindings across sub-modes ────────────────────────────────────

#[test]
fn alt_overrides_in_submodes() {
    for mode in [Mode::Pane, Mode::Tab] {
        assert!(
            matches!(
                handle_key(mode, alt_arrow(KeyCode::Left)),
                Action::MoveFocus(Direction::Left)
            ),
            "Alt+Left should move focus left in {:?}",
            mode
        );
        assert!(
            matches!(
                handle_key(mode, alt_arrow(KeyCode::Right)),
                Action::MoveFocus(Direction::Right)
            ),
            "Alt+Right should move focus right in {:?}",
            mode
        );
        assert!(
            matches!(
                handle_key(mode, alt_arrow(KeyCode::Down)),
                Action::NavigateDown
            ),
            "Alt+Down should navigate down in {:?}",
            mode
        );
        assert!(
            matches!(handle_key(mode, alt_arrow(KeyCode::Up)), Action::NavigateUp),
            "Alt+Up should navigate up in {:?}",
            mode
        );
    }
}

// ── EnterSearch mode ─────────────────────────────────────────────────────────

#[test]
fn terminal_ctrl_f_enters_search() {
    let action = handle_key(Mode::Terminal, ctrl('f'));
    assert!(matches!(action, Action::EnterMode(Mode::EnterSearch)));
}

#[test]
fn enter_search_enter_confirms() {
    let action = handle_key(Mode::EnterSearch, key(KeyCode::Enter));
    assert!(matches!(action, Action::SearchConfirm));
}

#[test]
fn enter_search_esc_cancels() {
    let action = handle_key(Mode::EnterSearch, key(KeyCode::Esc));
    assert!(matches!(action, Action::SearchCancel));
}

#[test]
fn enter_search_char_is_input() {
    let action = handle_key(Mode::EnterSearch, key(KeyCode::Char('a')));
    assert!(matches!(action, Action::SearchInput(_)));
}

#[test]
fn enter_search_ctrl_w_switches_mode() {
    let action = handle_key(Mode::EnterSearch, ctrl('w'));
    assert!(matches!(action, Action::EnterMode(Mode::Workspace)));
}

// ── Search mode ──────────────────────────────────────────────────────────────

#[test]
fn search_n_goes_next() {
    let action = handle_key(Mode::Search, key(KeyCode::Char('n')));
    assert!(matches!(action, Action::SearchNext));
}

#[test]
fn search_shift_n_goes_prev() {
    let k = KeyEvent {
        code: KeyCode::Char('N'),
        modifiers: KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    let action = handle_key(Mode::Search, k);
    assert!(matches!(action, Action::SearchPrev));
}

#[test]
fn search_c_toggles_case() {
    let action = handle_key(Mode::Search, key(KeyCode::Char('c')));
    assert!(matches!(action, Action::SearchToggleCase));
}

#[test]
fn search_w_toggles_wrap() {
    let action = handle_key(Mode::Search, key(KeyCode::Char('w')));
    assert!(matches!(action, Action::SearchToggleWrap));
}

#[test]
fn search_esc_cancels() {
    let action = handle_key(Mode::Search, key(KeyCode::Esc));
    assert!(matches!(action, Action::SearchCancel));
}

#[test]
fn search_arrows_scroll() {
    assert!(matches!(
        handle_key(Mode::Search, key(KeyCode::Up)),
        Action::ScrollUp
    ));
    assert!(matches!(
        handle_key(Mode::Search, key(KeyCode::Down)),
        Action::ScrollDown
    ));
    assert!(matches!(
        handle_key(Mode::Search, key(KeyCode::PageUp)),
        Action::ScrollPageUp
    ));
    assert!(matches!(
        handle_key(Mode::Search, key(KeyCode::PageDown)),
        Action::ScrollPageDown
    ));
}
