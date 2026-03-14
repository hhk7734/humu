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
}

pub struct RoomItem {
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

            let prefix = if is_selected { "▸ " } else { "  " };
            let suffix = if room.active { " ⠋" } else { "" };
            let text = format!("{prefix}{}{suffix}", room.name);

            let style = if is_selected {
                Style::default().fg(self.palette.accent_blue).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.palette.fg_primary)
            };

            buf.set_string(inner.x, y, &text, style);
        }
    }
}
