use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use humu::tui::input::{handle_key, Action, Direction, Mode};

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
fn terminal_ctrl_r_enters_room() {
    assert!(matches!(
        handle_key(Mode::Terminal, ctrl('r')),
        Action::EnterMode(Mode::Room)
    ));
}

#[test]
fn terminal_ctrl_q_quits() {
    assert!(matches!(handle_key(Mode::Terminal, ctrl('q')), Action::Quit));
}

#[test]
fn terminal_plain_key_passes_through() {
    assert!(matches!(
        handle_key(Mode::Terminal, key(KeyCode::Char('a'))),
        Action::PassThrough(_)
    ));
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
    assert!(matches!(handle_key(Mode::Pane, key(KeyCode::Char('n'))), Action::NewPane));
}

#[test]
fn pane_x_closes() {
    assert!(matches!(handle_key(Mode::Pane, key(KeyCode::Char('x'))), Action::ClosePane));
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
    assert!(matches!(handle_key(Mode::Pane, ctrl('p')), Action::EnterMode(Mode::Terminal)));
}

#[test]
fn pane_f_toggles_fullscreen() {
    assert!(matches!(
        handle_key(Mode::Pane, key(KeyCode::Char('f'))),
        Action::ToggleFullscreen
    ));
}

#[test]
fn pane_tab_management() {
    assert!(matches!(handle_key(Mode::Pane, key(KeyCode::Char('t'))), Action::NewTab));
    assert!(matches!(handle_key(Mode::Pane, key(KeyCode::Char('c'))), Action::CloseTab));
    assert!(matches!(handle_key(Mode::Pane, key(KeyCode::Char('['))), Action::PrevTab));
    assert!(matches!(handle_key(Mode::Pane, key(KeyCode::Char(']'))), Action::NextTab));
    assert!(matches!(
        handle_key(Mode::Pane, key(KeyCode::Char('1'))),
        Action::GoToTab(0)
    ));
    assert!(matches!(
        handle_key(Mode::Pane, key(KeyCode::Char('3'))),
        Action::GoToTab(2)
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
    assert!(matches!(handle_key(Mode::Workspace, key(KeyCode::Char('n'))), Action::Create));
}

#[test]
fn workspace_x_deletes() {
    assert!(matches!(handle_key(Mode::Workspace, key(KeyCode::Char('x'))), Action::Delete));
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
fn workspace_ctrl_w_exits() {
    assert!(matches!(
        handle_key(Mode::Workspace, ctrl('w')),
        Action::EnterMode(Mode::Terminal)
    ));
}

// ── Room mode ───────────────────────────────────────────────────────────────

#[test]
fn room_arrows_navigate() {
    assert!(matches!(
        handle_key(Mode::Room, key(KeyCode::Down)),
        Action::NavigateDown
    ));
    assert!(matches!(
        handle_key(Mode::Room, key(KeyCode::Up)),
        Action::NavigateUp
    ));
}

#[test]
fn room_enter_selects() {
    assert!(matches!(
        handle_key(Mode::Room, key(KeyCode::Enter)),
        Action::Select
    ));
}

#[test]
fn room_n_creates() {
    assert!(matches!(handle_key(Mode::Room, key(KeyCode::Char('n'))), Action::Create));
}

#[test]
fn room_x_deletes() {
    assert!(matches!(handle_key(Mode::Room, key(KeyCode::Char('x'))), Action::Delete));
}

#[test]
fn room_shift_arrows_resize() {
    assert!(matches!(
        handle_key(Mode::Room, shift_arrow(KeyCode::Left)),
        Action::Resize(Direction::Left)
    ));
    assert!(matches!(
        handle_key(Mode::Room, shift_arrow(KeyCode::Right)),
        Action::Resize(Direction::Right)
    ));
}

#[test]
fn room_esc_exits() {
    assert!(matches!(
        handle_key(Mode::Room, key(KeyCode::Esc)),
        Action::EnterMode(Mode::Terminal)
    ));
}

#[test]
fn room_ctrl_r_exits() {
    assert!(matches!(handle_key(Mode::Room, ctrl('r')), Action::EnterMode(Mode::Terminal)));
}

// ── Cross-mode switching ────────────────────────────────────────────────────

#[test]
fn cross_mode_switching() {
    // From Pane: Ctrl+w → Workspace, Ctrl+r → Room, Ctrl+t → Terminal
    assert!(matches!(handle_key(Mode::Pane, ctrl('w')), Action::EnterMode(Mode::Workspace)));
    assert!(matches!(handle_key(Mode::Pane, ctrl('r')), Action::EnterMode(Mode::Room)));
    assert!(matches!(handle_key(Mode::Pane, ctrl('t')), Action::EnterMode(Mode::Terminal)));

    // From Workspace: Ctrl+r → Room, Ctrl+t → Terminal, Ctrl+p → Pane
    assert!(matches!(handle_key(Mode::Workspace, ctrl('r')), Action::EnterMode(Mode::Room)));
    assert!(matches!(handle_key(Mode::Workspace, ctrl('t')), Action::EnterMode(Mode::Terminal)));
    assert!(matches!(handle_key(Mode::Workspace, ctrl('p')), Action::EnterMode(Mode::Pane)));

    // From Room: Ctrl+w → Workspace, Ctrl+t → Terminal, Ctrl+p → Pane
    assert!(matches!(handle_key(Mode::Room, ctrl('w')), Action::EnterMode(Mode::Workspace)));
    assert!(matches!(handle_key(Mode::Room, ctrl('t')), Action::EnterMode(Mode::Terminal)));
    assert!(matches!(handle_key(Mode::Room, ctrl('p')), Action::EnterMode(Mode::Pane)));
}

// ── Shared Alt bindings across sub-modes ────────────────────────────────────

#[test]
fn alt_overrides_in_submodes() {
    for mode in [Mode::Pane, Mode::Workspace, Mode::Room] {
        assert!(
            matches!(handle_key(mode, alt_arrow(KeyCode::Left)), Action::MoveFocus(Direction::Left)),
            "Alt+Left should move focus left in {:?}",
            mode
        );
        assert!(
            matches!(handle_key(mode, alt_arrow(KeyCode::Right)), Action::MoveFocus(Direction::Right)),
            "Alt+Right should move focus right in {:?}",
            mode
        );
        assert!(
            matches!(handle_key(mode, alt_arrow(KeyCode::Down)), Action::NavigateDown),
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
