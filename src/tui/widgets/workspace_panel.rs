use crate::git::room::RoomGitStatus;
use crate::id::{RoomId, WorkspaceId};
use crate::tui::theme::{Palette, UiConfig};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders, Widget};

/// Kinds of items in the workspace tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeItemKind {
    Workspace,
    Room,
}

/// A single row in the flattened workspace tree.
#[derive(Debug, Clone)]
pub struct WorkspaceTreeItem {
    pub kind: TreeItemKind,
    pub name: String,
    pub active: bool,
    pub workspace_id: WorkspaceId,
    pub room_id: Option<RoomId>,
    pub git_status: RoomGitStatus,
}

pub struct WorkspacePanel<'a> {
    items: &'a [WorkspaceTreeItem],
    selected: Option<usize>,
    has_focus: bool,
    palette: &'a Palette,
    ui_config: &'a UiConfig,
    spinner_frame: &'a str,
    active_ws: Option<WorkspaceId>,
    active_room: Option<RoomId>,
}

impl<'a> WorkspacePanel<'a> {
    pub fn new(
        items: &'a [WorkspaceTreeItem],
        palette: &'a Palette,
        ui_config: &'a UiConfig,
    ) -> Self {
        Self {
            items,
            selected: None,
            has_focus: false,
            palette,
            ui_config,
            spinner_frame: "\u{280b}",
            active_ws: None,
            active_room: None,
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

    pub fn active(mut self, ws: Option<WorkspaceId>, room: Option<RoomId>) -> Self {
        self.active_ws = ws;
        self.active_room = room;
        self
    }
}

impl Widget for WorkspacePanel<'_> {
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
            .title(" Workspaces ")
            .title_style(Style::default().fg(self.palette.fg_secondary));
        let inner = block.inner(area);
        block.render(area, buf);

        let viewport_height = inner.height as usize;

        // Compute visual row offset for each item (workspace=1 row, room=2 rows).
        let mut item_visual_rows: Vec<(usize, usize)> = Vec::new(); // (item_index, visual_start_row)
        let mut total_rows = 0usize;
        for (i, item) in self.items.iter().enumerate() {
            item_visual_rows.push((i, total_rows));
            total_rows += match item.kind {
                TreeItemKind::Workspace => 1,
                TreeItemKind::Room => 2, // name + git stats
            };
        }

        // Find scroll offset so selected item is visible.
        let scroll_offset = if let Some(selected_idx) = self.selected {
            let sel_start = item_visual_rows.get(selected_idx).map(|r| r.1).unwrap_or(0);
            let sel_height = self.items.get(selected_idx).map(|item| match item.kind {
                TreeItemKind::Workspace => 1,
                TreeItemKind::Room => 2,
            }).unwrap_or(1);
            let sel_end = sel_start + sel_height;
            if sel_end > viewport_height {
                sel_end.saturating_sub(viewport_height)
            } else {
                0
            }
        } else {
            0
        };

        let mut visual_row = 0usize;
        let mut y = inner.y;
        for (i, item) in self.items.iter().enumerate() {
            let item_height: usize = match item.kind {
                TreeItemKind::Workspace => 1,
                TreeItemKind::Room => 2,
            };

            // Skip items above the scroll offset
            if visual_row + item_height <= scroll_offset {
                visual_row += item_height;
                continue;
            }

            if y >= inner.y + inner.height {
                break;
            }

            let is_selected = self.selected == Some(i);
            let max_width = inner.width as usize;

            // Highlight the active workspace/room with bg_tertiary
            let is_active = match &item.kind {
                TreeItemKind::Workspace => self.active_ws == Some(item.workspace_id),
                TreeItemKind::Room => {
                    self.active_ws == Some(item.workspace_id)
                        && self.active_room.is_some()
                        && self.active_room == item.room_id
                }
            };
            if is_selected {
                let bg = Style::default().bg(self.palette.bg_tertiary);
                for bx in inner.x..inner.x + inner.width {
                    buf[(bx, y)].set_style(bg);
                }
            }
            if is_active {
                let bg = Style::default().bg(self.palette.bg_selected);
                for bx in inner.x..inner.x + inner.width {
                    buf[(bx, y)].set_style(bg);
                }
            }

            match &item.kind {
                TreeItemKind::Workspace => {
                    let suffix = if item.active {
                        format!(" {}", self.spinner_frame)
                    } else {
                        String::new()
                    };
                    let prefix_len = 2; // "▸ " selector
                    let suffix_len = if item.active { 2 } else { 0 };
                    let name_budget = max_width.saturating_sub(prefix_len + suffix_len);

                    let display_name = if item.name.chars().count() > name_budget && name_budget >= 3 {
                        let truncated: String = item.name.chars().take(name_budget - 3).collect();
                        format!("{truncated}...")
                    } else {
                        item.name.clone()
                    };
                    let prefix = if is_selected { "\u{25b8} " } else { "  " };
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
                }
                TreeItemKind::Room => {
                    // Room row: "    name" (indented by 4)
                    let indent = "    ";
                    let indent_len = 4;
                    let suffix = if item.active {
                        format!(" {}", self.spinner_frame)
                    } else {
                        String::new()
                    };
                    let suffix_len = if item.active { 2 } else { 0 };
                    let name_budget = max_width.saturating_sub(indent_len + suffix_len);

                    let display_name = if item.name.chars().count() > name_budget && name_budget >= 3 {
                        let truncated: String = item.name.chars().take(name_budget - 3).collect();
                        format!("{truncated}...")
                    } else {
                        item.name.clone()
                    };
                    let text = format!("{indent}{display_name}{suffix}");

                    let style = if is_selected {
                        Style::default()
                            .fg(self.palette.accent_blue)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.palette.fg_secondary)
                    };

                    buf.set_string(inner.x, y, &text, style);
                    y += 1;

                    // Git stats line below room: " ↑N ↓N ?N +N -N"
                    let (ahead, behind) = item.git_status.ahead_behind.unwrap_or((0, 0));
                    let (ins, del) = item.git_status.diff_stat.unwrap_or((0, 0));
                    let untracked = item.git_status.untracked_count;
                    let has_changes = ahead > 0 || behind > 0 || untracked > 0 || ins > 0 || del > 0;

                    if y < inner.y + inner.height {
                        // Fill background for active room stats line
                        if is_active {
                            let bg = Style::default().bg(self.palette.bg_selected);
                            for bx in inner.x..inner.x + inner.width {
                                buf[(bx, y)].set_style(bg);
                            }
                        }
                        let mut x = inner.x + 4;

                        // Git branch icon — green when clean, orange when dirty
                        let icon_color = if has_changes {
                            self.palette.accent_orange
                        } else {
                            self.palette.accent_green
                        };
                        buf.set_string(x, y, "\u{e725} ", Style::default().fg(icon_color));
                        x += 2;

                        let mut need_space = false;

                        if ahead > 0 {
                            let text = format!("\u{2191}{}", ahead);
                            buf.set_string(
                                x,
                                y,
                                &text,
                                Style::default().fg(self.palette.accent_cyan),
                            );
                            x += text.chars().count() as u16;
                            need_space = true;
                        }

                        if behind > 0 {
                            if need_space {
                                buf.set_string(x, y, " ", Style::default());
                                x += 1;
                            }
                            let text = format!("\u{2193}{}", behind);
                            buf.set_string(
                                x,
                                y,
                                &text,
                                Style::default().fg(self.palette.accent_orange),
                            );
                            x += text.chars().count() as u16;
                            need_space = true;
                        }

                        if untracked > 0 {
                            if need_space {
                                buf.set_string(x, y, " ", Style::default());
                                x += 1;
                            }
                            let text = format!("?{}", untracked);
                            buf.set_string(
                                x,
                                y,
                                &text,
                                Style::default().fg(self.palette.accent_green),
                            );
                            x += text.chars().count() as u16;
                            need_space = true;
                        }

                        if ins > 0 {
                            if need_space {
                                buf.set_string(x, y, " ", Style::default());
                                x += 1;
                            }
                            let text = format!("+{}", ins);
                            buf.set_string(
                                x,
                                y,
                                &text,
                                Style::default().fg(self.palette.accent_green),
                            );
                            x += text.chars().count() as u16;
                            need_space = true;
                        }

                        if del > 0 {
                            if need_space {
                                buf.set_string(x, y, " ", Style::default());
                                x += 1;
                            }
                            let text = format!("-{}", del);
                            buf.set_string(
                                x,
                                y,
                                &text,
                                Style::default().fg(self.palette.accent_red),
                            );
                        }

                        y += 1;
                    }
                }
            }
            visual_row += item_height;
        }
    }
}
