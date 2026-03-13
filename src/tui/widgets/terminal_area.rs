use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

pub struct TabBar<'a> {
    tab_names: &'a [&'a str],
    active: usize,
    active_indicators: &'a [bool],
}

impl<'a> TabBar<'a> {
    pub fn new(tab_names: &'a [&'a str], active: usize, active_indicators: &'a [bool]) -> Self {
        Self {
            tab_names,
            active,
            active_indicators,
        }
    }
}

impl Widget for TabBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let bg = Style::default().bg(Color::Black);
        for x in area.x..area.right() {
            buf.set_string(x, area.y, " ", bg);
        }

        let mut x = area.x;
        for (i, name) in self.tab_names.iter().enumerate() {
            let is_active = i == self.active;
            let spinner = if self.active_indicators.get(i).copied().unwrap_or(false) {
                " ⠋"
            } else {
                ""
            };
            let text = format!(" {name}{spinner} ");

            let style = if is_active {
                Style::default()
                    .fg(Color::Cyan)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray).bg(Color::Black)
            };

            buf.set_string(x, area.y, &text, style);
            x += text.len() as u16;
        }

        // "+" button
        let plus_style = Style::default().fg(Color::DarkGray).bg(Color::Black);
        buf.set_string(x, area.y, " + ", plus_style);
    }
}
