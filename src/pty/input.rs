use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::pty::terminal::{MouseProtocolEncoding, MouseProtocolMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneInputState {
    pub mouse_mode: MouseProtocolMode,
    pub mouse_encoding: MouseProtocolEncoding,
    pub alternate_screen: bool,
    pub bracketed_paste: bool,
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    Write(Vec<u8>),
    AdjustScrollback { lines: usize, up: bool },
    ResetScrollback,
    StartSelection { row: u16, col: u16 },
    UpdateSelection { row: u16, col: u16 },
    FinishSelection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputRoute {
    Handled(Vec<InputAction>),
    NotHandled,
}

pub fn route_mouse(
    mouse: MouseEvent,
    pane_rect: Rect,
    state: &PaneInputState,
    _pty_mouse_active: bool,
    selection_active: bool,
) -> InputRoute {
    match mouse.kind {
        MouseEventKind::ScrollUp => route_wheel(mouse, pane_rect, state, true),
        MouseEventKind::ScrollDown => route_wheel(mouse, pane_rect, state, false),
        MouseEventKind::Down(MouseButton::Left)
            if state.mouse_mode == MouseProtocolMode::None =>
        {
            handled(vec![InputAction::StartSelection {
                row: mouse.row.saturating_sub(pane_rect.y + 1),
                col: mouse.column.saturating_sub(pane_rect.x + 1),
            }])
        }
        MouseEventKind::Drag(MouseButton::Left)
            if state.mouse_mode == MouseProtocolMode::None && selection_active =>
        {
            handled(vec![InputAction::UpdateSelection {
                row: mouse.row.saturating_sub(pane_rect.y + 1),
                col: mouse.column.saturating_sub(pane_rect.x + 1),
            }])
        }
        MouseEventKind::Up(MouseButton::Left)
            if state.mouse_mode == MouseProtocolMode::None && selection_active =>
        {
            handled(vec![InputAction::FinishSelection])
        }
        _ if state.mouse_mode != MouseProtocolMode::None => {
            build_mouse_sequence(mouse, pane_rect, state.mouse_encoding)
                .map(|seq| handled(vec![InputAction::Write(seq)]))
                .unwrap_or(InputRoute::NotHandled)
        }
        _ => InputRoute::NotHandled,
    }
}

pub fn route_floating_mouse(
    mouse: MouseEvent,
    popup_area: Rect,
    state: &PaneInputState,
) -> InputRoute {
    if state.mouse_mode == MouseProtocolMode::None {
        return match mouse.kind {
            MouseEventKind::ScrollUp => handled(vec![InputAction::Write(b"kkk".to_vec())]),
            MouseEventKind::ScrollDown => handled(vec![InputAction::Write(b"jjj".to_vec())]),
            _ => handled(vec![]),
        };
    }

    build_mouse_sequence(mouse, popup_area, state.mouse_encoding)
        .map(|seq| handled(vec![InputAction::Write(seq)]))
        .unwrap_or(InputRoute::NotHandled)
}

pub fn route_passthrough(key: KeyEvent, state: &PaneInputState) -> InputRoute {
    if matches!(key.code, KeyCode::PageUp | KeyCode::PageDown)
        && (state.mouse_mode == MouseProtocolMode::None || state.alternate_screen)
    {
        return handled(vec![InputAction::AdjustScrollback {
            lines: state.rows as usize,
            up: key.code == KeyCode::PageUp,
        }]);
    }

    let mut actions = vec![InputAction::ResetScrollback];
    let bytes = key_event_to_bytes(&key);
    if !bytes.is_empty() {
        actions.push(InputAction::Write(bytes));
    }
    handled(actions)
}

pub fn route_paste(text: &str, state: &PaneInputState) -> InputRoute {
    let mut actions = vec![InputAction::ResetScrollback];
    if state.bracketed_paste {
        let mut buf = Vec::with_capacity(12 + text.len());
        buf.extend_from_slice(b"\x1b[200~");
        buf.extend_from_slice(text.as_bytes());
        buf.extend_from_slice(b"\x1b[201~");
        actions.push(InputAction::Write(buf));
    } else {
        actions.push(InputAction::Write(text.as_bytes().to_vec()));
    }
    handled(actions)
}

fn route_wheel(
    mouse: MouseEvent,
    pane_rect: Rect,
    state: &PaneInputState,
    up: bool,
) -> InputRoute {
    if state.mouse_mode != MouseProtocolMode::None && !state.alternate_screen {
        if let Some(seq) = build_mouse_sequence(mouse, pane_rect, state.mouse_encoding) {
            return handled(vec![InputAction::Write(seq)]);
        }
    }

    handled(vec![InputAction::AdjustScrollback {
        lines: 3,
        up,
    }])
}

fn build_mouse_sequence(
    mouse: MouseEvent,
    pane_rect: Rect,
    encoding: MouseProtocolEncoding,
) -> Option<Vec<u8>> {
    let col = mouse.column.saturating_sub(pane_rect.x + 1) as u32;
    let row = mouse.row.saturating_sub(pane_rect.y + 1) as u32;

    let (button, press) = match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => (0u32, true),
        MouseEventKind::Down(MouseButton::Right) => (2, true),
        MouseEventKind::Down(MouseButton::Middle) => (1, true),
        MouseEventKind::Up(MouseButton::Left) => (0, false),
        MouseEventKind::Up(MouseButton::Right) => (2, false),
        MouseEventKind::Up(MouseButton::Middle) => (1, false),
        MouseEventKind::Drag(MouseButton::Left) => (32, true),
        MouseEventKind::Drag(MouseButton::Right) => (34, true),
        MouseEventKind::Drag(MouseButton::Middle) => (33, true),
        MouseEventKind::ScrollUp => (64, true),
        MouseEventKind::ScrollDown => (65, true),
        MouseEventKind::Moved => (35, true),
        _ => return None,
    };

    Some(match encoding {
        MouseProtocolEncoding::Sgr => {
            let suffix = if press { 'M' } else { 'm' };
            format!("\x1b[<{};{};{}{}", button, col + 1, row + 1, suffix).into_bytes()
        }
        _ => {
            let b = (button + 32) as u8;
            let c = ((col + 33).min(255)) as u8;
            let r = ((row + 33).min(255)) as u8;
            format!("\x1b[M{}{}{}", b as char, c as char, r as char).into_bytes()
        }
    })
}

fn handled(actions: Vec<InputAction>) -> InputRoute {
    InputRoute::Handled(actions)
}

fn csi_u_modifier(modifiers: KeyModifiers) -> u8 {
    1 + if modifiers.contains(KeyModifiers::SHIFT) {
        1
    } else {
        0
    } + if modifiers.contains(KeyModifiers::ALT) {
        2
    } else {
        0
    } + if modifiers.contains(KeyModifiers::CONTROL) {
        4
    } else {
        0
    }
}

fn key_event_to_bytes(key: &KeyEvent) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let has_modifier = key.modifiers != KeyModifiers::NONE;
    match key.code {
        KeyCode::Char(c) if ctrl => {
            let base = vec![(c as u8) & 0x1f];
            if alt {
                [b"\x1b".as_slice(), &base].concat()
            } else {
                base
            }
        }
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            let base = s.as_bytes().to_vec();
            if alt {
                [b"\x1b".as_slice(), &base].concat()
            } else {
                base
            }
        }
        KeyCode::Enter if has_modifier => {
            format!("\x1b[13;{}u", csi_u_modifier(key.modifiers)).into_bytes()
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab if has_modifier => {
            format!("\x1b[9;{}u", csi_u_modifier(key.modifiers)).into_bytes()
        }
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::F(n) => match n {
            1 => b"\x1bOP".to_vec(),
            2 => b"\x1bOQ".to_vec(),
            3 => b"\x1bOR".to_vec(),
            4 => b"\x1bOS".to_vec(),
            5..=12 => format!("\x1b[{n}~").into_bytes(),
            _ => vec![],
        },
        _ => vec![],
    }
}
