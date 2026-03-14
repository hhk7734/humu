use crate::tui::input::Mode;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

pub struct StatusBar<'a> {
    mode: Mode,
    error: Option<&'a str>,
}

impl<'a> StatusBar<'a> {
    pub fn new(mode: Mode) -> Self {
        Self { mode, error: None }
    }

    pub fn error(mut self, error: Option<&'a str>) -> Self {
        self.error = error;
        self
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let bg = Style::default().bg(Color::DarkGray).fg(Color::White);
        for x in area.x..area.right() {
            buf.set_string(x, area.y, " ", bg);
        }

        let mut x = area.x;

        // Render a prominent mode badge on the left.
        let (mode_label, mode_bg) = mode_badge(self.mode);
        let badge_style = Style::default()
            .bg(mode_bg)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD);
        let badge = format!(" {} ", mode_label);
        buf.set_string(x, area.y, &badge, badge_style);
        x += badge.len() as u16;

        // Separator after badge.
        buf.set_string(x, area.y, " ", bg);
        x += 1;

        // Show error on the right if present; otherwise show key hints.
        if let Some(err) = self.error {
            let err_style = Style::default()
                .bg(Color::DarkGray)
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD);
            let msg: String = err.chars().take((area.width as usize).saturating_sub(x as usize - area.x as usize)).collect();
            buf.set_string(x, area.y, &msg, err_style);
        } else {
            let hints = mode_hints(self.mode);

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
}

fn mode_badge(mode: Mode) -> (&'static str, Color) {
    match mode {
        Mode::Normal => ("NORMAL", Color::Blue),
        Mode::Locked => ("LOCKED", Color::Gray),
        Mode::Pane => ("PANE", Color::Green),
        Mode::Tab => ("TAB", Color::Yellow),
        Mode::Workspace => ("WORKSPACE", Color::Magenta),
        Mode::Room => ("ROOM", Color::LightMagenta),
        Mode::Resize => ("RESIZE", Color::Cyan),
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
            ("r", "ROOM"),
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
        Mode::Room => vec![
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
