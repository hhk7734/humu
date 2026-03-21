use crate::pty::terminal::Screen;
use crate::tui::search::SearchMatch;
use crate::tui::theme::{Palette, UiConfig};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

pub struct TerminalWidget<'a> {
    screen: &'a Screen,
    has_focus: bool,
    exited: Option<i32>,
    pane_count: usize,
    title: &'a str,
    palette: &'a Palette,
    ui_config: &'a UiConfig,
    search_matches: &'a [SearchMatch],
    active_match_index: Option<usize>,
    scrollback_base_row: usize,
    /// Text selection range in screen coords: (start_row, start_col, end_row, end_col).
    selection: Option<(u16, u16, u16, u16)>,
}

impl<'a> TerminalWidget<'a> {
    pub fn new(
        screen: &'a Screen,
        title: &'a str,
        palette: &'a Palette,
        ui_config: &'a UiConfig,
    ) -> Self {
        Self {
            screen,
            has_focus: false,
            exited: None,
            pane_count: 1,
            title,
            palette,
            ui_config,
            search_matches: &[],
            active_match_index: None,
            scrollback_base_row: 0,
            selection: None,
        }
    }

    pub fn search(
        mut self,
        matches: &'a [SearchMatch],
        active: Option<usize>,
        base_row: usize,
    ) -> Self {
        self.search_matches = matches;
        self.active_match_index = active;
        self.scrollback_base_row = base_row;
        self
    }

    pub fn focus(mut self, focused: bool) -> Self {
        self.has_focus = focused;
        self
    }

    pub fn exited(mut self, exit_code: Option<i32>) -> Self {
        self.exited = exit_code;
        self
    }

    pub fn pane_count(mut self, count: usize) -> Self {
        self.pane_count = count;
        self
    }

    pub fn selection(mut self, sel: Option<(u16, u16, u16, u16)>) -> Self {
        self.selection = sel;
        self
    }
}

impl Widget for TerminalWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height < 4 {
            return;
        }

        let bc = self.ui_config.border_chars();
        let border_color = if self.has_focus {
            self.palette.accent_blue
        } else {
            self.palette.fg_muted
        };
        let border_style = Style::default().fg(border_color);

        // Top border: ╭─ title ─...─╮
        buf[(area.x, area.y)]
            .set_symbol(bc.top_left)
            .set_style(border_style);
        buf[(area.x + 1, area.y)]
            .set_symbol(bc.horizontal)
            .set_style(border_style);
        buf[(area.x + 2, area.y)]
            .set_char(' ')
            .set_style(border_style);
        let title_max = (area.width as usize).saturating_sub(6);
        let title_display: String = self.title.chars().take(title_max).collect();
        for (i, ch) in title_display.chars().enumerate() {
            buf[(area.x + 3 + i as u16, area.y)]
                .set_char(ch)
                .set_style(Style::default().fg(self.palette.fg_secondary));
        }
        let title_end = area.x + 3 + title_display.len() as u16;
        buf[(title_end, area.y)]
            .set_char(' ')
            .set_style(border_style);
        for x in (title_end + 1)..area.x + area.width - 1 {
            buf[(x, area.y)]
                .set_symbol(bc.horizontal)
                .set_style(border_style);
        }
        buf[(area.x + area.width - 1, area.y)]
            .set_symbol(bc.top_right)
            .set_style(border_style);

        // Side borders
        for y in (area.y + 1)..area.y + area.height - 1 {
            buf[(area.x, y)]
                .set_symbol(bc.vertical)
                .set_style(border_style);
            buf[(area.x + area.width - 1, y)]
                .set_symbol(bc.vertical)
                .set_style(border_style);
        }

        // Bottom border: ╰─ EXIT: N ─...─╯  or  ╰─────...─╯
        buf[(area.x, area.y + area.height - 1)]
            .set_symbol(bc.bottom_left)
            .set_style(border_style);
        if let Some(code) = self.exited {
            let exit_label = format!(" EXIT: {} ", code);
            let exit_color = if code == 0 {
                self.palette.accent_green
            } else {
                self.palette.accent_red
            };
            buf[(area.x + 1, area.y + area.height - 1)]
                .set_symbol(bc.horizontal)
                .set_style(border_style);
            buf[(area.x + 2, area.y + area.height - 1)]
                .set_char(' ')
                .set_style(border_style);
            for (i, ch) in exit_label.chars().enumerate() {
                let px = area.x + 3 + i as u16;
                if px >= area.x + area.width - 1 {
                    break;
                }
                buf[(px, area.y + area.height - 1)]
                    .set_char(ch)
                    .set_style(Style::default().fg(exit_color).add_modifier(Modifier::BOLD));
            }
            let exit_end = area.x + 3 + exit_label.len() as u16;
            for x in exit_end..area.x + area.width - 1 {
                buf[(x, area.y + area.height - 1)]
                    .set_symbol(bc.horizontal)
                    .set_style(border_style);
            }
        } else {
            for x in (area.x + 1)..area.x + area.width - 1 {
                buf[(x, area.y + area.height - 1)]
                    .set_symbol(bc.horizontal)
                    .set_style(border_style);
            }
        }
        buf[(area.x + area.width - 1, area.y + area.height - 1)]
            .set_symbol(bc.bottom_right)
            .set_style(border_style);

        // Scrollback indicator in bottom-right of border (e.g., "─ ↑42 ╯")
        let scrollback = self.screen.scrollback();
        if scrollback > 0 {
            let label = format!(" \u{2191}{} ", scrollback);
            // +2 accounts for the 3-byte UTF-8 arrow being 1 display column
            let display_len = label.len() as u16 - 2;
            let bot_y = area.y + area.height - 1;
            let start_x = (area.x + area.width - 1).saturating_sub(display_len);
            if start_x > area.x + 1 {
                let indicator_style = Style::default()
                    .fg(self.palette.accent_yellow)
                    .add_modifier(Modifier::BOLD);
                buf.set_string(start_x, bot_y, &label, indicator_style);
            }
        }

        // Inner content area
        let inner = Rect::new(area.x + 1, area.y + 1, area.width - 2, area.height - 2);

        // Render vt100 screen into inner area
        let rows = inner.height.min(self.screen.size().0);
        let cols = inner.width.min(self.screen.size().1);
        for row in 0..rows {
            for col in 0..cols {
                let cell = self.screen.cell(row, col);
                if let Some(cell) = cell {
                    let x = inner.x + col;
                    let y = inner.y + row;
                    if x < inner.right() && y < inner.bottom() {
                        let fg = convert_color(cell.fgcolor());
                        let bg = convert_color(cell.bgcolor());
                        let mut style = Style::default().fg(fg).bg(bg);
                        if cell.bold() {
                            style = style.add_modifier(Modifier::BOLD);
                        }
                        if cell.dim() {
                            style = style.add_modifier(Modifier::DIM);
                        }
                        if cell.italic() {
                            style = style.add_modifier(Modifier::ITALIC);
                        }
                        if cell.underline() {
                            style = style.add_modifier(Modifier::UNDERLINED);
                        }
                        if cell.inverse() {
                            let real_fg = if fg == Color::Reset {
                                self.palette.fg_primary
                            } else {
                                fg
                            };
                            let real_bg = if bg == Color::Reset {
                                self.palette.bg_primary
                            } else {
                                bg
                            };
                            style = style.fg(real_bg).bg(real_fg);
                        }
                        if cell.hidden() {
                            style = style.add_modifier(Modifier::HIDDEN);
                        }
                        if cell.strike() {
                            style = style.add_modifier(Modifier::CROSSED_OUT);
                        }
                        let ch = cell.contents();
                        let display_char = if ch.is_empty() { " " } else { &ch };
                        buf.set_string(x, y, display_char, style);
                    }
                }
            }
        }

        // Search match highlighting
        for (match_idx, sm) in self.search_matches.iter().enumerate() {
            if sm.row < self.scrollback_base_row {
                continue;
            }
            let vp_row = sm.row - self.scrollback_base_row;
            if vp_row >= rows as usize {
                continue;
            }
            let is_active = self.active_match_index == Some(match_idx);
            let hl_bg = if is_active {
                self.palette.accent_yellow
            } else {
                Color::Rgb(113, 89, 32)
            };
            for col in sm.col_start..sm.col_end {
                if col >= cols as usize {
                    break;
                }
                let sx = inner.x + col as u16;
                let sy = inner.y + vp_row as u16;
                if sx < inner.right() && sy < inner.bottom() {
                    let cell = &mut buf[(sx, sy)];
                    if is_active {
                        cell.set_style(Style::default().fg(Color::Rgb(13, 17, 23)).bg(hl_bg));
                    } else {
                        cell.set_style(cell.style().bg(hl_bg));
                    }
                }
            }
        }

        // Selection highlighting
        if let Some((sr, sc, er, ec)) = self.selection {
            let sel_bg = Color::Rgb(24, 50, 78); // dark blue for dark theme
            for row in sr..=er {
                if row >= rows {
                    break;
                }
                let from = if row == sr { sc } else { 0 };
                let to = if row == er {
                    ec
                } else {
                    cols.saturating_sub(1)
                };
                for col in from..=to {
                    if col >= cols {
                        break;
                    }
                    let sx = inner.x + col;
                    let sy = inner.y + row;
                    if sx < inner.right() && sy < inner.bottom() {
                        let cell = &mut buf[(sx, sy)];
                        cell.set_style(cell.style().bg(sel_bg));
                    }
                }
            }
        }

        // Exit overlay centered in inner area
        if let Some(code) = self.exited {
            let exit_color = if code == 0 {
                self.palette.accent_green
            } else {
                self.palette.accent_red
            };
            let line1 = format!(" exited: {code} ");
            let line2 = if self.pane_count > 1 {
                " [p] close pane "
            } else {
                " [t] close tab "
            };
            let max_len = line1.len().max(line2.len()) as u16;
            if inner.width >= max_len && inner.height >= 2 {
                let y1 = inner.y + inner.height / 2 - 1;
                let y2 = y1 + 1;
                let x1 = inner.x + (inner.width - line1.len() as u16) / 2;
                let x2 = inner.x + (inner.width - line2.len() as u16) / 2;
                let exit_style = Style::default().fg(self.palette.bg_primary).bg(exit_color);
                let hint_style = Style::default()
                    .fg(self.palette.bg_primary)
                    .bg(self.palette.fg_muted);
                buf.set_string(x1, y1, &line1, exit_style);
                buf.set_string(x2, y2, line2, hint_style);
            }
        }
    }
}

fn convert_color(color: crate::pty::terminal::Color) -> Color {
    match color {
        crate::pty::terminal::Color::Default => Color::Reset,
        crate::pty::terminal::Color::Idx(i) => Color::Indexed(i),
        crate::pty::terminal::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
