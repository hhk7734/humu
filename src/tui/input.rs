use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Terminal,
    Locked,
    Pane,
    Tab,
    Workspace,
    Room,
    EnterSearch,
    Search,
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
    SearchInput(KeyEvent),
    SearchConfirm,
    SearchCancel,
    SearchNext,
    SearchPrev,
    SearchToggleCase,
    SearchToggleWrap,
    ScrollUp,
    ScrollDown,
    ScrollPageUp,
    ScrollPageDown,
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
        Mode::Tab => handle_tab(key),
        Mode::Workspace => handle_workspace(key),
        Mode::Room => handle_room(key),
        Mode::EnterSearch => handle_enter_search(key),
        Mode::Search => handle_search(key),
    }
}

fn handle_terminal(key: KeyEvent) -> Action {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('g') => Action::EnterMode(Mode::Locked),
            KeyCode::Char('p') => Action::EnterMode(Mode::Pane),
            KeyCode::Char('t') => Action::EnterMode(Mode::Tab),
            KeyCode::Char('w') => Action::EnterMode(Mode::Workspace),
            KeyCode::Char('r') => Action::EnterMode(Mode::Room),
            KeyCode::Char('f') => Action::EnterMode(Mode::EnterSearch),
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
        KeyCode::Esc => Action::EnterMode(Mode::Terminal),
        _ => Action::None,
    }
}

fn handle_tab(key: KeyEvent) -> Action {
    if let Some(action) = check_mode_switch(Mode::Tab, key) {
        return action;
    }
    if let Some(action) = check_shared_alt(key) {
        return action;
    }
    match key.code {
        KeyCode::Char('n') => Action::NewTab,
        KeyCode::Char('x') => Action::CloseTab,
        KeyCode::Left => Action::PrevTab,
        KeyCode::Right => Action::NextTab,
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
        KeyCode::Char('d') => Action::Delete,
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
        KeyCode::Char('d') => Action::Delete,
        _ => Action::None,
    }
}

fn handle_enter_search(key: KeyEvent) -> Action {
    if let Some(action) = check_mode_switch(Mode::EnterSearch, key) {
        return action;
    }
    match key.code {
        KeyCode::Enter => Action::SearchConfirm,
        KeyCode::Esc => Action::SearchCancel,
        _ => Action::SearchInput(key),
    }
}

fn handle_search(key: KeyEvent) -> Action {
    if let Some(action) = check_mode_switch(Mode::Search, key) {
        return action;
    }
    match key.code {
        KeyCode::Char('n') => Action::SearchNext,
        KeyCode::Char('N') => Action::SearchPrev,
        KeyCode::Char('c') => Action::SearchToggleCase,
        KeyCode::Char('w') => Action::SearchToggleWrap,
        KeyCode::Up => Action::ScrollUp,
        KeyCode::Down => Action::ScrollDown,
        KeyCode::PageUp => Action::ScrollPageUp,
        KeyCode::PageDown => Action::ScrollPageDown,
        KeyCode::Esc => Action::SearchCancel,
        _ => Action::None,
    }
}

/// Universal mode switching via Ctrl+key.
/// Ctrl+w always enters Workspace, Ctrl+r always enters Room,
/// Ctrl+t always enters Terminal, Ctrl+p toggles Pane/Terminal.
fn check_mode_switch(current: Mode, key: KeyEvent) -> Option<Action> {
    if !key.modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }
    match key.code {
        KeyCode::Char('w') => Some(Action::EnterMode(Mode::Workspace)),
        KeyCode::Char('r') => Some(Action::EnterMode(Mode::Room)),
        KeyCode::Char('t') => Some(Action::EnterMode(Mode::Terminal)),
        KeyCode::Char('p') => Some(if current == Mode::Pane {
            Action::EnterMode(Mode::Terminal)
        } else {
            Action::EnterMode(Mode::Pane)
        }),
        _ => None,
    }
}

/// Map a clicked hint index to the corresponding action for the given mode.
pub fn hint_click_action(mode: Mode, hint_index: usize) -> Option<Action> {
    match mode {
        Mode::Terminal => match hint_index {
            0 => Some(Action::EnterMode(Mode::Locked)),      // g LOCK
            1 => Some(Action::EnterMode(Mode::EnterSearch)), // f FIND
            2 => Some(Action::EnterMode(Mode::Pane)),        // p PANE
            3 => Some(Action::EnterMode(Mode::Tab)),         // t TAB
            4 => Some(Action::EnterMode(Mode::Workspace)),   // w WORKSPACE
            5 => Some(Action::EnterMode(Mode::Room)),        // r ROOM
            _ => None,
        },
        Mode::Pane => match hint_index {
            0 => Some(Action::NewPane),                       // n New
            1 => Some(Action::SplitDown),                     // d Split↓
            2 => Some(Action::SplitRight),                    // r Split→
            3 => Some(Action::ClosePane),                     // x Close
            4 => None,                                        // ←→↑↓ Move
            5 => None,                                        // S+←→↑↓ Resize
            6 => Some(Action::ToggleFullscreen),              // f Full
            7 => Some(Action::EnterMode(Mode::Terminal)),     // Esc Back
            _ => None,
        },
        Mode::Tab => match hint_index {
            0 => Some(Action::NewTab),                        // n New
            1 => Some(Action::CloseTab),                      // x Close
            2 => None,                                        // ←→ Prev/Next
            3 => None,                                        // 1-9 GoTo
            4 => Some(Action::EnterMode(Mode::Terminal)),     // Esc Back
            _ => None,
        },
        Mode::Workspace => match hint_index {
            0 => None,                                        // ↑↓ Navigate
            1 => Some(Action::Select),                        // Enter Select
            2 => Some(Action::Create),                        // n Create
            3 => Some(Action::Delete),                        // d Delete
            4 => None,                                        // S+←→ Resize
            _ => None,
        },
        Mode::Room => match hint_index {
            0 => None,                                        // ↑↓ Navigate
            1 => Some(Action::Select),                        // Enter Select
            2 => Some(Action::Create),                        // n Create
            3 => Some(Action::Delete),                        // d Delete
            4 => None,                                        // S+←→ Resize
            _ => None,
        },
        Mode::Search => match hint_index {
            0 => Some(Action::SearchNext),                    // n NEXT
            1 => Some(Action::SearchPrev),                    // N PREV
            2 => Some(Action::SearchToggleCase),              // c CASE
            3 => Some(Action::SearchToggleWrap),              // w WRAP
            _ => None,
        },
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
