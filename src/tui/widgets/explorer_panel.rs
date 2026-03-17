use crate::explorer::{ExplorerState, FileKind, GitStatus, icons};
use crate::tui::theme::{Palette, UiConfig};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, BorderType, Borders, Widget};

pub struct ExplorerPanel<'a> {
    state: &'a ExplorerState,
    has_focus: bool,
    palette: &'a Palette,
    ui_config: &'a UiConfig,
}

impl<'a> ExplorerPanel<'a> {
    pub fn new(state: &'a ExplorerState, palette: &'a Palette, ui_config: &'a UiConfig) -> Self {
        Self {
            state,
            has_focus: false,
            palette,
            ui_config,
        }
    }

    pub fn focus(mut self, focused: bool) -> Self {
        self.has_focus = focused;
        self
    }
}

impl Widget for ExplorerPanel<'_> {
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

        let title = if self.state.show_ignored {
            " Explorer [+ignored] "
        } else {
            " Explorer "
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .border_type(border_type)
            .title(title)
            .title_style(Style::default().fg(self.palette.fg_secondary));
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let viewport_height = inner.height as usize;

        for (vi, entry_idx) in (self.state.scroll_offset..)
            .take(viewport_height)
            .enumerate()
        {
            let Some(entry) = self.state.entries.get(entry_idx) else {
                break;
            };

            let y = inner.y + vi as u16;
            let is_selected = entry_idx == self.state.selected;
            let line_bg = if is_selected { self.palette.bg_tertiary } else { self.palette.bg_primary };

            // Fill background for selected line
            if is_selected {
                let bg_style = Style::default().bg(line_bg);
                for x in inner.x..inner.x + inner.width {
                    buf[(x, y)].set_style(bg_style);
                }
            }

            let mut x = inner.x;
            let x_end = inner.x + inner.width;

            // Selector
            let selector = if is_selected { "\u{25b8} " } else { "  " };
            let sel_style = Style::default().fg(self.palette.accent_blue).bg(line_bg);
            for ch in selector.chars() {
                if x >= x_end { break; }
                buf[(x, y)].set_char(ch).set_style(sel_style);
                x += 1;
            }

            // Indent
            let indent_width = entry.depth * 2;
            for _ in 0..indent_width {
                if x >= x_end { break; }
                buf[(x, y)].set_char(' ').set_style(Style::default().bg(line_bg));
                x += 1;
            }

            // Icon with color
            let (icon, icon_color) = match entry.kind {
                FileKind::Directory => icons::dir_icon(entry.expanded),
                FileKind::File => icons::file_icon(&entry.name),
            };
            let icon_style = Style::default().fg(icon_color).bg(line_bg);
            for ch in icon.chars() {
                if x >= x_end { break; }
                buf[(x, y)].set_char(ch).set_style(icon_style);
                x += 1;
            }

            // Space after icon
            if x < x_end {
                buf[(x, y)].set_char(' ').set_style(Style::default().bg(line_bg));
                x += 1;
            }

            // Filename — color based on git status
            let name_color = match entry.git_status {
                Some(GitStatus::Modified) => self.palette.accent_orange,
                Some(GitStatus::Added) => self.palette.accent_green,
                None => self.palette.fg_primary,
            };
            let name_style = Style::default().fg(name_color).bg(line_bg);
            for ch in entry.name.chars() {
                if x >= x_end { break; }
                buf[(x, y)].set_char(ch).set_style(name_style);
                x += unicode_width(ch);
            }

            // Git status indicator
            if let Some(status) = entry.git_status {
                let (indicator, color) = match status {
                    GitStatus::Modified => (" \u{2717}", self.palette.accent_orange),
                    GitStatus::Added => (" \u{2605}", self.palette.accent_green),
                };
                let git_style = Style::default().fg(color).bg(line_bg);
                for ch in indicator.chars() {
                    if x >= x_end { break; }
                    buf[(x, y)].set_char(ch).set_style(git_style);
                    x += 1;
                }
            }
        }
    }
}

/// Returns the display width of a character.
fn unicode_width(ch: char) -> u16 {
    // ASCII characters are width 1; most CJK / emoji are width 2.
    // For the icons used here (Nerd Font private-use area), assume width 1
    // unless they are wide characters (U+1100..U+115F, U+2E80..U+A4CF, etc).
    if ch.is_ascii() {
        return 1;
    }
    // Simple heuristic: characters above U+1100 that are CJK get width 2.
    // Nerd Font icons in the private-use area (U+E000..U+F8FF, U+F0000..U+FFFFF)
    // render as width 1 in most terminal emulators with patched fonts.
    let cp = ch as u32;
    if (0x1100..=0x115F).contains(&cp)
        || (0x2E80..=0xA4CF).contains(&cp)
        || (0xAC00..=0xD7A3).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0xFE10..=0xFE19).contains(&cp)
        || (0xFE30..=0xFE6F).contains(&cp)
        || (0xFF00..=0xFF60).contains(&cp)
        || (0xFFE0..=0xFFE6).contains(&cp)
        || (0x20000..=0x2FA1F).contains(&cp)
    {
        2
    } else {
        1
    }
}
