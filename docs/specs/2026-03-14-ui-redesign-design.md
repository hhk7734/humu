# Humu UI Redesign — Zellij-Inspired Pretty Design

**Date:** 2026-03-14
**Status:** Approved

## Overview

Redesign humu's TUI to feel modern and polished, inspired by Zellij's design language. The core changes are: rounded corners, Powerline-style tabs and status bar, a GitHub Dark color palette, borderless ribbon bars, and context-aware key hints.

## Design Decisions

| Decision | Choice |
|---|---|
| Color palette | GitHub Dark (#0d1117 base) |
| Pane corners | Rounded (`╭╮╯╰`), configurable |
| Tab style | Powerline chevrons (U+E0B0), `simplified_ui` fallback |
| Status bar style | Powerline chevrons, borderless ribbon |
| Panel borders | Rounded, color-coded by focus |
| Pane borders | Rounded with title in top border |
| Tab bar position | Terminal area only (right of panels) |
| Font requirement | Nerd Font assumed, `simplified_ui` config for fallback |
| Implementation | Hybrid — `Palette` + `UiConfig` module |

## Architecture

### New Module: `src/tui/theme.rs`

Contains two structs and two helper structs:

#### Palette

Flat struct of `ratatui::style::Color` values. No traits, no generics, `const`-constructible.

```rust
pub struct Palette {
    // Backgrounds
    pub bg_primary: Color,      // #0d1117 — main background
    pub bg_secondary: Color,    // #161b22 — bars, ribbons
    pub bg_tertiary: Color,     // #21262d — active tab bg, hover

    // Foregrounds
    pub fg_primary: Color,      // #c9d1d9 — main text
    pub fg_secondary: Color,    // #8b949e — muted text, hints
    pub fg_muted: Color,        // #484f58 — disabled, dividers

    // Accents
    pub accent_blue: Color,     // #58a6ff — focused borders, active items
    pub accent_green: Color,    // #3fb950 — success, NORMAL mode
    pub accent_red: Color,      // #f85149 — error, exit code non-zero
    pub accent_orange: Color,   // #d29922 — modifier keys, warnings, TAB mode
    pub accent_purple: Color,   // #bc8cff — WORKSPACE mode
    pub accent_yellow: Color,   // #e3b341 — RESIZE mode
    pub accent_magenta: Color,  // #f778ba — ROOM mode
}

impl Palette {
    /// Returns the badge color for a given Mode.
    pub fn mode_color(&self, mode: &Mode) -> Color {
        match mode {
            Mode::Normal    => self.accent_green,   // #3fb950
            Mode::Locked    => self.fg_secondary,   // #8b949e
            Mode::Pane      => self.accent_blue,    // #58a6ff
            Mode::Tab       => self.accent_orange,  // #d29922
            Mode::Workspace => self.accent_purple,  // #bc8cff
            Mode::Resize    => self.accent_yellow,  // #e3b341
            Mode::Room      => self.accent_magenta, // #f778ba
        }
    }
}
```

#### UiConfig

```rust
pub struct UiConfig {
    pub simplified_ui: bool,    // false = Powerline, true = plain separators
    pub rounded_corners: bool,  // true by default
}
```

#### BorderChars

```rust
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
        top_left: "╭", top_right: "╮",
        bottom_left: "╰", bottom_right: "╯",
        horizontal: "─", vertical: "│",
    };
    pub const SHARP: Self = Self {
        top_left: "┌", top_right: "┐",
        bottom_left: "└", bottom_right: "┘",
        horizontal: "─", vertical: "│",
    };
}
```

#### TabChars

```rust
pub struct TabChars {
    pub separator: &'static str,
}

impl TabChars {
    pub const POWERLINE: Self = Self { separator: "\u{e0b0}" };
    pub const PLAIN: Self = Self { separator: "│" };
}
```

### Widget Changes

All widgets stop using hard-coded colors and instead reference `Palette` and `UiConfig` for border/separator selection.

**How Palette/UiConfig flow to widgets:** `App` holds `palette: Palette` and `ui_config: UiConfig` as fields. Each widget constructor takes `&Palette` and `&UiConfig` as additional parameters. For example, `StatusBar::new(mode, &palette, &ui_config)`. This is explicit — no global state, no trait objects, just reference passing.

#### Status Bar

- **Background**: `bg_secondary` — borderless ribbon, no frame
- **Mode badge**: Powerline segment — `bg=mode_color, fg=bg_primary`, bold, followed by `` separator
- **Modifier prefix**: Powerline segment — `bg=bg_tertiary, fg=accent_orange`, bold "Ctrl +", followed by ``
- **Key hints**: `fg_muted` for key character, `fg_secondary` for action label
- **Responsive**: render hints left-to-right; stop when remaining width is insufficient for the next hint group. Drop action labels first, then key characters.
- **LOCKED mode**: shows `"── INTERFACE LOCKED ──"` in `fg_muted`
- **Mode-specific hints**: each mode shows its own relevant keybindings (NORMAL, LOCKED, PANE, TAB, WORKSPACE, RESIZE, ROOM — all 7 modes)
- **`simplified_ui` fallback**: when `simplified_ui = true`, mode badge uses plain `│` separator instead of ``. Same fallback applies to the "Ctrl +" segment.

Mode badge colors (via `Palette::mode_color()`):

| Mode | Color | Hex |
|---|---|---|
| NORMAL | `accent_green` | #3fb950 |
| LOCKED | `fg_secondary` | #8b949e |
| PANE | `accent_blue` | #58a6ff |
| TAB | `accent_orange` | #d29922 |
| WORKSPACE | `accent_purple` | #bc8cff |
| RESIZE | `accent_yellow` | #e3b341 |
| ROOM | `accent_magenta` | #f778ba |

#### Tab Bar

- **Background**: `bg_secondary` — borderless ribbon
- **Active tab**: Powerline segment — `bg=accent_blue, fg=bg_primary`, bold
- **Inactive tab**: Powerline segment — `bg=bg_tertiary, fg=fg_secondary`
- **"+" button**: `fg=fg_muted` on `bg_secondary`
- **Activity spinner**: `⠋` appended to tab name when Claude is active
- **Separator**: `` (Powerline) or `│` (`simplified_ui`)

#### Workspace Panel

- **Border**: rounded (`╭╮╯╰`), `accent_blue` when focused, `fg_muted` when unfocused
- **Title**: rendered in top border line (e.g., `╭─ Workspaces ─╮`)
- **Selected item**: `accent_blue`, bold, `▸` prefix
- **Unselected item**: `fg_primary`
- **Spinner**: `⠋` suffix for active workspaces

#### Room Panel

- Same styling rules as workspace panel
- Title: `╭─ Rooms ─╮`

#### Terminal Pane Borders

- **Each pane** gets a full rounded border (`╭╮╯╰`)
- **Focused pane**: `accent_blue` border
- **Unfocused pane**: `fg_muted` border
- **Top border**: includes pane title (preset/command name) — e.g., `╭─ claude ─╮`
- **Bottom border**: shows exit code when process exits — e.g., `╰─ EXIT: 1 ─╯`
- Exit code 0: `accent_green`, non-zero: `accent_red`

**Implementation details:**
- `TerminalWidget` draws the border within its allocated rect and shrinks the terminal content area by 2 cols / 2 rows internally. `SplitTree::compute_rects()` is unchanged — it still gives full rects to each leaf.
- `TerminalWidget` constructor gains `title: &str` (pane preset/command name) and `focused: bool`. The caller (`App::render`) passes the pane's preset name from `pane_presets` map.
- PTY resize must account for the border inset: when the widget rect is `(w, h)`, the PTY gets `(w-2, h-2)`.
- Workspace and room panels use ratatui's built-in `Block::bordered().border_type(BorderType::Rounded)` since they already use `Block`. `BorderChars` is reserved for manually-drawn terminal pane borders only.

#### Popup Dialogs

- **PresetSelector**: border color `accent_blue`, rounded corners
- **Dialog (create/delete)**: border color `accent_orange`, rounded corners
- **Selected item**: `bg=accent_blue, fg=bg_primary`, bold
- **Focused label**: `accent_orange`, bold
- **Unfocused label**: `fg_secondary`

## Color Reference — GitHub Dark Palette

| Name | Hex | RGB | Usage |
|---|---|---|---|
| `bg_primary` | #0d1117 | (13,17,23) | Main background |
| `bg_secondary` | #161b22 | (22,27,34) | Bars, ribbons |
| `bg_tertiary` | #21262d | (33,38,45) | Active tab bg, hover |
| `fg_primary` | #c9d1d9 | (201,209,217) | Main text |
| `fg_secondary` | #8b949e | (139,148,158) | Muted text, labels |
| `fg_muted` | #484f58 | (72,79,88) | Disabled, dividers |
| `accent_blue` | #58a6ff | (88,166,255) | Focus, active |
| `accent_green` | #3fb950 | (63,185,80) | Success, NORMAL |
| `accent_red` | #f85149 | (248,81,73) | Error, exit fail |
| `accent_orange` | #d29922 | (210,153,34) | Modifiers, TAB |
| `accent_purple` | #bc8cff | (188,140,255) | WORKSPACE |
| `accent_yellow` | #e3b341 | (227,179,65) | RESIZE |
| `accent_magenta` | #f778ba | (247,120,186) | ROOM |

## Unicode Characters Used

| Character | Codepoint | Usage |
|---|---|---|
| `╭` `╮` `╰` `╯` | U+256D/256E/2570/256F | Rounded corners |
| `─` | U+2500 | Horizontal borders |
| `│` | U+2502 | Vertical borders |
| `` | U+E0B0 (PUA) | Powerline separator (Nerd Font) |
| `▸` | U+25B8 | Selection indicator |
| `⠋` | U+280B | Activity spinner |

## Config

The `simplified_ui` and `rounded_corners` flags are read from humu's config file. Defaults:

```toml
[ui]
simplified_ui = false
rounded_corners = true
```

The `[ui]` section is optional — existing config files without it will use defaults via `#[serde(default)]` on the `ui` field in `HumuConfig`.

## Files to Modify

| File | Change |
|---|---|
| `src/tui/theme.rs` | **New** — `Palette`, `UiConfig`, `BorderChars`, `TabChars` |
| `src/tui/mod.rs` | Add `pub mod theme;` |
| `src/tui/widgets/status_bar.rs` | Powerline rendering, mode-colored badges, styled hints |
| `src/tui/widgets/terminal_area.rs` | Powerline tab rendering |
| `src/tui/widgets/terminal_widget.rs` | Rounded pane borders with title/exit status |
| `src/tui/widgets/workspace_panel.rs` | Rounded borders, palette colors |
| `src/tui/widgets/room_panel.rs` | Rounded borders, palette colors |
| `src/tui/widgets/preset_selector.rs` | Rounded borders, palette colors |
| `src/tui/widgets/dialog.rs` | Rounded borders, palette colors |
| `src/app.rs` | Instantiate `Palette` + `UiConfig`, pass to widgets |
| `src/config.rs` | Add `[ui]` section with `simplified_ui`, `rounded_corners` |

## Out of Scope

- Multiple theme support / theme switching
- Pane frame titles with scroll position
- Stacked pane collapse
- Multiplayer user colors
- Dynamic theme loading from files
