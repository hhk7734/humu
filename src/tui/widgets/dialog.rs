use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Widget};

pub struct Dialog<'a> {
    pub title: &'a str,
    pub fields: &'a [DialogField],
    pub focused_field: usize,
}

pub enum DialogField {
    TextInput { label: String, value: String },
    Select { label: String, options: Vec<String>, selected: usize },
    Confirm { message: String, yes: bool },
}

impl DialogField {
    pub fn label(&self) -> &str {
        match self {
            DialogField::TextInput { label, .. } => label,
            DialogField::Select { label, .. } => label,
            DialogField::Confirm { message, .. } => message,
        }
    }
}

impl<'a> Dialog<'a> {
    pub fn new(title: &'a str, fields: &'a [DialogField], focused_field: usize) -> Self {
        Self { title, fields, focused_field }
    }
}

impl Widget for Dialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Each field takes 2 rows (label + input), plus 2 for border
        let field_rows = self.fields.len() as u16 * 2;
        let height = (field_rows + 3).min(area.height);
        let width = 50u16.min(area.width);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup = Rect::new(x, y, width, height);

        Clear.render(popup, buf);
        let block = Block::default()
            .title(format!(" {} ", self.title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));
        let inner = block.inner(popup);
        block.render(popup, buf);

        let mut row = inner.y;
        for (i, field) in self.fields.iter().enumerate() {
            if row >= inner.y + inner.height {
                break;
            }
            let focused = i == self.focused_field;

            match field {
                DialogField::TextInput { label, value } => {
                    // Label line
                    let label_style = if focused {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Gray)
                    };
                    buf.set_string(inner.x, row, label, label_style);
                    row += 1;
                    if row >= inner.y + inner.height {
                        break;
                    }
                    // Input box line
                    let input_style = if focused {
                        Style::default().fg(Color::White).bg(Color::DarkGray)
                    } else {
                        Style::default().fg(Color::Gray).bg(Color::DarkGray)
                    };
                    let display_width = inner.width as usize;
                    let padded = format!("{:<width$}", value, width = display_width);
                    buf.set_string(inner.x, row, &padded, input_style);
                    row += 1;
                }
                DialogField::Select { label, options, selected } => {
                    // Label line
                    let label_style = if focused {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Gray)
                    };
                    buf.set_string(inner.x, row, label, label_style);
                    row += 1;
                    if row >= inner.y + inner.height {
                        break;
                    }
                    // Inline options: show all as "[Option]" with selected highlighted
                    let mut col = inner.x;
                    for (j, opt) in options.iter().enumerate() {
                        let text = format!("[{}]", opt);
                        let opt_style = if j == *selected && focused {
                            Style::default().fg(Color::Black).bg(Color::Yellow)
                        } else if j == *selected {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default().fg(Color::Gray)
                        };
                        buf.set_string(col, row, &text, opt_style);
                        col += text.len() as u16 + 1;
                        if col >= inner.x + inner.width {
                            break;
                        }
                    }
                    row += 1;
                }
                DialogField::Confirm { message, yes } => {
                    // Message line
                    let msg_style = Style::default().fg(Color::White);
                    buf.set_string(inner.x, row, message, msg_style);
                    row += 1;
                    if row >= inner.y + inner.height {
                        break;
                    }
                    // Yes / No buttons
                    let yes_style = if *yes && focused {
                        Style::default().fg(Color::Black).bg(Color::Green)
                    } else if *yes {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::Gray)
                    };
                    let no_style = if !yes && focused {
                        Style::default().fg(Color::Black).bg(Color::Red)
                    } else if !yes {
                        Style::default().fg(Color::Red)
                    } else {
                        Style::default().fg(Color::Gray)
                    };
                    buf.set_string(inner.x, row, "[Yes]", yes_style);
                    buf.set_string(inner.x + 6, row, "[No]", no_style);
                    row += 1;
                }
            }
        }

        // Bottom hint
        if row < inner.y + inner.height {
            let hint = "Tab/↑↓: move  Enter: confirm  Esc: cancel";
            let hint_style = Style::default().fg(Color::DarkGray);
            buf.set_string(inner.x, row, hint, hint_style);
        }
    }
}
