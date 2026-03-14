use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Locked,
    Pane,
    Tab,
    Workspace,
    Room,
    Resize,
}

#[derive(Debug, Clone)]
pub enum Action {
    EnterMode(Mode),
    ExitToNormal,
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
    RenameTab,
    FocusWorkspacePanel,
    FocusRoomPanel,
    NavigateUp,
    NavigateDown,
    Select,
    Create,
    Delete,
    Resize(Direction),
    ResizeReverse(Direction),
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
        Mode::Locked => handle_locked(key),
        Mode::Normal => handle_normal(key),
        Mode::Pane => handle_pane(key),
        Mode::Tab => handle_tab(key),
        Mode::Workspace => handle_workspace(key),
        Mode::Room => handle_room(key),
        Mode::Resize => handle_resize(key),
    }
}

fn handle_locked(key: KeyEvent) -> Action {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('g') {
        Action::EnterMode(Mode::Normal)
    } else {
        Action::PassThrough(key)
    }
}

fn handle_normal(key: KeyEvent) -> Action {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('g') => Action::EnterMode(Mode::Locked),
            KeyCode::Char('p') => Action::EnterMode(Mode::Pane),
            KeyCode::Char('t') => Action::EnterMode(Mode::Tab),
            KeyCode::Char('w') => Action::EnterMode(Mode::Workspace),
            KeyCode::Char('r') => Action::EnterMode(Mode::Room),
            KeyCode::Char('n') => Action::EnterMode(Mode::Resize),
            KeyCode::Char('q') => Action::Quit,
            _ => Action::PassThrough(key),
        }
    } else if key.modifiers.contains(KeyModifiers::ALT) {
        match key.code {
            KeyCode::Char('h') | KeyCode::Left => Action::MoveFocus(Direction::Left),
            KeyCode::Char('l') | KeyCode::Right => Action::MoveFocus(Direction::Right),
            KeyCode::Char('j') | KeyCode::Down => Action::NavigateDown,
            KeyCode::Char('k') | KeyCode::Up => Action::NavigateUp,
            _ => Action::PassThrough(key),
        }
    } else {
        Action::PassThrough(key)
    }
}

fn handle_pane(key: KeyEvent) -> Action {
    if let Some(action) = check_shared_alt(key) {
        return action;
    }
    match key.code {
        KeyCode::Char('n') => Action::NewPane,
        KeyCode::Char('d') => Action::SplitDown,
        KeyCode::Char('r') => Action::SplitRight,
        KeyCode::Char('x') => Action::ClosePane,
        KeyCode::Char('h') | KeyCode::Left => Action::MoveFocus(Direction::Left),
        KeyCode::Char('j') | KeyCode::Down => Action::MoveFocus(Direction::Down),
        KeyCode::Char('k') | KeyCode::Up => Action::MoveFocus(Direction::Up),
        KeyCode::Char('l') | KeyCode::Right => Action::MoveFocus(Direction::Right),
        KeyCode::Char('f') => Action::ToggleFullscreen,
        KeyCode::Esc => Action::ExitToNormal,
        _ if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('p') => {
            Action::ExitToNormal
        }
        _ => Action::None,
    }
}

fn handle_tab(key: KeyEvent) -> Action {
    if let Some(action) = check_shared_alt(key) {
        return action;
    }
    match key.code {
        KeyCode::Char('n') => Action::NewTab,
        KeyCode::Char('x') => Action::CloseTab,
        KeyCode::Char('h') | KeyCode::Left => Action::PrevTab,
        KeyCode::Char('l') | KeyCode::Right => Action::NextTab,
        KeyCode::Char('r') => Action::RenameTab,
        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
            Action::GoToTab((c as usize) - ('1' as usize))
        }
        KeyCode::Esc => Action::ExitToNormal,
        _ if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('t') => {
            Action::ExitToNormal
        }
        _ => Action::None,
    }
}

fn handle_workspace(key: KeyEvent) -> Action {
    if let Some(action) = check_shared_alt(key) {
        return action;
    }
    match key.code {
        KeyCode::Char('h') | KeyCode::Left => Action::FocusWorkspacePanel,
        KeyCode::Char('l') | KeyCode::Right => Action::FocusRoomPanel,
        KeyCode::Char('j') | KeyCode::Down => Action::NavigateDown,
        KeyCode::Char('k') | KeyCode::Up => Action::NavigateUp,
        KeyCode::Enter => Action::Select,
        KeyCode::Char('n') => Action::Create,
        KeyCode::Char('x') => Action::Delete,
        KeyCode::Esc => Action::ExitToNormal,
        _ if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('w') => {
            Action::ExitToNormal
        }
        _ => Action::None,
    }
}

fn handle_room(key: KeyEvent) -> Action {
    if let Some(action) = check_shared_alt(key) {
        return action;
    }
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Action::NavigateDown,
        KeyCode::Char('k') | KeyCode::Up => Action::NavigateUp,
        KeyCode::Enter => Action::Select,
        KeyCode::Char('n') => Action::Create,
        KeyCode::Char('x') => Action::Delete,
        KeyCode::Esc => Action::ExitToNormal,
        _ if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r') => {
            Action::ExitToNormal
        }
        _ => Action::None,
    }
}

fn handle_resize(key: KeyEvent) -> Action {
    if let Some(action) = check_shared_alt(key) {
        return action;
    }
    match key.code {
        KeyCode::Char('h') | KeyCode::Left => Action::Resize(Direction::Left),
        KeyCode::Char('j') | KeyCode::Down => Action::Resize(Direction::Down),
        KeyCode::Char('k') | KeyCode::Up => Action::Resize(Direction::Up),
        KeyCode::Char('l') | KeyCode::Right => Action::Resize(Direction::Right),
        KeyCode::Char('H') => Action::ResizeReverse(Direction::Left),
        KeyCode::Char('J') => Action::ResizeReverse(Direction::Down),
        KeyCode::Char('K') => Action::ResizeReverse(Direction::Up),
        KeyCode::Char('L') => Action::ResizeReverse(Direction::Right),
        KeyCode::Esc => Action::ExitToNormal,
        _ if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('n') => {
            Action::ExitToNormal
        }
        _ => Action::None,
    }
}

fn check_shared_alt(key: KeyEvent) -> Option<Action> {
    if !key.modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    match key.code {
        KeyCode::Char('h') | KeyCode::Left => Some(Action::MoveFocus(Direction::Left)),
        KeyCode::Char('l') | KeyCode::Right => Some(Action::MoveFocus(Direction::Right)),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::NavigateDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::NavigateUp),
        _ => None,
    }
}
