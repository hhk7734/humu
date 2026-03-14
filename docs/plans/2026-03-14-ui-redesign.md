# UI Redesign Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restyle humu's TUI with a Zellij-inspired GitHub Dark theme, rounded corners, Powerline-style tabs/status bar, and context-aware key hints.

**Architecture:** Add a `theme.rs` module with `Palette` (color constants) and `UiConfig` (simplified_ui, rounded_corners flags). All widgets take `&Palette` and `&UiConfig` references. No theme engine — just centralized constants.

**Tech Stack:** Rust, ratatui 0.29, crossterm 0.28

**Spec:** `docs/specs/2026-03-14-ui-redesign-design.md`

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `src/tui/theme.rs` | Create | `Palette`, `UiConfig`, `BorderChars`, `TabChars`, default GitHub Dark palette |
| `src/tui/mod.rs` | Modify | Add `pub mod theme;` |
| `src/config.rs` | Modify | Add `UiSection` struct with `simplified_ui` and `rounded_corners`, wire into `HumuConfig` |
| `src/tui/widgets/status_bar.rs` | Modify | Powerline rendering, mode colors from palette, responsive hints |
| `src/tui/widgets/terminal_area.rs` | Modify | Powerline tab rendering with palette colors |
| `src/tui/widgets/workspace_panel.rs` | Modify | Rounded borders, palette colors |
| `src/tui/widgets/room_panel.rs` | Modify | Rounded borders, palette colors |
| `src/tui/widgets/terminal_widget.rs` | Modify | Add rounded pane border with title and exit status |
| `src/tui/widgets/preset_selector.rs` | Modify | Rounded borders, palette colors |
| `src/tui/widgets/dialog.rs` | Modify | Rounded borders, palette colors |
| `src/app.rs` | Modify | Instantiate Palette/UiConfig, pass to all widget constructors |

---

## Chunk 1: Foundation — Theme Module and Config

### Task 1: Create `src/tui/theme.rs` with Palette and UiConfig

**Files:**
- Create: `src/tui/theme.rs`
- Modify: `src/tui/mod.rs`

- [ ] **Step 1: Create the theme module file**

```rust
// src/tui/theme.rs
use crate::tui::input::Mode;
use ratatui::style::Color;

/// GitHub Dark color palette.
pub struct Palette {
    // Backgrounds
    pub bg_primary: Color,
    pub bg_secondary: Color,
    pub bg_tertiary: Color,
    // Foregrounds
    pub fg_primary: Color,
    pub fg_secondary: Color,
    pub fg_muted: Color,
    // Accents
    pub accent_blue: Color,
    pub accent_green: Color,
    pub accent_red: Color,
    pub accent_orange: Color,
    pub accent_purple: Color,
    pub accent_yellow: Color,
    pub accent_magenta: Color,
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
    };

    pub fn mode_color(&self, mode: &Mode) -> Color {
        match mode {
            Mode::Normal => self.accent_green,
            Mode::Locked => self.fg_secondary,
            Mode::Pane => self.accent_blue,
            Mode::Tab => self.accent_orange,
            Mode::Workspace => self.accent_purple,
            Mode::Room => self.accent_magenta,
            Mode::Resize => self.accent_yellow,
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
}

impl TabChars {
    pub const POWERLINE: Self = Self { separator: "\u{e0b0}" };
    pub const PLAIN: Self = Self { separator: "│" };
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
```

- [ ] **Step 2: Register the module in `src/tui/mod.rs`**

Add `pub mod theme;` to the existing module list in `src/tui/mod.rs`.

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles with no errors.

- [ ] **Step 4: Commit**

```bash
git add src/tui/theme.rs src/tui/mod.rs
git commit -m "feat(theme): add Palette, UiConfig, BorderChars, TabChars with GitHub Dark defaults"
```

### Task 2: Add UI config to `src/config.rs`

**Files:**
- Modify: `src/config.rs` (lines 28-32, HumuConfig struct)

- [ ] **Step 1: Add UiSection struct and wire into HumuConfig**

Add after the `Preset` struct (after line 24):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSection {
    #[serde(default)]
    pub simplified_ui: bool,
    #[serde(default = "default_rounded_corners")]
    pub rounded_corners: bool,
}

fn default_rounded_corners() -> bool {
    true
}

impl Default for UiSection {
    fn default() -> Self {
        Self {
            simplified_ui: false,
            rounded_corners: true,
        }
    }
}
```

Add the `ui` field to `HumuConfig`:

```rust
pub struct HumuConfig {
    #[serde(default)]
    pub presets: HashMap<String, Preset>,
    #[serde(default)]
    pub ui: UiSection,
}
```

- [ ] **Step 2: Verify it compiles and existing configs still load**

Run: `cargo check`
Expected: Compiles. Existing config files without `[ui]` section deserialize with defaults via `#[serde(default)]`.

- [ ] **Step 3: Commit**

```bash
git add src/config.rs
git commit -m "feat(config): add [ui] section with simplified_ui and rounded_corners"
```

### Task 3: Wire Palette and UiConfig into App

**Files:**
- Modify: `src/app.rs` (App struct ~line 99, App::new/init)

- [ ] **Step 1: Add palette and ui_config fields to App struct**

Add to the App struct:

```rust
pub palette: crate::tui::theme::Palette,
pub ui_config: crate::tui::theme::UiConfig,
```

- [ ] **Step 2: Initialize them in App construction**

In `App::new()`, add these fields to the `Ok(Self { ... })` struct literal at the end of the function (around line 180):

```rust
palette: crate::tui::theme::Palette::GITHUB_DARK,
ui_config: crate::tui::theme::UiConfig {
    simplified_ui: config.ui.simplified_ui,
    rounded_corners: config.ui.rounded_corners,
},
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles with no errors.

- [ ] **Step 4: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): wire Palette and UiConfig into App struct"
```

---

## Chunk 2: Panel Widgets — Workspace and Room

### Task 4: Restyle WorkspacePanel

**Files:**
- Modify: `src/tui/widgets/workspace_panel.rs`
- Modify: `src/app.rs` (line ~879, widget construction)

- [ ] **Step 1: Update WorkspacePanel struct to accept palette and ui_config**

Replace the struct and constructor:

```rust
use crate::tui::theme::{Palette, UiConfig};
use ratatui::widgets::BorderType;

pub struct WorkspacePanel<'a> {
    workspaces: &'a [WorkspaceItem],
    selected: Option<usize>,
    has_focus: bool,
    palette: &'a Palette,
    ui_config: &'a UiConfig,
}
```

Update `new()` signature:

```rust
pub fn new(workspaces: &'a [WorkspaceItem], palette: &'a Palette, ui_config: &'a UiConfig) -> Self {
    Self {
        workspaces,
        selected: None,
        has_focus: false,
        palette,
        ui_config,
    }
}
```

- [ ] **Step 2: Update render() to use palette and rounded borders**

In the `Widget::render` impl:

- Border color: `self.palette.accent_blue` (focused) or `self.palette.fg_muted` (unfocused) — replaces `Color::Cyan` / `Color::DarkGray`
- Add `BorderType::Rounded` when `self.ui_config.rounded_corners` is true, else `BorderType::Plain`
- Title style: `self.palette.fg_secondary`
- Selected item: `self.palette.accent_blue` + BOLD — replaces `Color::Cyan`
- Unselected item: `self.palette.fg_primary`

```rust
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
let block = Block::default()
    .borders(Borders::ALL)
    .border_style(Style::default().fg(border_color))
    .border_type(border_type)
    .title(" Workspaces ")
    .title_style(Style::default().fg(self.palette.fg_secondary));
```

For item styling:

```rust
let style = if Some(i) == self.selected {
    Style::default().fg(self.palette.accent_blue).add_modifier(Modifier::BOLD)
} else {
    Style::default().fg(self.palette.fg_primary)
};
```

- [ ] **Step 3: Update App::render() to pass palette/ui_config**

At line ~879 in `src/app.rs`:

```rust
let ws_widget = WorkspacePanel::new(&workspaces, &self.palette, &self.ui_config)
    .selected(self.workspace_selected)
    .focus(self.focus == FocusedPanel::Workspace);
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check`
Expected: Compiles with no errors.

- [ ] **Step 5: Commit**

```bash
git add src/tui/widgets/workspace_panel.rs src/app.rs
git commit -m "feat(workspace-panel): apply GitHub Dark palette and rounded borders"
```

### Task 5: Restyle RoomPanel

**Files:**
- Modify: `src/tui/widgets/room_panel.rs`
- Modify: `src/app.rs` (line ~886, widget construction)

- [ ] **Step 1: Update RoomPanel struct to accept palette and ui_config**

```rust
use crate::tui::theme::{Palette, UiConfig};
use ratatui::widgets::BorderType;

pub struct RoomPanel<'a> {
    rooms: &'a [RoomItem],
    selected: Option<usize>,
    has_focus: bool,
    palette: &'a Palette,
    ui_config: &'a UiConfig,
}
```

Update constructor:

```rust
pub fn new(rooms: &'a [RoomItem], palette: &'a Palette, ui_config: &'a UiConfig) -> Self {
    Self {
        rooms,
        selected: None,
        has_focus: false,
        palette,
        ui_config,
    }
}
```

- [ ] **Step 2: Update render() to use palette and rounded borders**

Replace hard-coded colors in the `Widget::render` impl:

```rust
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
let block = Block::default()
    .borders(Borders::ALL)
    .border_style(Style::default().fg(border_color))
    .border_type(border_type)
    .title(" Rooms ")
    .title_style(Style::default().fg(self.palette.fg_secondary));
```

For item styling:

```rust
let style = if Some(i) == self.selected {
    Style::default().fg(self.palette.accent_blue).add_modifier(Modifier::BOLD)
} else {
    Style::default().fg(self.palette.fg_primary)
};
```

- [ ] **Step 3: Update App::render() to pass palette/ui_config**

At line ~886:

```rust
let room_widget = RoomPanel::new(&rooms, &self.palette, &self.ui_config)
    .selected(self.room_selected)
    .focus(self.focus == FocusedPanel::Room);
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check`
Expected: Compiles.

- [ ] **Step 5: Commit**

```bash
git add src/tui/widgets/room_panel.rs src/app.rs
git commit -m "feat(room-panel): apply GitHub Dark palette and rounded borders"
```

---

## Chunk 3: Tab Bar and Status Bar — Powerline Rendering

### Task 6: Restyle TabBar with Powerline

**Files:**
- Modify: `src/tui/widgets/terminal_area.rs`
- Modify: `src/app.rs` (line ~966, TabBar construction in `render_terminal_area`)

- [ ] **Step 1: Update TabBar struct to accept palette and ui_config**

```rust
use crate::tui::theme::{Palette, UiConfig};

pub struct TabBar<'a> {
    tab_names: &'a [&'a str],
    active: usize,
    active_indicators: &'a [bool],
    palette: &'a Palette,
    ui_config: &'a UiConfig,
}
```

Update constructor to accept and store `palette` and `ui_config`.

- [ ] **Step 2: Rewrite render() for Powerline tabs**

Replace the entire render body. The key rendering logic:

```rust
fn render(self, area: Rect, buf: &mut Buffer) {
    // Fill background with bg_secondary
    for x in area.x..area.x + area.width {
        buf[(x, area.y)]
            .set_char(' ')
            .set_style(Style::default().bg(self.palette.bg_secondary));
    }

    let sep = self.ui_config.tab_chars().separator;
    let mut x = area.x;

    for (i, name) in self.tab_names.iter().enumerate() {
        let is_active = i == self.active;
        let spinner = if self.active_indicators.get(i).copied().unwrap_or(false) {
            " ⠋"
        } else {
            ""
        };
        let label = format!(" {}{} ", name, spinner);
        let label_width = label.chars().count() as u16;

        let (fg, bg) = if is_active {
            (self.palette.bg_primary, self.palette.accent_blue)
        } else {
            (self.palette.fg_secondary, self.palette.bg_tertiary)
        };

        // Draw tab body
        for (j, ch) in label.chars().enumerate() {
            if x + j as u16 >= area.x + area.width {
                break;
            }
            let mut style = Style::default().fg(fg).bg(bg);
            if is_active {
                style = style.add_modifier(Modifier::BOLD);
            }
            buf[(x + j as u16, area.y)].set_char(ch).set_style(style);
        }
        x += label_width;

        // Draw Powerline separator
        if x < area.x + area.width {
            let next_bg = self.palette.bg_secondary;
            buf[(x, area.y)]
                .set_symbol(sep)
                .set_style(Style::default().fg(bg).bg(next_bg));
            x += 1;
        }
    }

    // Draw "+" button
    if x + 2 < area.x + area.width {
        buf[(x + 1, area.y)]
            .set_char('+')
            .set_style(Style::default().fg(self.palette.fg_muted).bg(self.palette.bg_secondary));
    }
}
```

- [ ] **Step 3: Update App to pass palette/ui_config to TabBar**

In `render_terminal_area` (~line 966):

```rust
let tab_bar = TabBar::new(&tab_names, self.tabs.active_index(), &active_indicators, &self.palette, &self.ui_config);
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check`
Expected: Compiles.

- [ ] **Step 5: Commit**

```bash
git add src/tui/widgets/terminal_area.rs src/app.rs
git commit -m "feat(tab-bar): Powerline-style tabs with GitHub Dark palette"
```

### Task 7: Restyle StatusBar with Powerline

**Files:**
- Modify: `src/tui/widgets/status_bar.rs`
- Modify: `src/app.rs` (line ~895, StatusBar construction)

- [ ] **Step 1: Update StatusBar struct to accept palette and ui_config**

```rust
use crate::tui::theme::{Palette, UiConfig};

pub struct StatusBar<'a> {
    mode: Mode,
    error: Option<&'a str>,
    palette: &'a Palette,
    ui_config: &'a UiConfig,
}
```

Update `new()` to accept `palette` and `ui_config`. Keep the `.error()` builder.

- [ ] **Step 2: Rewrite render() for Powerline status bar**

Delete the free function `mode_badge()` (lines 78-88 in current `status_bar.rs`) — it is replaced by `Palette::mode_color()`. The free function `mode_hints()` (lines 90-140) will be kept but converted to a method on `StatusBar` in Step 3. Rewrite the `Widget::render` impl:

```rust
fn render(self, area: Rect, buf: &mut Buffer) {
    // Fill background with bg_secondary
    for x in area.x..area.x + area.width {
        buf[(x, area.y)]
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
    let mode_label = format!(" {} ", self.mode_label());
    let mode_width = mode_label.chars().count() as u16;
    for (i, ch) in mode_label.chars().enumerate() {
        if x + i as u16 >= area.x + area.width { break; }
        buf[(x + i as u16, area.y)].set_char(ch).set_style(
            Style::default().fg(self.palette.bg_primary).bg(mode_color).add_modifier(Modifier::BOLD)
        );
    }
    x += mode_width;

    // Separator: mode_color -> next_bg
    let next_bg = if self.mode == Mode::Locked {
        self.palette.bg_secondary
    } else {
        self.palette.bg_tertiary
    };
    if x < area.x + area.width {
        buf[(x, area.y)].set_symbol(sep).set_style(
            Style::default().fg(mode_color).bg(next_bg)
        );
        x += 1;
    }

    // LOCKED mode: just show message
    if self.mode == Mode::Locked {
        let msg = " ── INTERFACE LOCKED ── ";
        for (i, ch) in msg.chars().enumerate() {
            if x + i as u16 >= area.x + area.width { break; }
            buf[(x + i as u16, area.y)].set_char(ch).set_style(
                Style::default().fg(self.palette.fg_muted).bg(self.palette.bg_secondary)
            );
        }
        return;
    }

    // "Ctrl +" segment + separator
    let ctrl_label = " Ctrl + ";
    let ctrl_width = ctrl_label.len() as u16;
    for (i, ch) in ctrl_label.chars().enumerate() {
        if x + i as u16 >= area.x + area.width { break; }
        buf[(x + i as u16, area.y)].set_char(ch).set_style(
            Style::default().fg(self.palette.accent_orange).bg(self.palette.bg_tertiary).add_modifier(Modifier::BOLD)
        );
    }
    x += ctrl_width;

    if x < area.x + area.width {
        buf[(x, area.y)].set_symbol(sep).set_style(
            Style::default().fg(self.palette.bg_tertiary).bg(self.palette.bg_secondary)
        );
        x += 1;
    }

    // Key hints
    let hints = self.mode_hints();
    for (key, label) in hints {
        let hint_width = (key.len() + 1 + label.len() + 2) as u16;
        if x + hint_width > area.x + area.width { break; }
        // Space before
        x += 1;
        // Key character
        for ch in key.chars() {
            buf[(x, area.y)].set_char(ch).set_style(
                Style::default().fg(self.palette.fg_muted).bg(self.palette.bg_secondary)
            );
            x += 1;
        }
        // Space
        buf[(x, area.y)].set_char(' ').set_style(
            Style::default().bg(self.palette.bg_secondary)
        );
        x += 1;
        // Label
        for ch in label.chars() {
            if x >= area.x + area.width { break; }
            buf[(x, area.y)].set_char(ch).set_style(
                Style::default().fg(self.palette.fg_secondary).bg(self.palette.bg_secondary)
            );
            x += 1;
        }
    }
}
```

- [ ] **Step 3: Add `mode_label()` and convert `mode_hints()` to a method**

Add `mode_label()` as a method on `StatusBar`:

```rust
fn mode_label(&self) -> &'static str {
    match self.mode {
        Mode::Normal => "NORMAL",
        Mode::Locked => "LOCKED",
        Mode::Pane => "PANE",
        Mode::Tab => "TAB",
        Mode::Workspace => "WORKSPACE",
        Mode::Room => "ROOM",
        Mode::Resize => "RESIZE",
    }
}
```

Convert the existing free function `mode_hints()` (lines 90-140) to a method `fn mode_hints(&self) -> Vec<(&'static str, &'static str)>`. It already returns `Vec<(&str, &str)>` tuples. The key change: for Normal mode, remove the synthetic `("Ctrl+", "")` prefix entry — the new Powerline render handles "Ctrl +" as a separate segment. Normal mode hints should only contain the actual key-action pairs: `[("g", "LOCK"), ("p", "PANE"), ("t", "TAB"), ("w", "WORKSPACE"), ("n", "RESIZE")]`.

- [ ] **Step 4: Update App to pass palette/ui_config to StatusBar**

At line ~895:

```rust
let status = StatusBar::new(self.mode, &self.palette, &self.ui_config)
    .error(self.last_error.as_deref());
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check`
Expected: Compiles.

- [ ] **Step 6: Commit**

```bash
git add src/tui/widgets/status_bar.rs src/app.rs
git commit -m "feat(status-bar): Powerline mode badges with context-aware key hints"
```

---

## Chunk 4: Terminal Pane Borders and Popup Dialogs

### Task 8: Add Rounded Borders to TerminalWidget

**Files:**
- Modify: `src/tui/widgets/terminal_widget.rs`
- Modify: `src/app.rs` (line ~983, TerminalWidget construction)

- [ ] **Step 1: Update TerminalWidget struct**

Add `title`, `palette`, and `ui_config` fields:

```rust
use crate::tui::theme::{Palette, UiConfig};

pub struct TerminalWidget<'a> {
    screen: &'a Screen,
    has_focus: bool,
    exited: Option<i32>,
    title: &'a str,
    palette: &'a Palette,
    ui_config: &'a UiConfig,
}
```

Update `new()` to accept `title: &'a str`, `palette: &'a Palette`, `ui_config: &'a UiConfig`.

- [ ] **Step 2: Rewrite render() to draw border then terminal content**

The render method draws a rounded border frame first, then renders terminal content inside the inner area (2 cols / 2 rows smaller):

```rust
fn render(self, area: Rect, buf: &mut Buffer) {
    if area.width < 4 || area.height < 4 {
        return; // Too small to render
    }

    let bc = self.ui_config.border_chars();
    let border_color = if self.has_focus {
        self.palette.accent_blue
    } else {
        self.palette.fg_muted
    };
    let border_style = Style::default().fg(border_color);

    // Top border: ╭─ title ─...─╮
    buf[(area.x, area.y)].set_symbol(bc.top_left).set_style(border_style);
    buf[(area.x + 1, area.y)].set_symbol(bc.horizontal).set_style(border_style);
    buf[(area.x + 2, area.y)].set_char(' ').set_style(border_style);
    let title_max = (area.width as usize).saturating_sub(6);
    let title_display: String = self.title.chars().take(title_max).collect();
    for (i, ch) in title_display.chars().enumerate() {
        buf[(area.x + 3 + i as u16, area.y)].set_char(ch).set_style(
            Style::default().fg(self.palette.fg_secondary)
        );
    }
    let title_end = area.x + 3 + title_display.len() as u16;
    buf[(title_end, area.y)].set_char(' ').set_style(border_style);
    for x in (title_end + 1)..area.x + area.width - 1 {
        buf[(x, area.y)].set_symbol(bc.horizontal).set_style(border_style);
    }
    buf[(area.x + area.width - 1, area.y)].set_symbol(bc.top_right).set_style(border_style);

    // Side borders
    for y in (area.y + 1)..area.y + area.height - 1 {
        buf[(area.x, y)].set_symbol(bc.vertical).set_style(border_style);
        buf[(area.x + area.width - 1, y)].set_symbol(bc.vertical).set_style(border_style);
    }

    // Bottom border: ╰─ EXIT: N ─...─╯  or  ╰─────...─╯
    buf[(area.x, area.y + area.height - 1)].set_symbol(bc.bottom_left).set_style(border_style);
    if let Some(code) = self.exited {
        let exit_label = format!(" EXIT: {} ", code);
        let exit_color = if code == 0 { self.palette.accent_green } else { self.palette.accent_red };
        buf[(area.x + 1, area.y + area.height - 1)].set_symbol(bc.horizontal).set_style(border_style);
        buf[(area.x + 2, area.y + area.height - 1)].set_char(' ').set_style(border_style);
        for (i, ch) in exit_label.chars().enumerate() {
            let px = area.x + 3 + i as u16;
            if px >= area.x + area.width - 1 { break; }
            buf[(px, area.y + area.height - 1)].set_char(ch).set_style(
                Style::default().fg(exit_color).add_modifier(Modifier::BOLD)
            );
        }
        let exit_end = area.x + 3 + exit_label.len() as u16;
        for x in exit_end..area.x + area.width - 1 {
            buf[(x, area.y + area.height - 1)].set_symbol(bc.horizontal).set_style(border_style);
        }
    } else {
        for x in (area.x + 1)..area.x + area.width - 1 {
            buf[(x, area.y + area.height - 1)].set_symbol(bc.horizontal).set_style(border_style);
        }
    }
    buf[(area.x + area.width - 1, area.y + area.height - 1)].set_symbol(bc.bottom_right).set_style(border_style);

    // Inner content area (terminal output renders here)
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
            let style = Style::default()
                .fg(self.palette.bg_primary)
                .bg(if code == 0 { self.palette.accent_green } else { self.palette.accent_red });
            buf.set_string(x, y, &msg, style);
        }
    }
}
```

Note: The `convert_color()` free function is unchanged — keep it as-is.

- [ ] **Step 3: Update PTY resize to account for border inset**

In `src/app.rs`, update **both** resize sites to subtract the 2-col/2-row border.

Fullscreen path (line 977-978):

```rust
// Before:
if pane.cols() != pane_area.width || pane.rows() != pane_area.height {
    let _ = pane.resize(pane_area.width, pane_area.height);
}

// After:
let inner_w = pane_area.width.saturating_sub(2);
let inner_h = pane_area.height.saturating_sub(2);
if pane.cols() != inner_w || pane.rows() != inner_h {
    let _ = pane.resize(inner_w, inner_h);
}
```

Normal split path (line 993-996):

```rust
// Before:
if let Some(pane) = self.panes.get_mut(pane_id)
    && (pane.cols() != rect.width || pane.rows() != rect.height)
{
    let _ = pane.resize(rect.width, rect.height);
}

// After:
let inner_w = rect.width.saturating_sub(2);
let inner_h = rect.height.saturating_sub(2);
if let Some(pane) = self.panes.get_mut(pane_id)
    && (pane.cols() != inner_w || pane.rows() != inner_h)
{
    let _ = pane.resize(inner_w, inner_h);
}
```

- [ ] **Step 4: Collect exit codes and update both TerminalWidget construction sites**

Exit codes: `pane.exit_status()` requires `&mut self`, but the render loop borrows `self.panes` immutably. Collect exit codes into a `HashMap` in the mutable resize loop before the immutable render loop.

In the normal split path, add after the resize loop (line ~998):

```rust
// Collect exit codes while we have mutable access
let exit_codes: HashMap<PaneId, Option<i32>> = rects
    .iter()
    .filter_map(|(pid, _)| {
        self.panes.get_mut(pid).map(|p| (*pid, p.exit_status()))
    })
    .collect();
```

For fullscreen path, add after the resize (line ~980):

```rust
let fs_exit_code = self.panes.get_mut(&fs_id).and_then(|p| p.exit_status());
```

Update **fullscreen** TerminalWidget construction (line 983):

```rust
let preset_name = self.pane_presets.get(&fs_id).map(|s| s.as_str()).unwrap_or("shell");
let widget = TerminalWidget::new(&screen, preset_name, &self.palette, &self.ui_config)
    .focus(true)
    .exited(fs_exit_code);
```

Update **normal loop** TerminalWidget construction (line 1005):

```rust
let preset_name = self.pane_presets.get(&pane_id).map(|s| s.as_str()).unwrap_or("shell");
let exit_code = exit_codes.get(&pane_id).copied().flatten();
let widget = TerminalWidget::new(&screen, preset_name, &self.palette, &self.ui_config)
    .focus(is_focused)
    .exited(exit_code);
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check`
Expected: Compiles.

- [ ] **Step 6: Commit**

```bash
git add src/tui/widgets/terminal_widget.rs src/app.rs
git commit -m "feat(terminal-widget): rounded pane borders with title and exit status"
```

### Task 9: Restyle PresetSelector with Palette

**Files:**
- Modify: `src/tui/widgets/preset_selector.rs`
- Modify: `src/app.rs` (line ~914, popup rendering)

- [ ] **Step 1: Update PresetSelector struct**

Add `palette: &'a Palette` and `ui_config: &'a UiConfig` fields.

- [ ] **Step 2: Update render() to use palette colors and rounded borders**

- Border: `palette.accent_blue` with `BorderType::Rounded` (when `ui_config.rounded_corners`)
- Selected item: `Style::default().fg(palette.bg_primary).bg(palette.accent_blue).add_modifier(Modifier::BOLD)`
- Unselected: `Style::default().fg(palette.fg_primary)`
- Selection indicator `▸`: same color as selected item

- [ ] **Step 3: Update App popup rendering to pass palette/ui_config**

In `render_popup()` (line 906), update the PresetSelector construction:

```rust
PopupState::PresetSelector { presets, selected, .. } => {
    frame.render_widget(
        PresetSelector::new(presets, *selected, &self.palette, &self.ui_config),
        area,
    );
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check`

- [ ] **Step 5: Commit**

```bash
git add src/tui/widgets/preset_selector.rs src/app.rs
git commit -m "feat(preset-selector): apply GitHub Dark palette and rounded borders"
```

### Task 10: Restyle Dialog with Palette

**Files:**
- Modify: `src/tui/widgets/dialog.rs`
- Modify: `src/app.rs` (popup rendering)

- [ ] **Step 1: Update Dialog struct**

Add `palette: &'a Palette` and `ui_config: &'a UiConfig` fields.

- [ ] **Step 2: Update render() to use palette colors and rounded borders**

Color mapping (all 3 states for each interactive element):

**Border & Labels:**
- Border: `palette.accent_orange` with `BorderType::Rounded`
- Focused label: `palette.accent_orange` + BOLD
- Unfocused label: `palette.fg_secondary`

**TextInput:**
- Focused: bg=`palette.bg_tertiary`, fg=`palette.fg_primary`
- Unfocused: bg=`palette.bg_tertiary`, fg=`palette.fg_secondary`

**Select options (3 states):**
- Focused + selected: `fg=palette.bg_primary, bg=palette.accent_orange` + BOLD
- Selected (field not focused): `fg=palette.accent_orange`
- Unselected: `fg=palette.fg_secondary`

**Confirm Yes button (3 states):**
- Focused + selected: `fg=palette.bg_primary, bg=palette.accent_green`
- Selected (field not focused): `fg=palette.accent_green`
- Unselected: `fg=palette.fg_secondary`

**Confirm No button (3 states):**
- Focused + selected: `fg=palette.bg_primary, bg=palette.accent_red`
- Selected (field not focused): `fg=palette.accent_red`
- Unselected: `fg=palette.fg_secondary`

**Completions:**
- Selected: `fg=palette.bg_primary, bg=palette.accent_blue`
- Unselected: `fg=palette.accent_blue, bg=palette.bg_primary`
- Hint text: `palette.fg_muted`

- [ ] **Step 3: Update App popup rendering to pass palette/ui_config**

In `render_popup()`, update all 4 Dialog construction sites (lines 914, 921, 924, 927):

```rust
PopupState::WorkspaceCreate { fields, focused_field, completions, completion_selected } => {
    let mut dialog = Dialog::new("Create Workspace", fields, *focused_field, &self.palette, &self.ui_config);
    dialog.completions = completions;
    dialog.completion_selected = *completion_selected;
    dialog.completion_field = Some(1);
    frame.render_widget(dialog, area);
}
PopupState::RoomCreate { fields, focused_field } => {
    frame.render_widget(
        Dialog::new("Create Room", fields, *focused_field, &self.palette, &self.ui_config),
        area,
    );
}
PopupState::WorkspaceDelete { fields, focused_field, .. } => {
    frame.render_widget(
        Dialog::new("Delete Workspace", fields, *focused_field, &self.palette, &self.ui_config),
        area,
    );
}
PopupState::RoomDelete { fields, focused_field, .. } => {
    frame.render_widget(
        Dialog::new("Delete Room", fields, *focused_field, &self.palette, &self.ui_config),
        area,
    );
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check`

- [ ] **Step 5: Commit**

```bash
git add src/tui/widgets/dialog.rs src/app.rs
git commit -m "feat(dialog): apply GitHub Dark palette and rounded borders"
```

---

## Chunk 5: Final Integration and Verification

### Task 11: Full Build and Manual Smoke Test

- [ ] **Step 1: Full build**

Run: `cargo build`
Expected: Compiles with no errors and no warnings related to our changes.

- [ ] **Step 2: Run existing tests**

Run: `cargo test`
Expected: All existing tests pass.

- [ ] **Step 3: Manual verification checklist**

Run `cargo run` and visually verify:
1. Workspace panel has rounded borders with blue (focused) / muted (unfocused) colors
2. Room panel has rounded borders with same color scheme
3. Tab bar shows Powerline chevron separators
4. Active tab is blue, inactive tabs are dark
5. Status bar shows mode badge with Powerline separator
6. Mode badge color changes per mode (cycle through with Ctrl+g, Ctrl+p, etc.)
7. Key hints show after the mode badge
8. Terminal panes have rounded borders with title in top border
9. Exit code shows in bottom border when a process exits
10. Popup dialogs (preset selector, create/delete) have rounded borders
11. All colors match GitHub Dark palette

- [ ] **Step 4: Test simplified_ui fallback**

Add `simplified_ui = true` to `~/.humu/config.toml` under `[ui]` and verify:
- Tab bar uses `│` separator instead of ``
- Status bar uses `│` separator instead of ``
- Remove the test config change after verification

- [ ] **Step 5: Test rounded_corners = false**

Add `rounded_corners = false` to config and verify:
- All borders use sharp corners `┌┐└┘`
- Remove the test config change after verification

- [ ] **Step 6: Final commit if any adjustments were needed**

Only commit files that were changed during testing adjustments:

```bash
git add src/tui/ src/app.rs src/config.rs
git commit -m "fix: polish UI redesign after manual testing"
```
