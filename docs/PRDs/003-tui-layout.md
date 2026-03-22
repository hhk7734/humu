# TUI Layout

## Main Layout

```
╭──────────────────────╮╭────────────────────────────────────╮╭──────────────╮
│    WORKSPACES        ││ claude ⠋▸▸ shell ▸ [+]             ││              │
│                      ││ ╭─────────────────────────────────╮││  EXPLORER    │
│ ▸ humu               ││ │ $ claude                        ││├──────────────┤
│     main       ?1   ││ │ ⏵ Claude is working...          │││ ▸ src ✗      │
│     feat/x     ↑1   ││ │                                 │││   app.rs ✗   │
│   infra    ⠋         ││ ├─────────────────────────────────┤││   link ->    │
│     main       +3   ││ │ $ cargo test                    │││   Cargo.toml │
│                      ││ │ running 12 tests ... ok         │││              │
│                      ││ │ $  ▋                            │││              │
│                      ││ ╰─────────────────────────────────╯││              │
╰──────────────────────╯╰────────────────────────────────────╯╰──────────────╯
 TERMINAL ▸▸ Ctrl + ▸▸ g LOCK ▸▸ p PANE ▸▸ t TAB ▸▸ w WORKSPACE ▸
```

Three panels plus a status bar. The left panel is a single workspace tree: each workspace row is followed by its room rows.

## Panels

- **WorkspacePanel**: A flattened workspace tree. Workspace rows are 1 line; room rows are 2 lines (name + git summary). Duplicate workspace names (e.g., two repos named `vllm`) are disambiguated as `parent/name` (e.g., `distributed/vllm`, `moreh-dev/vllm`). Rounded border, `accent_blue` when focused, `fg_muted` when unfocused. Selected item: `▸` prefix, bold. The active workspace section (workspace row + all its room rows) uses a blue-gray section background, while the active room uses a darker blue background and stronger text weight on top so the current room stands out within the active workspace. Single-click selects an item and keeps Workspace mode active; activating the selection requires `Enter` or a second click on the same item. Mouse wheel over the panel moves the selection up/down and auto-scrolls long lists. The room summary line always shows a git icon, colored green when clean and orange when dirty; additional markers show ahead/behind and working tree deltas including `?N` for untracked files.
- **Terminal Area**: Tab bar (Powerline-style) at top with `+` button. Each tab is a Powerline segment with entry/exit arrows (first tab has no entry arrow, second+ tabs do). Animated spinner on tabs with active agent panes. Each tab contains one or more split panes (vertical/horizontal) with rounded borders and preset title. Panes run presets with `cwd` set to the room's working directory.
- **ExplorerPanel**: File tree of the active room's directory. Nerd Font icons per file extension plus distinct symlink file/dir icons, git status indicators (`✗` modified, `★` added). Navigate with `↑/↓`, `Enter` opens files in `$EDITOR` (floating pane), `Shift+Enter` opens delta diff (floating pane). `Shift+I` toggles gitignored files.
- **StatusBar**: Borderless ribbon with Powerline mode badge (color-coded per mode), `Ctrl +` segment (Powerline arrows, orange text, `bg_tertiary`), and key hint segments (Powerline arrows, dark red bold keys, black labels on light gray `#8B949E` background). Errors displayed in red, auto-clear on next keypress.

## Terminal Area

- **Tabs**: Each tab is a Powerline segment. First tab starts flush, second+ tabs have an entry arrow. Active tab: `accent_blue` bg, bold white text. Inactive tab: `bg_tertiary` bg. Animated braille spinner (`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`) shown on tabs with active agent panes. `+` button opens the preset selector.
- **Splits within tabs**: Vertical and horizontal splits, nested. One tab might be a single Claude pane; another might be a vertical split with shell + cargo watch.
- **Pane borders**: Rounded (`╭╮╯╰`), focused pane in `accent_blue`, unfocused in `fg_muted`. Title in top border (preset name). Exited panes are automatically closed.

## Preset Selector

When creating a new tab or pane, a popup lists available presets from `config.yaml`. Navigate with arrow keys, select with `Enter`, dismiss with `Esc`. Blocked if no room is selected. After a preset is selected, the mode transitions to Terminal so the user can immediately interact with the new pane.

## Keybindings

Modal approach. Terminal mode passes all input to the active terminal pane. Press a Ctrl key to enter a mode. Arrow keys only — no hjkl.

### Mode Switching

From any sub-mode, `Ctrl+w/e/t/p` switches directly to that mode. `Ctrl+q` quits from any mode or popup (except Locked mode and floating panes — in floating panes it closes the pane). `Ctrl+w` and `Ctrl+e` are idempotent. `Ctrl+t` always returns to Terminal mode. `Ctrl+p` toggles between Pane and Terminal.

| Key | Target Mode |
| --- | ----------- |
| `Ctrl+q` | Quit (global — from any mode/popup; closes floating pane) |
| `Ctrl+g` | Locked (toggle from Terminal) |
| `Ctrl+p` | Pane (toggle with Terminal) |
| `Ctrl+w` | Workspace (idempotent) |
| `Ctrl+e` | Explorer (idempotent) |
| `Ctrl+t` | Terminal (always) |
| `Ctrl+f` | EnterSearch (from Terminal) |
| `Ctrl+,` | Settings popup (from Terminal) |

### Locked Mode

`Ctrl+g` toggles between Terminal and Locked. In Locked mode, all input passes directly to the terminal. For programs that conflict with humu's Ctrl key combos.

### Pane Mode

Manages panes within the terminal area.

| Key | Action |
| --- | ------ |
| `n` | New pane (select direction, then preset) |
| `d` | Delete pane |
| `←↓↑→` | Move focus between panes |
| `Shift+←↓↑→` | Resize pane |
| `f` | Toggle fullscreen |
| `Esc` / `Ctrl+p` | Back to Terminal |

### Tab Mode

Manages tabs within the terminal area. Enter via `Ctrl+t` from Terminal mode.

| Key | Action |
| --- | ------ |
| `n` | New tab (select preset) |
| `d` | Delete tab |
| `←/→` | Previous / next tab |
| `1-9` | Go to tab N |
| `Esc` / `Ctrl+t` | Back to Terminal |

### Workspace Mode

| Key | Action |
| --- | ------ |
| `↑/↓` | Navigate workspace tree |
| `Enter` | Select workspace or room |
| `n` | New room |
| `Shift+N` | New workspace |
| `d` | Delete selected workspace or room |
| `Shift+←/→` | Resize workspace panel |
| `Esc` | Back to Terminal |

Workspace mode auto-returns to Terminal mode after 5 seconds of keyboard inactivity.

### Explorer Mode

| Key | Action |
| --- | ------ |
| `↑/↓` | Navigate tree |
| `Enter` | Toggle dir expand/collapse, or open file in `$EDITOR` (floating pane) |
| `Shift+Enter` | Open `git diff` via delta (floating pane, modified files only) |
| `n` | New file |
| `Shift+N` | New directory |
| `d` | Delete selected entry |
| `Shift+C` | Copy selected path |
| `Shift+←/→` | Resize explorer panel |
| `Shift+I` | Toggle show/hide gitignored files |
| `Esc` | Return to Terminal mode |

Click behavior: first click focuses panel + selects item, second click on same item opens it.

### Floating Pane

A centered overlay (90% of terminal panel area) that runs a PTY process (`$EDITOR` or `delta`). All keys forwarded to the PTY. Mouse scroll sends `j`/`k` to `less`/`delta` when no mouse reporting. Closes on `Ctrl+Q`, `Ctrl+G`, or when the process exits.

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
| `Alt+n` | New pane (Terminal mode only) |

### Mouse Support

Clicking a panel enters the corresponding mode: workspace tree → Workspace mode, terminal area → Terminal mode, explorer panel → Explorer mode. Workspace tree activation uses two steps: first click selects, second click on the same item activates. Explorer activation already follows the same pattern. Clicking tabs, `+` button are also supported. Panel resizing is keyboard-only (`Shift+←→`). All mouse interactions have keyboard equivalents.

**Status bar hint clicks**: Clicking a key hint segment in the status bar triggers the corresponding action (e.g., clicking the "PANE" hint in Terminal mode enters Pane mode). Multi-key hints like arrow navigation are not clickable. Works across all modes including Search mode hints (NEXT, PREV, CASE, WRAP).

**Mouse forwarding**: When a child process enables mouse tracking (e.g., vim, htop), mouse events targeting an actual terminal pane are forwarded as SGR escape sequences with pane-relative coordinates. Humu's own click handling (panel selection, tab switching, status-bar hints) still runs first on Down events to maintain focus. Clicks on the tab bar or other non-pane UI regions are not reinterpreted as terminal input.

**Text selection**: When the child process has no mouse tracking (e.g., plain shell, Claude Code), mouse drag selects text from the vt100 screen with a dark blue highlight. On mouse release, selected text is copied to the system clipboard via OSC 52 escape sequence.

**Bracketed paste**: Multi-line paste from the system clipboard is forwarded as a single block through the terminal input router. If the child process has requested bracketed paste mode, the text is wrapped in `\x1b[200~`...`\x1b[201~` sequences. In EnterSearch mode, pasted text is appended to the search query.

**Keyboard enhancement**: On terminals that support the Kitty keyboard protocol, humu enables `DISAMBIGUATE_ESCAPE_CODES`, `REPORT_ALL_KEYS_AS_ESCAPE_CODES`, `REPORT_ALTERNATE_KEYS`, and `REPORT_EVENT_TYPES` so modified keys arrive as CSI u events with minimal ambiguity. Modified Enter and Tab are forwarded as CSI u sequences (`\x1b[{codepoint};{modifier}u`). Alt+char is forwarded with the standard ESC prefix. For Ctrl-modified Hangul 2-set jamo emitted by IMEs, humu normalizes the jamo to the corresponding QWERTY letter before dispatching shortcuts or forwarding bytes to the PTY, so `Ctrl+ㅊ` behaves like `Ctrl+C`. For keyboard navigation and passthrough, humu handles both key `Press` and auto-repeat events, while ignoring `Release` so held arrow keys keep moving without an extra action when the key is lifted. Gracefully degrades on unsupported terminals.

**Scroll wheel** on terminal panes:
- **Programs with mouse reporting** (vim, less, tmux): scroll events are forwarded as proper mouse escape sequences (SGR or default encoding) with pane-relative coordinates, except when the pane is on the alternate screen. Alternate-screen panes keep humu-managed scrollback so fullscreen TUIs can still be reviewed with wheel scroll.
- **Plain shell / no mouse reporting**: scrolls through the vt100 scrollback buffer (10,000 lines). A yellow `↑N` indicator appears in the pane's bottom border showing lines scrolled back. Scrollback auto-resets to live view on new output or keypress.
- **PageUp/PageDown**: the same router keeps these keys local when the pane has no mouse reporting or is on the alternate screen; otherwise they are forwarded to the PTY after resetting local scrollback.

`App` does not inspect parser internals directly. Terminal rendering goes through the pane facade (`PtyPane`), while terminal mouse/key/paste decisions go through `TerminalInputRouter`, which returns explicit actions such as PTY writes, local scrollback changes, and selection updates.

**Scroll wheel** on list panels:
- **Workspace panel**: focuses Workspace mode, moves the selection up/down by one item per wheel tick, and auto-scrolls long trees so the selection stays visible.
- **Explorer panel**: focuses Explorer mode, moves the selection up/down by one entry per wheel tick, and updates `scroll_offset` so long trees stay navigable without keyboard input.

## Status Bar Structure

All segments use Powerline-style arrows (entry + exit) for clear visual separation.

```
[MODE]▸ ▸[Ctrl +]▸ ▸[key label]▸ ...          ... ◂[key label]◂ ◂[Alt +]◂
```

In Terminal mode, left-aligned hints show `Ctrl+` shortcuts (right-pointing arrows) and right-aligned hints show `Alt+` shortcuts (left-pointing arrows).

| Segment | Background | Key color | Label color |
|---|---|---|---|
| Mode badge | `mode_color` (per mode) | — | `bg_primary` (bold) |
| Ctrl + (Terminal, left) | `bg_tertiary` | — | `accent_orange` (bold) |
| Alt + (Terminal, right) | `bg_tertiary` | — | `accent_orange` (bold) |
| Key hints | `#8B949E` (light gray) | `#B42828` (dark red, bold) | `#0D1117` (black) |

## Status Bar Mode Colors

| Mode | Color |
|---|---|
| TERMINAL | green (#3fb950) |
| LOCKED | gray (#8b949e) |
| PANE | blue (#58a6ff) |
| TAB | orange (#d29922) |
| WORKSPACE | purple (#bc8cff) |
| EXPLORER | yellow (#e3b341) |
| SEARCH | cyan (#56d4dd) |
