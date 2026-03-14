use crate::tui::theme::{Palette, UiConfig};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;
use vt100::Screen;

pub struct TerminalWidget<'a> {
    screen: &'a Screen,
    has_focus: bool,
    exited: Option<i32>,
    title: &'a str,
    palette: &'a Palette,
    ui_config: &'a UiConfig,
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
            title,
            palette,
            ui_config,
        }
    }

    pub fn focus(mut self, focused: bool) -> Self {
        self.has_focus = focused;
        self
    }

    pub fn exited(mut self, exit_code: Option<i32>) -> Self {
        self.exited = exit_code;
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
                buf[(px, area.y + area.height - 1)].set_char(ch).set_style(
                    Style::default()
                        .fg(exit_color)
                        .add_modifier(Modifier::BOLD),
                );
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
                        if cell.italic() {
                            style = style.add_modifier(Modifier::ITALIC);
                        }
                        if cell.underline() {
                            style = style.add_modifier(Modifier::UNDERLINED);
                        }
                        if cell.inverse() {
                            style = Style::default().fg(bg).bg(fg);
                        }
                        let ch = cell.contents();
                        let display_char = if ch.is_empty() { " " } else { &ch };
                        buf.set_string(x, y, display_char, style);
                    }
                }
            }
        }

        // Exit overlay centered in inner area
        if let Some(code) = self.exited {
            let msg = format!(" [exited: {code}] Press Enter to restart ");
            let msg_len = msg.len() as u16;
            if inner.width >= msg_len && inner.height > 0 {
                let x = inner.x + (inner.width - msg_len) / 2;
                let y = inner.y + inner.height / 2;
                let style = Style::default().fg(self.palette.bg_primary).bg(if code == 0 {
                    self.palette.accent_green
                } else {
                    self.palette.accent_red
                });
                buf.set_string(x, y, &msg, style);
            }
        }
    }
}

fn convert_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
