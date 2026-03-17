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
        let max_width = inner.width as usize;

        for (vi, entry_idx) in (self.state.scroll_offset..)
            .take(viewport_height)
            .enumerate()
        {
            let Some(entry) = self.state.entries.get(entry_idx) else {
                break;
            };

            let y = inner.y + vi as u16;
            let is_selected = entry_idx == self.state.selected;

            // Build the line: selector + indent + icon + space + name + git indicator
            let selector = if is_selected { "\u{25b8} " } else { "  " };
            let indent = " ".repeat(entry.depth * 2);
            let icon = match entry.kind {
                FileKind::Directory => icons::dir_icon(entry.expanded),
                FileKind::File => icons::file_icon(&entry.name),
            };
            let git_suffix = match entry.git_status {
                Some(GitStatus::Modified) => " \u{2717}",
                Some(GitStatus::Added) => " \u{2605}",
                None => "",
            };

            let text = format!("{selector}{indent}{icon} {}{git_suffix}", entry.name);

            // Truncate to fit within max_width
            let display: String = text.chars().take(max_width).collect();

            // Background for selected line
            if is_selected {
                let bg_style = Style::default().bg(self.palette.bg_tertiary);
                for x in inner.x..inner.x + inner.width {
                    buf[(x, y)].set_style(bg_style);
                }
            }

            // Render the text with default foreground
            let base_style = Style::default().fg(self.palette.fg_primary);
            buf.set_string(inner.x, y, &display, base_style);

            // Overlay git status indicator color
            if entry.git_status.is_some() {
                let git_color = match entry.git_status {
                    Some(GitStatus::Modified) => self.palette.accent_orange,
                    Some(GitStatus::Added) => self.palette.accent_green,
                    None => unreachable!(),
                };
                // The git suffix is the last 2 characters (" " + indicator)
                // Find where the git suffix starts in the rendered text
                let text_char_count = display.chars().count();
                if text_char_count >= 2 {
                    let suffix_start = text_char_count - 2;
                    // Calculate the byte offset for the suffix start position
                    let mut col = 0u16;
                    for (ci, ch) in display.chars().enumerate() {
                        if ci == suffix_start {
                            let git_style = Style::default().fg(git_color);
                            let suffix_str: String = display.chars().skip(suffix_start).collect();
                            buf.set_string(inner.x + col, y, &suffix_str, git_style);
                            break;
                        }
                        col += unicode_width(ch);
                    }
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
