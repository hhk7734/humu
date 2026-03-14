use crate::tui::theme::{Palette, UiConfig};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Widget};

pub struct PresetSelector<'a> {
    presets: &'a [String],
    selected: usize,
    palette: &'a Palette,
    ui_config: &'a UiConfig,
}

impl<'a> PresetSelector<'a> {
    pub fn new(
        presets: &'a [String],
        selected: usize,
        palette: &'a Palette,
        ui_config: &'a UiConfig,
    ) -> Self {
        Self {
            presets,
            selected,
            palette,
            ui_config,
        }
    }
}

impl Widget for PresetSelector<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let width = 30u16.min(area.width);
        let height = (self.presets.len() as u16 + 2).min(area.height);
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
            .title(" Select Preset ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.palette.accent_blue))
            .border_type(border_type);
        let inner = block.inner(popup);
        block.render(popup, buf);

        for (i, name) in self.presets.iter().enumerate() {
            if i as u16 >= inner.height {
                break;
            }
            let style = if i == self.selected {
                Style::default()
                    .fg(self.palette.bg_primary)
                    .bg(self.palette.accent_blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.palette.fg_primary)
            };
            let prefix = if i == self.selected { " \u{25b8} " } else { "   " };
            buf.set_string(
                inner.x,
                inner.y + i as u16,
                format!("{prefix}{name}"),
                style,
            );
        }
    }
}
