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
    /// (insertions, deletions) from `git diff --shortstat`
    pub diff_stat: Option<(usize, usize)>,
    /// (ahead, behind) commits relative to upstream
    pub ahead_behind: Option<(usize, usize)>,
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

        let mut y = inner.y;
        for (i, room) in self.rooms.iter().enumerate() {
            if y >= inner.y + inner.height {
                break;
            }
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
            y += 1;

            // Render git stats below room name
            let (ahead, behind) = room.ahead_behind.unwrap_or((0, 0));
            let (ins, del) = room.diff_stat.unwrap_or((0, 0));
            let has_stats = ahead > 0 || behind > 0 || ins > 0 || del > 0;

            if y < inner.y + inner.height && has_stats {
                let mut x = inner.x + 2; // align with room name

                // Git branch icon
                let git_icon = "\u{e725} ";
                buf.set_string(x, y, git_icon, Style::default().fg(self.palette.fg_muted));
                x += 2;

                let mut need_space = false;

                if ahead > 0 {
                    let text = format!("\u{2191}{}", ahead); // ↑N
                    buf.set_string(x, y, &text, Style::default().fg(self.palette.accent_cyan));
                    x += text.chars().count() as u16;
                    need_space = true;
                }

                if behind > 0 {
                    if need_space { buf.set_string(x, y, " ", Style::default()); x += 1; }
                    let text = format!("\u{2193}{}", behind); // ↓N
                    buf.set_string(x, y, &text, Style::default().fg(self.palette.accent_orange));
                    x += text.chars().count() as u16;
                    need_space = true;
                }

                if ins > 0 {
                    if need_space { buf.set_string(x, y, " ", Style::default()); x += 1; }
                    let text = format!("+{}", ins);
                    buf.set_string(x, y, &text, Style::default().fg(self.palette.accent_green));
                    x += text.len() as u16;
                    need_space = true;
                }

                if del > 0 {
                    if need_space { buf.set_string(x, y, " ", Style::default()); x += 1; }
                    let text = format!("-{}", del);
                    buf.set_string(x, y, &text, Style::default().fg(self.palette.accent_red));
                }

                y += 1;
            }
        }
    }
}
