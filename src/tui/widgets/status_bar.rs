use crate::tui::input::Mode;
use crate::tui::theme::{Palette, UiConfig};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;

pub struct StatusBar<'a> {
    mode: Mode,
    error: Option<&'a str>,
    palette: &'a Palette,
    ui_config: &'a UiConfig,
}

impl<'a> StatusBar<'a> {
    pub fn new(mode: Mode, palette: &'a Palette, ui_config: &'a UiConfig) -> Self {
        Self {
            mode,
            error: None,
            palette,
            ui_config,
        }
    }

    pub fn error(mut self, error: Option<&'a str>) -> Self {
        self.error = error;
        self
    }

    fn mode_label(&self) -> &'static str {
        match self.mode {
            Mode::Terminal => "TERMINAL",
            Mode::Locked => "LOCKED",
            Mode::Pane => "PANE",
            Mode::Workspace => "WORKSPACE",
            Mode::Room => "ROOM",
        }
    }

    fn mode_hints(&self) -> Vec<(&'static str, &'static str)> {
        match self.mode {
            Mode::Terminal => vec![
                ("g", "LOCK"),
                ("p", "PANE"),
                ("w", "WORKSPACE"),
                ("r", "ROOM"),
            ],
            Mode::Locked => vec![("Ctrl+g", "UNLOCK")],
            Mode::Pane => vec![
                ("n", "New"),
                ("d", "Split↓"),
                ("r", "Split→"),
                ("x", "Close"),
                ("←→↑↓", "Move"),
                ("S+←→↑↓", "Resize"),
                ("f", "Full"),
                ("t", "Tab"),
                ("c", "CloseTab"),
                ("Esc", "Back"),
            ],
            Mode::Workspace => vec![
                ("↑↓", "Navigate"),
                ("Enter", "Select"),
                ("n", "Create"),
                ("x", "Delete"),
                ("S+←→", "Resize"),
                ("Esc", "Back"),
            ],
            Mode::Room => vec![
                ("↑↓", "Navigate"),
                ("Enter", "Select"),
                ("n", "Create"),
                ("x", "Delete"),
                ("S+←→", "Resize"),
                ("Esc", "Back"),
            ],
        }
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Fill background with bg_secondary
        for x in area.x..area.x + area.width {
            buf[(x, area.y)]
                .set_char(' ')
                .set_style(Style::default().bg(self.palette.bg_secondary));
        }

        // If error, show error message and return
        if let Some(err) = self.error {
            let err_style = Style::default()
                .fg(self.palette.accent_red)
                .bg(self.palette.bg_secondary)
                .add_modifier(Modifier::BOLD);
            let msg = format!(" ERROR: {} ", err);
            for (i, ch) in msg.chars().enumerate() {
                if area.x + i as u16 >= area.x + area.width {
                    break;
                }
                buf[(area.x + i as u16, area.y)].set_char(ch).set_style(err_style);
            }
            return;
        }

        let sep = self.ui_config.tab_chars().separator;
        let mode_color = self.palette.mode_color(&self.mode);
        let mut x = area.x;

        // Mode badge: [MODE_NAME] + separator
        let mode_label = format!(" {} ", self.mode_label());
        let mode_width = mode_label.chars().count() as u16;
        for (i, ch) in mode_label.chars().enumerate() {
            if x + i as u16 >= area.x + area.width {
                break;
            }
            buf[(x + i as u16, area.y)].set_char(ch).set_style(
                Style::default()
                    .fg(self.palette.bg_primary)
                    .bg(mode_color)
                    .add_modifier(Modifier::BOLD),
            );
        }
        x += mode_width;

        // Separator: mode_color -> next_bg
        let next_bg = if self.mode == Mode::Locked {
            self.palette.bg_secondary
        } else {
            self.palette.bg_tertiary
        };
        if x < area.x + area.width {
            buf[(x, area.y)]
                .set_symbol(sep)
                .set_style(Style::default().fg(mode_color).bg(next_bg));
            x += 1;
        }

        // LOCKED mode: just show message
        if self.mode == Mode::Locked {
            let msg = " ── INTERFACE LOCKED ── ";
            for (i, ch) in msg.chars().enumerate() {
                if x + i as u16 >= area.x + area.width {
                    break;
                }
                buf[(x + i as u16, area.y)].set_char(ch).set_style(
                    Style::default()
                        .fg(self.palette.fg_muted)
                        .bg(self.palette.bg_secondary),
                );
            }
            return;
        }

        // "Ctrl +" segment only in Terminal mode (hints are Ctrl+ shortcuts).
        if self.mode == Mode::Terminal {
            let ctrl_label = " Ctrl + ";
            let ctrl_width = ctrl_label.len() as u16;
            for (i, ch) in ctrl_label.chars().enumerate() {
                if x + i as u16 >= area.x + area.width {
                    break;
                }
                buf[(x + i as u16, area.y)].set_char(ch).set_style(
                    Style::default()
                        .fg(self.palette.accent_orange)
                        .bg(self.palette.bg_tertiary)
                        .add_modifier(Modifier::BOLD),
                );
            }
            x += ctrl_width;

            if x < area.x + area.width {
                buf[(x, area.y)].set_symbol(sep).set_style(
                    Style::default()
                        .fg(self.palette.bg_tertiary)
                        .bg(self.palette.bg_secondary),
                );
                x += 1;
            }
        }

        // Key hints
        let hints = self.mode_hints();
        for (key, label) in hints {
            let hint_width = (key.len() + 1 + label.len() + 2) as u16;
            if x + hint_width > area.x + area.width {
                break;
            }
            // Space before
            x += 1;
            // Key character
            for ch in key.chars() {
                buf[(x, area.y)].set_char(ch).set_style(
                    Style::default()
                        .fg(self.palette.fg_muted)
                        .bg(self.palette.bg_secondary),
                );
                x += 1;
            }
            // Space
            buf[(x, area.y)]
                .set_char(' ')
                .set_style(Style::default().bg(self.palette.bg_secondary));
            x += 1;
            // Label
            for ch in label.chars() {
                if x >= area.x + area.width {
                    break;
                }
                buf[(x, area.y)].set_char(ch).set_style(
                    Style::default()
                        .fg(self.palette.fg_secondary)
                        .bg(self.palette.bg_secondary),
                );
                x += 1;
            }
        }
    }
}
