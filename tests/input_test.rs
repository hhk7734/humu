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

fn alt(c: char) -> KeyEvent {
    KeyEvent {
        code: KeyCode::Char(c),
        modifiers: KeyModifiers::ALT,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

// ── Normal mode ──────────────────────────────────────────────────────────────

#[test]
fn normal_ctrl_w_enters_workspace() {
    assert!(matches!(
        handle_key(Mode::Normal, ctrl('w')),
        Action::EnterMode(Mode::Workspace)
    ));
}

#[test]
fn normal_ctrl_p_enters_pane() {
    assert!(matches!(
        handle_key(Mode::Normal, ctrl('p')),
        Action::EnterMode(Mode::Pane)
    ));
}

#[test]
fn normal_ctrl_t_enters_tab() {
    assert!(matches!(
        handle_key(Mode::Normal, ctrl('t')),
        Action::EnterMode(Mode::Tab)
    ));
}

#[test]
fn normal_ctrl_g_enters_locked() {
    assert!(matches!(
        handle_key(Mode::Normal, ctrl('g')),
        Action::EnterMode(Mode::Locked)
    ));
}

#[test]
fn normal_ctrl_n_enters_resize() {
    assert!(matches!(
        handle_key(Mode::Normal, ctrl('n')),
        Action::EnterMode(Mode::Resize)
    ));
}

#[test]
fn normal_ctrl_q_quits() {
    assert!(matches!(handle_key(Mode::Normal, ctrl('q')), Action::Quit));
}

#[test]
fn normal_plain_key_passes_through() {
    assert!(matches!(
        handle_key(Mode::Normal, key(KeyCode::Char('a'))),
        Action::PassThrough(_)
    ));
}

#[test]
fn normal_alt_h_moves_focus_left() {
    assert!(matches!(
        handle_key(Mode::Normal, alt('h')),
        Action::MoveFocus(Direction::Left)
    ));
}

#[test]
fn normal_alt_j_navigates_down() {
    assert!(matches!(
        handle_key(Mode::Normal, alt('j')),
        Action::NavigateDown
    ));
}

// ── Locked mode ──────────────────────────────────────────────────────────────

#[test]
fn locked_ctrl_g_unlocks() {
    assert!(matches!(
        handle_key(Mode::Locked, ctrl('g')),
        Action::EnterMode(Mode::Normal)
    ));
}

#[test]
fn locked_other_keys_pass_through() {
    assert!(matches!(
        handle_key(Mode::Locked, key(KeyCode::Char('a'))),
        Action::PassThrough(_)
    ));
}

// ── Pane mode ────────────────────────────────────────────────────────────────

#[test]
fn pane_n_creates_new() {
    assert!(matches!(handle_key(Mode::Pane, key(KeyCode::Char('n'))), Action::NewPane));
}

#[test]
fn pane_x_closes() {
    assert!(matches!(handle_key(Mode::Pane, key(KeyCode::Char('x'))), Action::ClosePane));
}

#[test]
fn pane_hjkl_moves_focus() {
    assert!(matches!(
        handle_key(Mode::Pane, key(KeyCode::Char('h'))),
        Action::MoveFocus(Direction::Left)
    ));
    assert!(matches!(
        handle_key(Mode::Pane, key(KeyCode::Char('j'))),
        Action::MoveFocus(Direction::Down)
    ));
    assert!(matches!(
        handle_key(Mode::Pane, key(KeyCode::Char('k'))),
        Action::MoveFocus(Direction::Up)
    ));
    assert!(matches!(
        handle_key(Mode::Pane, key(KeyCode::Char('l'))),
        Action::MoveFocus(Direction::Right)
    ));
}

#[test]
fn pane_esc_exits() {
    assert!(matches!(
        handle_key(Mode::Pane, key(KeyCode::Esc)),
        Action::ExitToNormal
    ));
}

#[test]
fn pane_ctrl_p_exits() {
    assert!(matches!(handle_key(Mode::Pane, ctrl('p')), Action::ExitToNormal));
}

#[test]
fn pane_f_toggles_fullscreen() {
    assert!(matches!(
        handle_key(Mode::Pane, key(KeyCode::Char('f'))),
        Action::ToggleFullscreen
    ));
}

// ── Tab mode ─────────────────────────────────────────────────────────────────

#[test]
fn tab_n_creates_new() {
    assert!(matches!(handle_key(Mode::Tab, key(KeyCode::Char('n'))), Action::NewTab));
}

#[test]
fn tab_h_prev_l_next() {
    assert!(matches!(
        handle_key(Mode::Tab, key(KeyCode::Char('h'))),
        Action::PrevTab
    ));
    assert!(matches!(
        handle_key(Mode::Tab, key(KeyCode::Char('l'))),
        Action::NextTab
    ));
}

#[test]
fn tab_digit_goes_to_tab() {
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
fn tab_ctrl_t_exits() {
    assert!(matches!(handle_key(Mode::Tab, ctrl('t')), Action::ExitToNormal));
}

// ── Workspace mode ───────────────────────────────────────────────────────────

#[test]
fn workspace_jk_navigates() {
    assert!(matches!(
        handle_key(Mode::Workspace, key(KeyCode::Char('j'))),
        Action::NavigateDown
    ));
    assert!(matches!(
        handle_key(Mode::Workspace, key(KeyCode::Char('k'))),
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
fn workspace_hl_switches_panels() {
    assert!(matches!(
        handle_key(Mode::Workspace, key(KeyCode::Char('h'))),
        Action::FocusWorkspacePanel
    ));
    assert!(matches!(
        handle_key(Mode::Workspace, key(KeyCode::Char('l'))),
        Action::FocusRoomPanel
    ));
}

#[test]
fn workspace_ctrl_w_exits() {
    assert!(matches!(
        handle_key(Mode::Workspace, ctrl('w')),
        Action::ExitToNormal
    ));
}

// ── Resize mode ──────────────────────────────────────────────────────────────

#[test]
fn resize_hjkl_resizes() {
    assert!(matches!(
        handle_key(Mode::Resize, key(KeyCode::Char('h'))),
        Action::Resize(Direction::Left)
    ));
    assert!(matches!(
        handle_key(Mode::Resize, key(KeyCode::Char('j'))),
        Action::Resize(Direction::Down)
    ));
}

#[test]
fn resize_shift_reverses() {
    assert!(matches!(
        handle_key(Mode::Resize, key(KeyCode::Char('H'))),
        Action::ResizeReverse(Direction::Left)
    ));
    assert!(matches!(
        handle_key(Mode::Resize, key(KeyCode::Char('J'))),
        Action::ResizeReverse(Direction::Down)
    ));
}

#[test]
fn resize_ctrl_n_exits() {
    assert!(matches!(
        handle_key(Mode::Resize, ctrl('n')),
        Action::ExitToNormal
    ));
}

// ── Shared Alt bindings across sub-modes ─────────────────────────────────────

#[test]
fn alt_overrides_in_submodes() {
    for mode in [Mode::Pane, Mode::Tab, Mode::Workspace, Mode::Resize] {
        assert!(
            matches!(handle_key(mode, alt('h')), Action::MoveFocus(Direction::Left)),
            "Alt+h should move focus left in {:?}",
            mode
        );
        assert!(
            matches!(handle_key(mode, alt('l')), Action::MoveFocus(Direction::Right)),
            "Alt+l should move focus right in {:?}",
            mode
        );
        assert!(
            matches!(handle_key(mode, alt('j')), Action::NavigateDown),
            "Alt+j should navigate down in {:?}",
            mode
        );
        assert!(
            matches!(handle_key(mode, alt('k')), Action::NavigateUp),
            "Alt+k should navigate up in {:?}",
            mode
        );
    }
}
