use crate::tui::input::Mode;
use crate::tui::theme::{Palette, UiConfig};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
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
            Mode::Tab => "TAB",
            Mode::Workspace => "WORKSPACE",
            Mode::Room => "ROOM",
            Mode::EnterSearch | Mode::Search => "SEARCH",
        }
    }

    fn mode_hints(&self) -> Vec<(&'static str, &'static str)> {
        match self.mode {
            Mode::Terminal => vec![
                ("g", "LOCK"),
                ("p", "PANE"),
                ("t", "TAB"),
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
                ("Esc", "Back"),
            ],
            Mode::Tab => vec![
                ("n", "New"),
                ("x", "Close"),
                ("←→", "Prev/Next"),
                ("1-9", "GoTo"),
                ("Esc", "Back"),
            ],
            Mode::Workspace => vec![
                ("↑↓", "Navigate"),
                ("Enter", "Select"),
                ("n", "Create"),
                ("x", "Delete"),
                ("S+←→", "Resize"),
            ],
            Mode::Room => vec![
                ("↑↓", "Navigate"),
                ("Enter", "Select"),
                ("n", "Create"),
                ("x", "Delete"),
                ("S+←→", "Resize"),
            ],
            Mode::EnterSearch | Mode::Search => vec![],
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

        // Separator: mode_color -> bg_secondary
        if x < area.x + area.width {
            buf[(x, area.y)]
                .set_symbol(sep)
                .set_style(Style::default().fg(mode_color).bg(self.palette.bg_secondary));
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

        // "Ctrl +" segment only in Terminal mode — Powerline style like hints.
        if self.mode == Mode::Terminal {
            let ctrl_bg = self.palette.bg_tertiary;
            let ctrl_label = " Ctrl + ";
            let ctrl_width = ctrl_label.len() as u16;

            // Entry arrow into Ctrl+ segment
            if x < area.x + area.width {
                buf[(x, area.y)].set_symbol(sep).set_style(
                    Style::default().fg(self.palette.bg_secondary).bg(ctrl_bg),
                );
                x += 1;
            }
            // Ctrl+ text
            for (i, ch) in ctrl_label.chars().enumerate() {
                if x + i as u16 >= area.x + area.width {
                    break;
                }
                buf[(x + i as u16, area.y)].set_char(ch).set_style(
                    Style::default()
                        .fg(self.palette.accent_orange)
                        .bg(ctrl_bg)
                        .add_modifier(Modifier::BOLD),
                );
            }
            x += ctrl_width;
            // Exit arrow out of Ctrl+ segment
            if x < area.x + area.width {
                buf[(x, area.y)].set_symbol(sep).set_style(
                    Style::default().fg(ctrl_bg).bg(self.palette.bg_secondary),
                );
                x += 1;
            }
        }

        // Key hints — each hint is a Powerline segment: [ key label ]▸
        let hints = self.mode_hints();
        let hint_bg = Color::Rgb(139, 148, 158);
        let outer_bg = self.palette.bg_secondary;
        for (key, label) in &hints {
            // Width: space + key + space + label + space + separator(1)
            let seg_width = 1 + key.len() as u16 + 1 + label.len() as u16 + 1 + 1;
            if x + seg_width > area.x + area.width {
                break;
            }
            // Powerline arrow into segment (left=outer, right=inner)
            if x < area.x + area.width {
                buf[(x, area.y)].set_symbol(sep).set_style(
                    Style::default().fg(outer_bg).bg(hint_bg),
                );
                x += 1;
            }
            // Key — bold bright
            buf[(x, area.y)].set_char(' ').set_style(Style::default().bg(hint_bg));
            x += 1;
            for ch in key.chars() {
                if x >= area.x + area.width { break; }
                buf[(x, area.y)].set_char(ch).set_style(
                    Style::default()
                        .fg(Color::Rgb(180, 40, 40))
                        .bg(hint_bg)
                        .add_modifier(Modifier::BOLD),
                );
                x += 1;
            }
            // Space + label — dimmer
            if x < area.x + area.width {
                buf[(x, area.y)].set_char(' ').set_style(Style::default().bg(hint_bg));
                x += 1;
            }
            for ch in label.chars() {
                if x >= area.x + area.width { break; }
                buf[(x, area.y)].set_char(ch).set_style(
                    Style::default().fg(Color::Rgb(13, 17, 23)).bg(hint_bg),
                );
                x += 1;
            }
            // Trailing space
            if x < area.x + area.width {
                buf[(x, area.y)].set_char(' ').set_style(Style::default().bg(hint_bg));
                x += 1;
            }
            // Powerline arrow out of segment
            if x < area.x + area.width {
                buf[(x, area.y)].set_symbol(sep).set_style(
                    Style::default().fg(hint_bg).bg(outer_bg),
                );
                x += 1;
            }
        }
    }
}
