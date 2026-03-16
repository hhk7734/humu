use crate::tui::theme::{Palette, UiConfig};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders, Widget};

pub struct RoomPanel<'a> {
    rooms: &'a [RoomItem],
    selected: Option<usize>,
    has_focus: bool,
    palette: &'a Palette,
    ui_config: &'a UiConfig,
    spinner_frame: &'a str,
}

pub struct RoomItem {
    pub id: Option<crate::id::RoomId>,
    pub name: String,
    pub is_default: bool,
    pub active: bool,
}

impl<'a> RoomPanel<'a> {
    pub fn new(rooms: &'a [RoomItem], palette: &'a Palette, ui_config: &'a UiConfig) -> Self {
        Self {
            rooms,
            selected: None,
            has_focus: false,
            palette,
            ui_config,
            spinner_frame: "⠋",
        }
    }

    pub fn selected(mut self, index: Option<usize>) -> Self {
        self.selected = index;
        self
    }

    pub fn focus(mut self, focused: bool) -> Self {
        self.has_focus = focused;
        self
    }

    pub fn spinner(mut self, frame: &'a str) -> Self {
        self.spinner_frame = frame;
        self
    }
}

impl Widget for RoomPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_color = if self.has_focus {
            self.palette.accent_blue
        } else {
            self.palette.fg_muted
        };
        let border_type = if self.ui_config.rounded_corners {
            BorderType::Rounded
        } else {
            BorderType::Plain
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .border_type(border_type)
            .title(" Rooms ")
            .title_style(Style::default().fg(self.palette.fg_secondary));
        let inner = block.inner(area);
        block.render(area, buf);

        for (i, room) in self.rooms.iter().enumerate() {
            if i as u16 >= inner.height {
                break;
            }
            let y = inner.y + i as u16;
            let is_selected = self.selected == Some(i);
            let max_width = inner.width as usize;

            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            let suffix = if room.active {
                format!(" {}", self.spinner_frame)
            } else {
                String::new()
            };
            let prefix_len = 2;
            let suffix_len = if room.active { 2 } else { 0 };
            let name_budget = max_width.saturating_sub(prefix_len + suffix_len);

            let display_name = if room.name.len() > name_budget && name_budget >= 3 {
                format!("{}...", &room.name[..name_budget - 3])
            } else {
                room.name.clone()
            };
            let text = format!("{prefix}{display_name}{suffix}");

            let style = if is_selected {
                Style::default()
                    .fg(self.palette.accent_blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.palette.fg_primary)
            };

            buf.set_string(inner.x, y, &text, style);
        }
    }
}
