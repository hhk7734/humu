use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;
use vt100::Screen;

pub struct TerminalWidget<'a> {
    screen: &'a Screen,
    has_focus: bool,
    exited: Option<i32>,
}

impl<'a> TerminalWidget<'a> {
    pub fn new(screen: &'a Screen) -> Self {
        Self {
            screen,
            has_focus: false,
            exited: None,
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
        let rows = area.height.min(self.screen.size().0);
        let cols = area.width.min(self.screen.size().1);

        for row in 0..rows {
            for col in 0..cols {
                let cell = self.screen.cell(row, col);
                if let Some(cell) = cell {
                    let x = area.x + col;
                    let y = area.y + row;

                    if x < area.right() && y < area.bottom() {
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

        // Show exit status overlay
        if let Some(code) = self.exited {
            let msg = format!(" [exited: {code}] Press Enter to restart ");
            let msg_len = msg.len() as u16;
            if area.width >= msg_len && area.height > 0 {
                let x = area.x + (area.width - msg_len) / 2;
                let y = area.y + area.height / 2;
                let style = Style::default()
                    .fg(Color::Black)
                    .bg(if code == 0 { Color::Green } else { Color::Red });
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
