use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Widget};

pub struct RoomPanel<'a> {
    rooms: &'a [RoomItem],
    selected: Option<usize>,
    has_focus: bool,
}

pub struct RoomItem {
    pub name: String,
    pub is_default: bool,
    pub active: bool,
}

impl<'a> RoomPanel<'a> {
    pub fn new(rooms: &'a [RoomItem]) -> Self {
        Self {
            rooms,
            selected: None,
            has_focus: false,
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
        let border_style = if self.has_focus {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(" ROOMS ")
            .borders(Borders::ALL)
            .border_style(border_style);
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
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            buf.set_string(inner.x, y, &text, style);
        }
    }
}
