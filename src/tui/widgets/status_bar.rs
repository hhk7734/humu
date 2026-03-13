use crate::tui::input::Mode;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

pub struct StatusBar {
    mode: Mode,
}

impl StatusBar {
    pub fn new(mode: Mode) -> Self {
        Self { mode }
    }
}

impl Widget for StatusBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let bg = Style::default().bg(Color::DarkGray).fg(Color::White);
        for x in area.x..area.right() {
            buf.set_string(x, area.y, " ", bg);
        }

        let hints = mode_hints(self.mode);
        let mut x = area.x + 1;

        for (i, (key, label)) in hints.iter().enumerate() {
            if i > 0 {
                let sep = " │ ";
                buf.set_string(x, area.y, sep, bg);
                x += sep.len() as u16;
            }

            let key_style = bg.add_modifier(Modifier::BOLD);
            buf.set_string(x, area.y, key, key_style);
            x += key.len() as u16;

            buf.set_string(x, area.y, " ", bg);
            x += 1;

            buf.set_string(x, area.y, label, bg);
            x += label.len() as u16;
        }
    }
}

fn mode_hints(mode: Mode) -> Vec<(&'static str, &'static str)> {
    match mode {
        Mode::Normal => vec![
            ("Ctrl+", ""),
            ("g", "LOCK"),
            ("p", "PANE"),
            ("t", "TAB"),
            ("w", "WORKSPACE"),
            ("n", "RESIZE"),
        ],
        Mode::Locked => vec![("Ctrl+g", "UNLOCK")],
        Mode::Pane => vec![
            ("n", "New"),
            ("d", "Split↓"),
            ("r", "Split→"),
            ("x", "Close"),
            ("hjkl", "Move"),
            ("f", "Fullscreen"),
            ("Esc", "Back"),
        ],
        Mode::Tab => vec![
            ("n", "New"),
            ("x", "Close"),
            ("h/l", "Prev/Next"),
            ("1-9", "GoTo"),
            ("r", "Rename"),
            ("Esc", "Back"),
        ],
        Mode::Workspace => vec![
            ("h/l", "Panel"),
            ("j/k", "Navigate"),
            ("Enter", "Select"),
            ("n", "Create"),
            ("x", "Delete"),
            ("Esc", "Back"),
        ],
        Mode::Resize => vec![
            ("hjkl", "Resize"),
            ("HJKL", "Reverse"),
            ("Esc", "Back"),
        ],
    }
}
