use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Widget};

pub struct WorkspacePanel<'a> {
    workspaces: &'a [WorkspaceItem],
    selected: Option<usize>,
    has_focus: bool,
}

pub struct WorkspaceItem {
    pub name: String,
    pub active: bool,
}

impl<'a> WorkspacePanel<'a> {
    pub fn new(workspaces: &'a [WorkspaceItem]) -> Self {
        Self {
            workspaces,
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

impl Widget for WorkspacePanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_style = if self.has_focus {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(" WORKSPACES ")
            .borders(Borders::ALL)
            .border_style(border_style);
        let inner = block.inner(area);
        block.render(area, buf);

        for (i, ws) in self.workspaces.iter().enumerate() {
            if i as u16 >= inner.height {
                break;
            }
            let y = inner.y + i as u16;
            let is_selected = self.selected == Some(i);

            let prefix = if is_selected { "▸ " } else { "  " };
            let suffix = if ws.active { " ⠋" } else { "" };
            let text = format!("{prefix}{}{suffix}", ws.name);

            let style = if is_selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            buf.set_string(inner.x, y, &text, style);
        }
    }
}
