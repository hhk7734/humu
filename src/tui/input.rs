use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Terminal,
    Locked,
    Pane,
    Workspace,
    Room,
}

#[derive(Debug, Clone)]
pub enum Action {
    EnterMode(Mode),
    NewPane,
    SplitDown,
    SplitRight,
    ClosePane,
    MoveFocus(Direction),
    ToggleFullscreen,
    NewTab,
    CloseTab,
    PrevTab,
    NextTab,
    GoToTab(usize),
    FocusWorkspacePanel,
    FocusRoomPanel,
    NavigateUp,
    NavigateDown,
    Select,
    Create,
    Delete,
    Resize(Direction),
    PassThrough(KeyEvent),
    Quit,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Down,
    Up,
    Right,
}

pub fn handle_key(mode: Mode, key: KeyEvent) -> Action {
    match mode {
        Mode::Terminal => handle_terminal(key),
        Mode::Locked => handle_locked(key),
        Mode::Pane => handle_pane(key),
        Mode::Workspace => handle_workspace(key),
        Mode::Room => handle_room(key),
    }
}

fn handle_terminal(key: KeyEvent) -> Action {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('g') => Action::EnterMode(Mode::Locked),
            KeyCode::Char('p') => Action::EnterMode(Mode::Pane),
            KeyCode::Char('w') => Action::EnterMode(Mode::Workspace),
            KeyCode::Char('r') => Action::EnterMode(Mode::Room),
            KeyCode::Char('q') => Action::Quit,
            _ => Action::PassThrough(key),
        }
    } else if key.modifiers.contains(KeyModifiers::ALT) {
        match key.code {
            KeyCode::Left => Action::MoveFocus(Direction::Left),
            KeyCode::Right => Action::MoveFocus(Direction::Right),
            KeyCode::Down => Action::NavigateDown,
            KeyCode::Up => Action::NavigateUp,
            _ => Action::PassThrough(key),
        }
    } else {
        Action::PassThrough(key)
    }
}

fn handle_locked(key: KeyEvent) -> Action {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('g') {
        Action::EnterMode(Mode::Terminal)
    } else {
        Action::PassThrough(key)
    }
}

fn handle_pane(key: KeyEvent) -> Action {
    if let Some(action) = check_mode_switch(Mode::Pane, key) {
        return action;
    }
    if let Some(action) = check_shared_alt(key) {
        return action;
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        match key.code {
            KeyCode::Left => return Action::Resize(Direction::Left),
            KeyCode::Down => return Action::Resize(Direction::Down),
            KeyCode::Up => return Action::Resize(Direction::Up),
            KeyCode::Right => return Action::Resize(Direction::Right),
            _ => {}
        }
    }
    match key.code {
        KeyCode::Char('n') => Action::NewPane,
        KeyCode::Char('d') => Action::SplitDown,
        KeyCode::Char('r') => Action::SplitRight,
        KeyCode::Char('x') => Action::ClosePane,
        KeyCode::Left => Action::MoveFocus(Direction::Left),
        KeyCode::Down => Action::MoveFocus(Direction::Down),
        KeyCode::Up => Action::MoveFocus(Direction::Up),
        KeyCode::Right => Action::MoveFocus(Direction::Right),
        KeyCode::Char('f') => Action::ToggleFullscreen,
        KeyCode::Char('t') => Action::NewTab,
        KeyCode::Char('c') => Action::CloseTab,
        KeyCode::Char('[') => Action::PrevTab,
        KeyCode::Char(']') => Action::NextTab,
        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
            Action::GoToTab((c as usize) - ('1' as usize))
        }
        KeyCode::Esc => Action::EnterMode(Mode::Terminal),
        _ => Action::None,
    }
}

fn handle_workspace(key: KeyEvent) -> Action {
    if let Some(action) = check_mode_switch(Mode::Workspace, key) {
        return action;
    }
    if let Some(action) = check_shared_alt(key) {
        return action;
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        match key.code {
            KeyCode::Left => return Action::Resize(Direction::Left),
            KeyCode::Right => return Action::Resize(Direction::Right),
            _ => {}
        }
    }
    match key.code {
        KeyCode::Down => Action::NavigateDown,
        KeyCode::Up => Action::NavigateUp,
        KeyCode::Enter => Action::Select,
        KeyCode::Char('n') => Action::Create,
        KeyCode::Char('x') => Action::Delete,
        KeyCode::Esc => Action::EnterMode(Mode::Terminal),
        _ => Action::None,
    }
}

fn handle_room(key: KeyEvent) -> Action {
    if let Some(action) = check_mode_switch(Mode::Room, key) {
        return action;
    }
    if let Some(action) = check_shared_alt(key) {
        return action;
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        match key.code {
            KeyCode::Left => return Action::Resize(Direction::Left),
            KeyCode::Right => return Action::Resize(Direction::Right),
            _ => {}
        }
    }
    match key.code {
        KeyCode::Down => Action::NavigateDown,
        KeyCode::Up => Action::NavigateUp,
        KeyCode::Enter => Action::Select,
        KeyCode::Char('n') => Action::Create,
        KeyCode::Char('x') => Action::Delete,
        KeyCode::Esc => Action::EnterMode(Mode::Terminal),
        _ => Action::None,
    }
}

/// Universal mode switching via Ctrl+key. Pressing the same Ctrl+key that
/// entered the current mode toggles back to Terminal.
fn check_mode_switch(current: Mode, key: KeyEvent) -> Option<Action> {
    if !key.modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }
    match key.code {
        KeyCode::Char('w') => Some(if current == Mode::Workspace {
            Action::EnterMode(Mode::Terminal)
        } else {
            Action::EnterMode(Mode::Workspace)
        }),
        KeyCode::Char('r') => Some(if current == Mode::Room {
            Action::EnterMode(Mode::Terminal)
        } else {
            Action::EnterMode(Mode::Room)
        }),
        KeyCode::Char('t') => Some(Action::EnterMode(Mode::Terminal)),
        KeyCode::Char('p') => Some(if current == Mode::Pane {
            Action::EnterMode(Mode::Terminal)
        } else {
            Action::EnterMode(Mode::Pane)
        }),
        _ => None,
    }
}

fn check_shared_alt(key: KeyEvent) -> Option<Action> {
    if !key.modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    match key.code {
        KeyCode::Left => Some(Action::MoveFocus(Direction::Left)),
        KeyCode::Right => Some(Action::MoveFocus(Direction::Right)),
        KeyCode::Down => Some(Action::NavigateDown),
        KeyCode::Up => Some(Action::NavigateUp),
        _ => None,
    }
}
