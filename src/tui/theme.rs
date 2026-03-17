// src/tui/theme.rs
use crate::tui::input::Mode;
use ratatui::style::Color;

/// GitHub Dark color palette.
pub struct Palette {
    pub bg_primary: Color,
    pub bg_secondary: Color,
    pub bg_tertiary: Color,
    pub fg_primary: Color,
    pub fg_secondary: Color,
    pub fg_muted: Color,
    pub accent_blue: Color,
    pub accent_green: Color,
    pub accent_red: Color,
    pub accent_orange: Color,
    pub accent_purple: Color,
    pub accent_yellow: Color,
    pub accent_magenta: Color,
    pub accent_cyan: Color,
}

impl Palette {
    pub const GITHUB_DARK: Self = Self {
        bg_primary: Color::Rgb(13, 17, 23),
        bg_secondary: Color::Rgb(22, 27, 34),
        bg_tertiary: Color::Rgb(33, 38, 45),
        fg_primary: Color::Rgb(201, 209, 217),
        fg_secondary: Color::Rgb(139, 148, 158),
        fg_muted: Color::Rgb(72, 79, 88),
        accent_blue: Color::Rgb(88, 166, 255),
        accent_green: Color::Rgb(63, 185, 80),
        accent_red: Color::Rgb(248, 81, 73),
        accent_orange: Color::Rgb(210, 153, 34),
        accent_purple: Color::Rgb(188, 140, 255),
        accent_yellow: Color::Rgb(227, 179, 65),
        accent_magenta: Color::Rgb(247, 120, 186),
        accent_cyan: Color::Rgb(86, 212, 221),
    };

    pub fn mode_color(&self, mode: &Mode) -> Color {
        match mode {
            Mode::Terminal => self.accent_green,
            Mode::Locked => self.fg_secondary,
            Mode::Pane => self.accent_blue,
            Mode::Tab => self.accent_orange,
            Mode::Workspace => self.accent_purple,
            Mode::Explorer => self.accent_yellow,
            Mode::EnterSearch | Mode::Search => self.accent_cyan,
        }
    }
}

pub struct UiConfig {
    pub simplified_ui: bool,
    pub rounded_corners: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            simplified_ui: false,
            rounded_corners: true,
        }
    }
}

pub struct BorderChars {
    pub top_left: &'static str,
    pub top_right: &'static str,
    pub bottom_left: &'static str,
    pub bottom_right: &'static str,
    pub horizontal: &'static str,
    pub vertical: &'static str,
}

impl BorderChars {
    pub const ROUNDED: Self = Self {
        top_left: "╭",
        top_right: "╮",
        bottom_left: "╰",
        bottom_right: "╯",
        horizontal: "─",
        vertical: "│",
    };
    pub const SHARP: Self = Self {
        top_left: "┌",
        top_right: "┐",
        bottom_left: "└",
        bottom_right: "┘",
        horizontal: "─",
        vertical: "│",
    };
}

pub struct TabChars {
    pub separator: &'static str,
    pub separator_left: &'static str,
}

impl TabChars {
    pub const POWERLINE: Self = Self { separator: "\u{e0b0}", separator_left: "\u{e0b2}" };
    pub const PLAIN: Self = Self { separator: "│", separator_left: "│" };
}

impl UiConfig {
    pub fn border_chars(&self) -> &BorderChars {
        if self.rounded_corners {
            &BorderChars::ROUNDED
        } else {
            &BorderChars::SHARP
        }
    }

    pub fn tab_chars(&self) -> &TabChars {
        if self.simplified_ui {
            &TabChars::PLAIN
        } else {
            &TabChars::POWERLINE
        }
    }
}
