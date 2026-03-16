use crate::tui::input::Mode;
use crate::tui::theme::{Palette, UiConfig};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

pub fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Terminal => "TERMINAL",
        Mode::Locked => "LOCKED",
        Mode::Pane => "PANE",
        Mode::Tab => "TAB",
        Mode::Workspace => "WORKSPACE",
        Mode::Room => "ROOM",
        Mode::EnterSearch | Mode::Search => "SEARCH",
    }
}

pub fn mode_hints(mode: Mode) -> Vec<(&'static str, &'static str)> {
    match mode {
        Mode::Terminal => vec![
            ("g", "LOCK"),
            ("f", "FIND"),
            ("p", "PANE"),
            ("t", "TAB"),
            ("w", "WORKSPACE"),
            ("r", "ROOM"),
            (",", "SET"),
        ],
        Mode::Locked => vec![("Ctrl+g", "UNLOCK")],
        Mode::Pane => vec![
            ("n", "New"),
            ("d", "Delete"),
            ("\u{2190}\u{2192}\u{2191}\u{2193}", "Move"),
            ("S+\u{2190}\u{2192}\u{2191}\u{2193}", "Resize"),
            ("f", "Full"),
            ("Esc", "Back"),
        ],
        Mode::Tab => vec![
            ("n", "New"),
            ("d", "Delete"),
            ("\u{2190}\u{2192}", "Prev/Next"),
            ("1-9", "GoTo"),
            ("Esc", "Back"),
        ],
        Mode::Workspace => vec![
            ("\u{2191}\u{2193}", "Navigate"),
            ("Enter", "Select"),
            ("n", "New"),
            ("d", "Delete"),
            ("S+\u{2190}\u{2192}", "Resize"),
        ],
        Mode::Room => vec![
            ("\u{2191}\u{2193}", "Navigate"),
            ("Enter", "Select"),
            ("n", "New"),
            ("d", "Delete"),
            ("S+\u{2190}\u{2192}", "Resize"),
        ],
        Mode::EnterSearch | Mode::Search => vec![],
    }
}

pub fn mode_hints_right(mode: Mode) -> Vec<(&'static str, &'static str)> {
    match mode {
        Mode::Terminal => vec![("n", "NEW PANE")],
        _ => vec![],
    }
}

/// Width of a single Powerline hint segment: arrow + space + key + space + label + space + arrow.
pub fn hint_segment_width(key: &str, label: &str) -> u16 {
    1 + 1 + key.chars().count() as u16 + 1 + label.chars().count() as u16 + 1 + 1
}

pub struct StatusBar<'a> {
    mode: Mode,
    error: Option<&'a str>,
    palette: &'a Palette,
    ui_config: &'a UiConfig,
    search_query: Option<&'a str>,
    /// (active_1based, total, case_sensitive, wrap)
    search_info: Option<(usize, usize, bool, bool)>,
    search_valid: bool,
}

impl<'a> StatusBar<'a> {
    pub fn new(mode: Mode, palette: &'a Palette, ui_config: &'a UiConfig) -> Self {
        Self {
            mode,
            error: None,
            palette,
            ui_config,
            search_query: None,
            search_info: None,
            search_valid: true,
        }
    }

    pub fn error(mut self, error: Option<&'a str>) -> Self {
        self.error = error;
        self
    }

    pub fn search_query(mut self, query: Option<&'a str>) -> Self {
        self.search_query = query;
        self
    }

    pub fn search_info(mut self, info: Option<(usize, usize, bool, bool)>) -> Self {
        self.search_info = info;
        self
    }

    pub fn search_valid(mut self, valid: bool) -> Self {
        self.search_valid = valid;
        self
    }

    /// Render Powerline hint segments starting at `x`.
    /// Each hint is `(key, label, active)`. Active segments get a distinct
    /// background (accent_cyan) and bold label for better visibility.
    /// When `left_pointing` is true, uses left-pointing arrows with swapped fg/bg.
    fn render_hint_segments(
        &self,
        hints: &[(&str, &str, bool)],
        x: &mut u16,
        area: Rect,
        buf: &mut Buffer,
        left_pointing: bool,
    ) {
        let tab_chars = self.ui_config.tab_chars();
        let sep = if left_pointing { tab_chars.separator_left } else { tab_chars.separator };
        let normal_bg = Color::Rgb(139, 148, 158);
        let outer_bg = self.palette.bg_secondary;
        for &(key, label, active) in hints {
            let bg = if active { self.palette.accent_cyan } else { normal_bg };
            let w = hint_segment_width(key, label);
            if *x + w > area.x + area.width {
                break;
            }
            // Arrow fg/bg: left-pointing swaps the colors vs right-pointing
            let (entry_fg, entry_bg) = if left_pointing { (bg, outer_bg) } else { (outer_bg, bg) };
            let (exit_fg, exit_bg) = if left_pointing { (outer_bg, bg) } else { (bg, outer_bg) };
            // Entry arrow
            if *x < area.x + area.width {
                buf[(*x, area.y)].set_symbol(sep).set_style(
                    Style::default().fg(entry_fg).bg(entry_bg),
                );
                *x += 1;
            }
            // Key
            buf[(*x, area.y)].set_char(' ').set_style(Style::default().bg(bg));
            *x += 1;
            for ch in key.chars() {
                if *x >= area.x + area.width { break; }
                buf[(*x, area.y)].set_char(ch).set_style(
                    Style::default()
                        .fg(Color::Rgb(180, 40, 40))
                        .bg(bg)
                        .add_modifier(Modifier::BOLD),
                );
                *x += 1;
            }
            // Label
            if *x < area.x + area.width {
                buf[(*x, area.y)].set_char(' ').set_style(Style::default().bg(bg));
                *x += 1;
            }
            let label_style = if active {
                Style::default()
                    .fg(Color::Rgb(13, 17, 23))
                    .bg(bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Rgb(13, 17, 23)).bg(bg)
            };
            for ch in label.chars() {
                if *x >= area.x + area.width { break; }
                buf[(*x, area.y)].set_char(ch).set_style(label_style);
                *x += 1;
            }
            if *x < area.x + area.width {
                buf[(*x, area.y)].set_char(' ').set_style(Style::default().bg(bg));
                *x += 1;
            }
            // Exit arrow
            if *x < area.x + area.width {
                buf[(*x, area.y)].set_symbol(sep).set_style(
                    Style::default().fg(exit_fg).bg(exit_bg),
                );
                *x += 1;
            }
        }
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Fill background with bg_secondary
        for bx in area.x..area.x + area.width {
            buf[(bx, area.y)]
                .set_char(' ')
                .set_style(Style::default().bg(self.palette.bg_secondary));
        }

        // If error, show error message and return
        if let Some(err) = self.error {
            let err_style = Style::default()
                .fg(self.palette.accent_red)
                .bg(self.palette.bg_secondary)
                .add_modifier(Modifier::BOLD);
            let msg = format!(" ERROR: {} ", err);
            for (i, ch) in msg.chars().enumerate() {
                if area.x + i as u16 >= area.x + area.width {
                    break;
                }
                buf[(area.x + i as u16, area.y)].set_char(ch).set_style(err_style);
            }
            return;
        }

        let sep = self.ui_config.tab_chars().separator;
        let mode_color = self.palette.mode_color(&self.mode);
        let mut x = area.x;

        // Mode badge: [MODE_NAME] + separator
        let mode_label = format!(" {} ", mode_label(self.mode));
        let mode_width = mode_label.chars().count() as u16;
        for (i, ch) in mode_label.chars().enumerate() {
            if x + i as u16 >= area.x + area.width {
                break;
            }
            buf[(x + i as u16, area.y)].set_char(ch).set_style(
                Style::default()
                    .fg(self.palette.bg_primary)
                    .bg(mode_color)
                    .add_modifier(Modifier::BOLD),
            );
        }
        x += mode_width;

        // Separator: mode_color -> bg_secondary
        if x < area.x + area.width {
            buf[(x, area.y)]
                .set_symbol(sep)
                .set_style(Style::default().fg(mode_color).bg(self.palette.bg_secondary));
            x += 1;
        }

        // ── EnterSearch mode: search input bar ──────────────────────────────
        if self.mode == Mode::EnterSearch {
            if let Some(query) = self.search_query {
                let text_style = Style::default()
                    .fg(self.palette.fg_primary)
                    .bg(self.palette.bg_secondary);
                let prefix = " / ";
                for ch in prefix.chars() {
                    if x >= area.x + area.width { break; }
                    buf[(x, area.y)].set_char(ch).set_style(text_style);
                    x += 1;
                }
                for ch in query.chars() {
                    if x >= area.x + area.width { break; }
                    buf[(x, area.y)].set_char(ch).set_style(text_style);
                    x += 1;
                }
                // Cursor block
                if x < area.x + area.width {
                    buf[(x, area.y)].set_char('\u{2588}').set_style(text_style);
                    x += 1;
                }
                // Invalid regex indicator
                if !self.search_valid {
                    let err_style = Style::default()
                        .fg(self.palette.accent_red)
                        .bg(self.palette.bg_secondary);
                    let msg = "  [invalid regex]";
                    for ch in msg.chars() {
                        if x >= area.x + area.width { break; }
                        buf[(x, area.y)].set_char(ch).set_style(err_style);
                        x += 1;
                    }
                }
            }
            return;
        }

        // ── Search mode: query + hints + counter ────────────────────────────
        if self.mode == Mode::Search {
            if let Some(query) = self.search_query {
                let text_style = Style::default()
                    .fg(self.palette.fg_primary)
                    .bg(self.palette.bg_secondary);
                let prefix = format!(" / {} ", query);
                for ch in prefix.chars() {
                    if x >= area.x + area.width { break; }
                    buf[(x, area.y)].set_char(ch).set_style(text_style);
                    x += 1;
                }
            }
            if let Some((active, total, case_sensitive, wrap)) = self.search_info {
                let case_label = if case_sensitive { "CASE" } else { "case" };
                let wrap_label = if wrap { "WRAP" } else { "wrap" };
                let hints: Vec<(&str, &str, bool)> = vec![
                    ("n", "NEXT", false),
                    ("N", "PREV", false),
                    ("c", case_label, case_sensitive),
                    ("w", wrap_label, wrap),
                ];
                self.render_hint_segments(&hints, &mut x, area, buf, false);

                // Match counter
                let counter = format!(" {}/{} ", active, total);
                let counter_style = Style::default()
                    .fg(self.palette.fg_secondary)
                    .bg(self.palette.bg_secondary);
                for ch in counter.chars() {
                    if x >= area.x + area.width { break; }
                    buf[(x, area.y)].set_char(ch).set_style(counter_style);
                    x += 1;
                }
            }
            return;
        }

        // ── LOCKED mode: just show message ──────────────────────────────────
        if self.mode == Mode::Locked {
            let msg = " \u{2500}\u{2500} INTERFACE LOCKED \u{2500}\u{2500} ";
            for (i, ch) in msg.chars().enumerate() {
                if x + i as u16 >= area.x + area.width {
                    break;
                }
                buf[(x + i as u16, area.y)].set_char(ch).set_style(
                    Style::default()
                        .fg(self.palette.fg_muted)
                        .bg(self.palette.bg_secondary),
                );
            }
            return;
        }

        // "Ctrl +" segment only in Terminal mode
        if self.mode == Mode::Terminal {
            let ctrl_bg = self.palette.bg_tertiary;
            let ctrl_label = " Ctrl + ";
            let ctrl_width = ctrl_label.len() as u16;

            if x < area.x + area.width {
                buf[(x, area.y)].set_symbol(sep).set_style(
                    Style::default().fg(self.palette.bg_secondary).bg(ctrl_bg),
                );
                x += 1;
            }
            for (i, ch) in ctrl_label.chars().enumerate() {
                if x + i as u16 >= area.x + area.width {
                    break;
                }
                buf[(x + i as u16, area.y)].set_char(ch).set_style(
                    Style::default()
                        .fg(self.palette.accent_orange)
                        .bg(ctrl_bg)
                        .add_modifier(Modifier::BOLD),
                );
            }
            x += ctrl_width;
            if x < area.x + area.width {
                buf[(x, area.y)].set_symbol(sep).set_style(
                    Style::default().fg(ctrl_bg).bg(self.palette.bg_secondary),
                );
                x += 1;
            }
        }

        // Key hints (none are toggles, so all inactive)
        let hints = mode_hints(self.mode);
        let hint_refs: Vec<(&str, &str, bool)> = hints.iter().map(|&(k, l)| (k, l, false)).collect();
        self.render_hint_segments(&hint_refs, &mut x, area, buf, false);

        // Right-aligned hints (e.g. Alt+n in Terminal mode)
        let right_hints = mode_hints_right(self.mode);
        if !right_hints.is_empty() {
            let sep_left = self.ui_config.tab_chars().separator_left;
            let alt_prefix = " Alt + ";
            let alt_prefix_width = 1 + alt_prefix.len() as u16 + 1; // sep + text + sep
            let hints_width: u16 = right_hints.iter().map(|&(k, l)| {
                hint_segment_width(k, l)
            }).sum();
            let total_right = alt_prefix_width + hints_width;
            let right_start = area.x + area.width.saturating_sub(total_right);

            if right_start > x {
                let mut rx = right_start;
                let alt_bg = self.palette.bg_tertiary;

                // Hint segments first (left-pointing arrows)
                let right_hint_refs: Vec<(&str, &str, bool)> =
                    right_hints.iter().map(|&(k, l)| (k, l, false)).collect();
                self.render_hint_segments(&right_hint_refs, &mut rx, area, buf, true);

                // "Alt +" label (left-pointing arrows)
                if rx < area.x + area.width {
                    buf[(rx, area.y)].set_symbol(sep_left).set_style(
                        Style::default().fg(alt_bg).bg(self.palette.bg_secondary),
                    );
                    rx += 1;
                }
                for (i, ch) in alt_prefix.chars().enumerate() {
                    if rx + i as u16 >= area.x + area.width { break; }
                    buf[(rx + i as u16, area.y)].set_char(ch).set_style(
                        Style::default()
                            .fg(self.palette.accent_orange)
                            .bg(alt_bg)
                            .add_modifier(Modifier::BOLD),
                    );
                }
                rx += alt_prefix.len() as u16;
                if rx < area.x + area.width {
                    buf[(rx, area.y)].set_symbol(sep_left).set_style(
                        Style::default().fg(self.palette.bg_secondary).bg(alt_bg),
                    );
                }
            }
        }
    }
}
