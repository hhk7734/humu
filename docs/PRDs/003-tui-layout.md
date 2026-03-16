# TUI Layout

## Main Layout

```
╭──────────────╮╭───────────╮╭──────────────────────────────────────────╮
│              ││           ││ claude ⠋▸▸ shell ▸ [+]                 │
│  WORKSPACES  ││   ROOMS   ││ ╭──────────────────────────────────────╮ │
│              ││           ││ │ $ claude                             │ │
│  ▸ humu ⠋   ││   main    ││ │ ⏵ Claude is working...              │ │
│    infra     ││ ▸ feat/x ⠋││ │                                     │ │
│    docs      ││   fix/y   ││ ├──────────────────────────────────────┤ │
│              ││           ││ │ $ cargo test                         │ │
│              ││           ││ │ running 12 tests ... ok              │ │
│              ││           ││ │ $  ▋                                 │ │
│              ││           ││ ╰──────────────────────────────────────╯ │
╰──────────────╯╰───────────╯╰──────────────────────────────────────────╯
 TERMINAL ▸▸ Ctrl + ▸▸ g LOCK ▸▸ p PANE ▸▸ t TAB ▸▸ w WS ▸▸ r ROOM ▸
```

Three panels separated by draggable resize handles, plus a status bar.

## Panels

- **WorkspacePanel**: Lists workspaces (repo names). Rounded border, `accent_blue` when focused, `fg_muted` when unfocused. Selected item: `▸` prefix, bold. Spinner `⠋` when Claude is active.
- **RoomPanel**: Lists rooms in the selected workspace. Same styling as WorkspacePanel. Default room always first.
- **Terminal Area**: Tab bar (Powerline-style) at top with `+` button. Each tab is a Powerline segment with entry/exit arrows (first tab has no entry arrow, second+ tabs do). Animated spinner on tabs with active Claude agents. Each tab contains one or more split panes (vertical/horizontal) with rounded borders and preset title. Panes run presets with `cwd` set to the room's working directory.
- **StatusBar**: Borderless ribbon with Powerline mode badge (color-coded per mode), `Ctrl +` segment (Powerline arrows, orange text, `bg_tertiary`), and key hint segments (Powerline arrows, dark red bold keys, black labels on light gray `#8B949E` background). Errors displayed in red, auto-clear on next keypress.

## Terminal Area

- **Tabs**: Each tab is a Powerline segment. First tab starts flush, second+ tabs have an entry arrow. Active tab: `accent_blue` bg, bold white text. Inactive tab: `bg_tertiary` bg. Animated braille spinner (`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`) shown on tabs with active Claude agents. `+` button opens the preset selector.
- **Splits within tabs**: Vertical and horizontal splits, nested. One tab might be a single Claude pane; another might be a vertical split with shell + cargo watch.
- **Pane borders**: Rounded (`╭╮╯╰`), focused pane in `accent_blue`, unfocused in `fg_muted`. Title in top border (preset name), exit code in bottom border when process exits.

## Preset Selector

When creating a new tab or pane, a popup lists available presets from `config.yaml`. Navigate with arrow keys, select with `Enter`, dismiss with `Esc`. Blocked if no room is selected.

## Keybindings

Modal approach. Terminal mode passes all input to the active terminal pane. Press a Ctrl key to enter a mode. Arrow keys only — no hjkl.

### Mode Switching

From any sub-mode, `Ctrl+w/r/t/p` switches directly to that mode. `Ctrl+w` and `Ctrl+r` are idempotent — pressing them while already in that mode keeps focus there. `Ctrl+t` always returns to Terminal mode, focusing the last active pane. `Ctrl+p` toggles between Pane and Terminal.

| Key | Target Mode |
| --- | ----------- |
| `Ctrl+g` | Locked (toggle from Terminal) |
| `Ctrl+p` | Pane (toggle with Terminal) |
| `Ctrl+w` | Workspace (idempotent) |
| `Ctrl+r` | Room (idempotent) |
| `Ctrl+t` | Terminal (always) |
| `Ctrl+q` | Quit (from Terminal only) |
| `Ctrl+f` | EnterSearch (from Terminal) |
| `Ctrl+,` | Settings popup (from Terminal) |

### Locked Mode

`Ctrl+g` toggles between Terminal and Locked. In Locked mode, all input passes directly to the terminal. For programs that conflict with humu's Ctrl key combos.

### Pane Mode

Manages panes within the terminal area.

| Key | Action |
| --- | ------ |
| `n` | New pane (select preset) |
| `d` | Split down |
| `r` | Split right |
| `x` | Close pane |
| `←↓↑→` | Move focus between panes |
| `Shift+←↓↑→` | Resize pane |
| `f` | Toggle fullscreen |
| `Esc` / `Ctrl+p` | Back to Terminal |

### Tab Mode

Manages tabs within the terminal area. Enter via `Ctrl+t` from Terminal mode.

| Key | Action |
| --- | ------ |
| `n` | New tab (select preset) |
| `x` | Close tab |
| `←/→` | Previous / next tab |
| `1-9` | Go to tab N |
| `Esc` / `Ctrl+t` | Back to Terminal |

### Workspace Mode

| Key | Action |
| --- | ------ |
| `↑/↓` | Navigate list |
| `Enter` | Select |
| `n` | Create workspace or room (context-dependent) |
| `d` | Delete workspace or room (context-dependent) |
| `Shift+←/→` | Resize workspace panel |

### Room Mode

| Key | Action |
| --- | ------ |
| `↑/↓` | Navigate list |
| `Enter` | Select room |
| `n` | Create room |
| `d` | Delete room |
| `Shift+←/→` | Resize room panel |

### EnterSearch Mode

`Ctrl+f` from Terminal mode enters search. Type a regex query with live highlighting.

| Key | Action |
| --- | ------ |
| Any char | Append to query |
| `Backspace` | Delete last char |
| `Enter` | Confirm (enter Search mode) |
| `Esc` | Cancel search |

### Search Mode

Navigate search results in the focused pane.

| Key | Action |
| --- | ------ |
| `n` | Next match |
| `N` | Previous match |
| `c` | Toggle case sensitivity |
| `w` | Toggle wrap-around |
| `↑/↓` | Scroll up/down |
| `PageUp/PageDown` | Page scroll |
| `Esc` | Exit search |

### Shared (all modes except Locked)

| Key | Action |
| --- | ------ |
| `Alt+←/→` | Move focus left / right between panels |
| `Alt+↑/↓` | Navigate up / down within panel |

### Mouse Support

Clicking a panel enters the corresponding mode: workspace panel → Workspace mode, room panel → Room mode, terminal area → Terminal mode. Clicking tabs, `+` button, and dragging resize handles are also supported. All mouse interactions have keyboard equivalents.

**Status bar hint clicks**: Clicking a key hint segment in the status bar triggers the corresponding action (e.g., clicking the "PANE" hint in Terminal mode enters Pane mode). Multi-key hints like arrow navigation are not clickable. Works across all modes including Search mode hints (NEXT, PREV, CASE, WRAP).

**Bracketed paste**: Multi-line paste from the system clipboard is forwarded as a single block. If the child process has requested bracketed paste mode, the text is wrapped in `\x1b[200~`...`\x1b[201~` sequences. In EnterSearch mode, pasted text is appended to the search query.

**Keyboard enhancement**: On terminals that support the Kitty keyboard protocol, humu enables `DISAMBIGUATE_ESCAPE_CODES` to distinguish modified keys (e.g., Shift+Enter vs Enter). Modified Enter and Tab are forwarded as CSI u sequences (`\x1b[{codepoint};{modifier}u`). Alt+char is forwarded with the standard ESC prefix. Gracefully degrades on unsupported terminals.

**Scroll wheel** on terminal panes:
- **Programs with mouse reporting** (vim, less, tmux): scroll events are forwarded as proper mouse escape sequences (SGR or default encoding) with pane-relative coordinates.
- **Plain shell / no mouse reporting**: scrolls through the vt100 scrollback buffer (10,000 lines). A yellow `↑N` indicator appears in the pane's bottom border showing lines scrolled back. Scrollback auto-resets to live view on new output or keypress.

## Status Bar Structure

All segments use Powerline-style arrows (entry + exit) for clear visual separation.

```
[MODE]▸ ▸[Ctrl +]▸ ▸[key label]▸ ▸[key label]▸ ...
```

| Segment | Background | Key color | Label color |
|---|---|---|---|
| Mode badge | `mode_color` (per mode) | — | `bg_primary` (bold) |
| Ctrl + (Terminal only) | `bg_tertiary` | — | `accent_orange` (bold) |
| Key hints | `#8B949E` (light gray) | `#B42828` (dark red, bold) | `#0D1117` (black) |

## Status Bar Mode Colors

| Mode | Color |
|---|---|
| TERMINAL | green (#3fb950) |
| LOCKED | gray (#8b949e) |
| PANE | blue (#58a6ff) |
| TAB | orange (#d29922) |
| WORKSPACE | purple (#bc8cff) |
| ROOM | magenta (#f778ba) |
| SEARCH | cyan (#56d4dd) |
