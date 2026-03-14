# TUI Layout

## Main Layout

```
╭──────────────╮╭───────────╮╭──────────────────────────────────────────╮
│              ││           ││ [claude ⠋] [shell] [+]                  │
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
 NORMAL ▸ Ctrl+  g LOCK  p PANE  t TAB  w WORKSPACE  r ROOM  n RESIZE
```

Three panels separated by draggable resize handles, plus a status bar.

## Panels

- **WorkspacePanel**: Lists workspaces (repo names). Rounded border, `accent_blue` when focused, `fg_muted` when unfocused. Selected item: `▸` prefix, bold. Spinner `⠋` when Claude is active.
- **RoomPanel**: Lists rooms in the selected workspace. Same styling as WorkspacePanel. Default room always first.
- **Terminal Area**: Tab bar (Powerline-style) at top with `+` button. Each tab contains one or more split panes (vertical/horizontal) with rounded borders and preset title. Panes run presets with `cwd` set to the room's working directory.
- **StatusBar**: Borderless ribbon with Powerline mode badge (color-coded per mode) and context-aware key hints. `Ctrl+` prefix only in Normal mode. Errors displayed in red, auto-clear on next keypress.

## Terminal Area

- **Tabs**: Each tab is a container for one or more split panes. Tab bar shows preset name + spinner for Claude. `+` button opens the preset selector.
- **Splits within tabs**: Vertical and horizontal splits, nested. One tab might be a single Claude pane; another might be a vertical split with shell + cargo watch.
- **Pane borders**: Rounded (`╭╮╯╰`), focused pane in `accent_blue`, unfocused in `fg_muted`. Title in top border (preset name), exit code in bottom border when process exits.

## Preset Selector

When creating a new tab or pane, a popup lists available presets from `config.toml`. Navigate with `j/k`, select with `Enter`, dismiss with `Esc`. Blocked if no room is selected.

## Keybindings

Zellij-style modal approach. Normal mode passes all input to the active terminal pane. Press a Ctrl key to enter a mode, single keys act within that mode.

### Mode Entry (from Normal mode)

| Key | Mode | Purpose |
| --- | ---- | ------- |
| `Ctrl+g` | Locked | Pass all input to terminal |
| `Ctrl+p` | Pane | Manage splits within a tab |
| `Ctrl+t` | Tab | Manage tabs |
| `Ctrl+w` | Workspace | Navigate workspaces and rooms |
| `Ctrl+r` | Room | Navigate rooms |
| `Ctrl+n` | Resize | Resize panels and panes |

### Locked Mode

`Ctrl+g` toggles between Normal and Locked. In Locked mode, all input passes directly to the terminal. For programs that conflict with humu's Ctrl key combos.

### Pane Mode

| Key | Action |
| --- | ------ |
| `n` | New pane (select preset) |
| `d` | Split down |
| `r` | Split right |
| `x` | Close pane |
| `h/j/k/l` | Move focus between panes |
| `f` | Toggle fullscreen |
| `Esc` / `Ctrl+p` | Back to Normal |

### Tab Mode

| Key | Action |
| --- | ------ |
| `n` | New tab (select preset) |
| `x` | Close tab |
| `h/l` | Previous / next tab |
| `1-9` | Go to tab N |
| `r` | Rename tab |
| `Esc` / `Ctrl+t` | Back to Normal |

### Workspace Mode

| Key | Action |
| --- | ------ |
| `h/l` | Focus workspace panel / room panel |
| `j/k` | Navigate list up / down |
| `Enter` | Select |
| `n` | Create workspace or room (context-dependent) |
| `x` | Delete workspace or room (context-dependent) |
| `Esc` / `Ctrl+w` | Back to Normal |

### Room Mode

| Key | Action |
| --- | ------ |
| `j/k` | Navigate list up / down |
| `Enter` | Select room |
| `n` | Create room |
| `x` | Delete room |
| `Esc` / `Ctrl+r` | Back to Normal |

### Resize Mode

Targets the boundary nearest to the focused element.

| Key | Action |
| --- | ------ |
| `h/j/k/l` | Resize in direction |
| `H/J/K/L` | Resize in opposite direction |
| `Esc` / `Ctrl+n` | Back to Normal |

### Shared (all modes except Locked)

| Key | Action |
| --- | ------ |
| `Alt+h/l` | Move focus left / right between panels |
| `Alt+j/k` | Move focus up / down within panel |

### Mouse Support

Clicking a panel enters the corresponding mode: workspace panel → Workspace mode, room panel → Room mode, terminal area → Normal mode. Clicking tabs, `+` button, and dragging resize handles are also supported. All mouse interactions have keyboard equivalents.

## Status Bar Mode Colors

| Mode | Color |
|---|---|
| NORMAL | green (#3fb950) |
| LOCKED | gray (#8b949e) |
| PANE | blue (#58a6ff) |
| TAB | orange (#d29922) |
| WORKSPACE | purple (#bc8cff) |
| ROOM | magenta (#f778ba) |
| RESIZE | yellow (#e3b341) |
