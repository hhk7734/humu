use crate::tui::theme::{Palette, UiConfig};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;

pub struct TabBar<'a> {
    tab_names: &'a [&'a str],
    active: usize,
    active_indicators: &'a [bool],
    palette: &'a Palette,
    ui_config: &'a UiConfig,
    spinner_frame: &'a str,
}

impl<'a> TabBar<'a> {
    pub fn new(
        tab_names: &'a [&'a str],
        active: usize,
        active_indicators: &'a [bool],
        palette: &'a Palette,
        ui_config: &'a UiConfig,
    ) -> Self {
        Self {
            tab_names,
            active,
            active_indicators,
            palette,
            ui_config,
            spinner_frame: "⠋",
        }
    }

    pub fn spinner(mut self, frame: &'a str) -> Self {
        self.spinner_frame = frame;
        self
    }
}

impl Widget for TabBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Fill background with bg_secondary
        for x in area.x..area.x + area.width {
            buf[(x, area.y)]
                .set_char(' ')
                .set_style(Style::default().bg(self.palette.bg_secondary));
        }

        let sep = self.ui_config.tab_chars().separator;
        let mut x = area.x;

        for (i, name) in self.tab_names.iter().enumerate() {
            let is_active = i == self.active;
            let spinner = if self.active_indicators.get(i).copied().unwrap_or(false) {
                format!(" {}", self.spinner_frame)
            } else {
                String::new()
            };
            let label = format!(" {}{} ", name, spinner);
            let label_width = label.chars().count() as u16;

            let (fg, bg) = if is_active {
                (self.palette.bg_primary, self.palette.accent_blue)
            } else {
                (self.palette.fg_secondary, self.palette.bg_tertiary)
            };

            // Entry arrow for second+ tabs
            if i > 0 && x < area.x + area.width {
                buf[(x, area.y)]
                    .set_symbol(sep)
                    .set_style(Style::default().fg(self.palette.bg_secondary).bg(bg));
                x += 1;
            }

            // Draw tab body
            for (j, ch) in label.chars().enumerate() {
                if x + j as u16 >= area.x + area.width {
                    break;
                }
                let mut style = Style::default().fg(fg).bg(bg);
                if is_active {
                    style = style.add_modifier(Modifier::BOLD);
                }
                buf[(x + j as u16, area.y)].set_char(ch).set_style(style);
            }
            x += label_width;

            // Draw Powerline separator (exit arrow)
            if x < area.x + area.width {
                let next_bg = self.palette.bg_secondary;
                buf[(x, area.y)]
                    .set_symbol(sep)
                    .set_style(Style::default().fg(bg).bg(next_bg));
                x += 1;
            }
        }

        // Draw "+" button
        if x + 2 < area.x + area.width {
            buf[(x + 1, area.y)].set_char('+').set_style(
                Style::default()
                    .fg(self.palette.fg_muted)
                    .bg(self.palette.bg_secondary),
            );
        }
    }
}
