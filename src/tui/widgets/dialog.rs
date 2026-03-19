use crate::tui::theme::{Palette, UiConfig};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Widget};

pub struct Dialog<'a> {
    pub title: &'a str,
    pub fields: &'a [DialogField],
    pub focused_field: usize,
    /// Completion suggestions to show below the designated field.
    pub completions: &'a [String],
    pub completion_selected: Option<usize>,
    /// Which field index shows completions (e.g. Some(1) for Path).
    pub completion_field: Option<usize>,
    palette: &'a Palette,
    ui_config: &'a UiConfig,
}

pub enum DialogField {
    TextInput { label: String, value: String },
    Select { label: String, options: Vec<String>, selected: usize },
    Confirm { message: String, yes: bool },
    Checkbox { label: String, checked: bool },
}

impl DialogField {
    pub fn label(&self) -> &str {
        match self {
            DialogField::TextInput { label, .. } => label,
            DialogField::Select { label, .. } => label,
            DialogField::Confirm { message, .. } => message,
            DialogField::Checkbox { label, .. } => label,
        }
    }
}

impl<'a> Dialog<'a> {
    pub fn new(
        title: &'a str,
        fields: &'a [DialogField],
        focused_field: usize,
        palette: &'a Palette,
        ui_config: &'a UiConfig,
    ) -> Self {
        Self {
            title,
            fields,
            focused_field,
            completions: &[],
            completion_selected: None,
            completion_field: None,
            palette,
            ui_config,
        }
    }
}

impl Widget for Dialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Dialog size is based on fields only -- completions render as an overlay.
        let field_rows = self.fields.len() as u16 * 2;
        let height = (field_rows + 3).min(area.height);
        let width = 60u16.min(area.width);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup = Rect::new(x, y, width, height);

        Clear.render(popup, buf);

        let border_type = if self.ui_config.rounded_corners {
            BorderType::Rounded
        } else {
            BorderType::Plain
        };
        let block = Block::default()
            .title(format!(" {} ", self.title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.palette.accent_orange))
            .border_type(border_type);
        let inner = block.inner(popup);
        block.render(popup, buf);

        // Track where the completion-target field's input row ends up.
        let mut completion_anchor_row: Option<u16> = None;
        let mut completion_anchor_x = inner.x;

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
                        Style::default()
                            .fg(self.palette.accent_orange)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.palette.fg_secondary)
                    };
                    buf.set_string(inner.x, row, label, label_style);
                    row += 1;
                    if row >= inner.y + inner.height {
                        break;
                    }
                    // Input box line
                    let input_style = if focused {
                        Style::default()
                            .fg(self.palette.fg_primary)
                            .bg(self.palette.bg_tertiary)
                    } else {
                        Style::default()
                            .fg(self.palette.fg_secondary)
                            .bg(self.palette.bg_tertiary)
                    };
                    let display_width = inner.width as usize;
                    let padded = format!("{:<width$}", value, width = display_width);
                    buf.set_string(inner.x, row, &padded, input_style);

                    // Remember position for completion overlay.
                    if self.completion_field == Some(i) && focused {
                        completion_anchor_row = Some(row + 1);
                        completion_anchor_x = inner.x;
                    }
                    row += 1;
                }
                DialogField::Select {
                    label,
                    options,
                    selected,
                } => {
                    let label_style = if focused {
                        Style::default()
                            .fg(self.palette.accent_orange)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.palette.fg_secondary)
                    };
                    buf.set_string(inner.x, row, label, label_style);
                    row += 1;
                    if row >= inner.y + inner.height {
                        break;
                    }
                    let mut col = inner.x;
                    for (j, opt) in options.iter().enumerate() {
                        let text = format!("[{}]", opt);
                        let opt_style = if j == *selected && focused {
                            Style::default()
                                .fg(self.palette.bg_primary)
                                .bg(self.palette.accent_orange)
                                .add_modifier(Modifier::BOLD)
                        } else if j == *selected {
                            Style::default().fg(self.palette.accent_orange)
                        } else {
                            Style::default().fg(self.palette.fg_secondary)
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
                    let msg_style = if focused {
                        Style::default()
                            .fg(self.palette.accent_orange)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.palette.fg_secondary)
                    };
                    buf.set_string(inner.x, row, message, msg_style);
                    row += 1;
                    if row >= inner.y + inner.height {
                        break;
                    }
                    let yes_style = if *yes && focused {
                        Style::default()
                            .fg(self.palette.bg_primary)
                            .bg(self.palette.accent_green)
                    } else if *yes {
                        Style::default().fg(self.palette.accent_green)
                    } else {
                        Style::default().fg(self.palette.fg_secondary)
                    };
                    let no_style = if !yes && focused {
                        Style::default()
                            .fg(self.palette.bg_primary)
                            .bg(self.palette.accent_red)
                    } else if !yes {
                        Style::default().fg(self.palette.accent_red)
                    } else {
                        Style::default().fg(self.palette.fg_secondary)
                    };
                    buf.set_string(inner.x, row, "[Yes]", yes_style);
                    buf.set_string(inner.x + 6, row, "[No]", no_style);
                    row += 1;
                }
                DialogField::Checkbox { label, checked } => {
                    let marker = if *checked { "[x]" } else { "[ ]" };
                    let text = format!("{} {}", marker, label);
                    let style = if focused {
                        Style::default()
                            .fg(self.palette.accent_orange)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.palette.fg_secondary)
                    };
                    buf.set_string(inner.x, row, &text, style);
                    row += 1;
                }
            }
        }

        // Bottom hint
        if row < inner.y + inner.height {
            let has_completions =
                self.completion_field.is_some() && !self.completions.is_empty();
            let hint = if has_completions {
                "Tab: complete  \u{2191}\u{2193}: move  Enter: confirm  Esc: cancel"
            } else {
                "Tab/\u{2191}\u{2193}: move  Enter: confirm  Esc: cancel"
            };
            let hint_style = Style::default().fg(self.palette.fg_muted);
            buf.set_string(inner.x, row, hint, hint_style);
        }

        // Render completion overlay outside the dialog box (drawn over underlying content).
        if let Some(anchor_y) = completion_anchor_row {
            if !self.completions.is_empty() {
                let max_width = width.saturating_sub(2) as usize;
                for (ci, suggestion) in self.completions.iter().enumerate() {
                    let cy = anchor_y + ci as u16;
                    if cy >= area.y + area.height {
                        break;
                    }
                    // Clear the row background for this overlay line.
                    let bg_style = Style::default().bg(self.palette.bg_primary);
                    let blank = " ".repeat(max_width);
                    buf.set_string(completion_anchor_x, cy, &blank, bg_style);

                    let style = if self.completion_selected == Some(ci) {
                        Style::default()
                            .fg(self.palette.bg_primary)
                            .bg(self.palette.accent_blue)
                    } else {
                        Style::default()
                            .fg(self.palette.accent_blue)
                            .bg(self.palette.bg_primary)
                    };
                    let truncated: String = suggestion.chars().take(max_width).collect();
                    let padded = format!("{:<width$}", truncated, width = max_width);
                    buf.set_string(completion_anchor_x, cy, &padded, style);
                }
            }
        }
    }
}
