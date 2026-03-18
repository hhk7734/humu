# PRD 007: File Explorer Panel

## Overview

A file explorer panel on the right side of the layout showing the active room's directory tree with Nerd Font icons and git status indicators. Enter opens files in `$EDITOR`, Shift+Enter shows diffs via `delta`. Both open in a floating pane overlay.

## Layout

`[WorkspaceTree | Terminal | Explorer]`

- Explorer is the rightmost panel with `panel_widths[1]` (default: 25 columns, clamped 5-60)
- Resizable with `Shift+←/→` in Explorer mode
- `panel_widths` in `state.yaml` uses `[u16; 2]` for `[workspace_panel, explorer_panel]`

## Mode & Focus

- `Mode::Explorer` entered via `Ctrl+E` (available from any mode via `check_mode_switch`)
- `FocusedPanel::Explorer` for focus tracking, border turns `accent_blue` when focused
- Explorer rescans the tree on mode enter, workspace/room switch, and automatically every ~3 seconds (periodic git status refresh)
- Click on explorer panel: first click focuses + selects, second click on same item opens

## Keybindings (Explorer Mode)

| Key | Action |
|---|---|
| `↑/↓` | Navigate tree |
| `Enter` | Toggle dir expand/collapse, or open file in `$EDITOR` (floating pane) |
| `Shift+Enter` | Open `git diff \| delta --side-by-side` in floating pane (modified files only) |
| `n` | Create new file in the selected directory |
| `Shift+N` | Create new directory in the selected directory |
| `d` | Delete selected file/directory (with confirm popup) |
| `Shift+C` | Copy selected relative path |
| `Shift+←/→` | Resize explorer panel |
| `Shift+I` | Toggle show/hide gitignored files |
| `Esc` | Return to Terminal mode |

## File Tree

- Read filesystem under `current_room_path()`, lazy expansion (only user-toggled dirs)
- Skip `.git/` always
- When hiding ignored files (default): `git ls-files` + `git ls-files --others --exclude-standard`
- When showing ignored files: plain `read_dir` (skip `.git/`)
- Sort: directories first, then case-insensitive alphabetical
- Git status from `git status --porcelain`: `M` → Modified, `A`/`??` → Added, `D` → skip, `R`/`C` → destination as Added
- Status propagated to parent directories (computed from git status HashMap, works for collapsed dirs)

## Rendering

- Nerd Font icons per file extension (nvim-web-devicons style, 40+ extensions) with per-type colors (e.g., Rust brown, Python blue, JS yellow, Go cyan)
- Directory icons: `` collapsed, `` expanded — cyan `#56B6C2`
- Symlink files and symlink directories have distinct purple icons
- Each line segment rendered independently: selector (`accent_blue`), indent, icon (per-type color), filename (git-aware color), git indicator
- Git status: `✗` in `accent_orange` for Modified, `★` in `accent_green` for Added — filenames also colored to match
- Selected line: `▸` indicator + `bg_tertiary` highlight
- Panel title: `" Explorer "` or `" Explorer [+ignored] "`

## Floating Pane

Editor and diff views open in a floating pane overlay (90% of terminal panel area, centered). The floating pane:

- Spawns the PTY at the correct size immediately (no delayed resize)
- Forwards all keyboard input to the PTY
- Forwards mouse events when child has mouse reporting; scroll wheel sends `j`/`k` otherwise
- Cursor rendered inside the overlay
- Closes on `Ctrl+Q`, `Ctrl+G`, or process exit
- Paste (Ctrl+V) forwarded to PTY
- Excluded from `cleanup_exited_panes` (has its own auto-close)

### Actions

- **Enter on file**: `$EDITOR <filepath>` (falls back to `vi`) in floating pane
- **Enter on directory**: toggle expand/collapse
- **Shift+Enter on modified file**: `sh -c "git diff '<path>' | delta --side-by-side --paging=always"` in floating pane (path shell-escaped)
- **`n` / `Shift+N`**: open popup to create a file or directory relative to the selected directory (or the selected entry's parent when a file is highlighted)
- **`d`**: open confirmation popup, then delete the selected file/directory from disk
- **`Shift+C`**: copy the selected path to the clipboard
- **Shift+I**: toggle `show_ignored`, rescan tree (session-only)

## Module Architecture

```
src/explorer/
├── mod.rs       # ExplorerState, FileEntry, tree scan/toggle, git status parsing
└── icons.rs     # Nerd Font file extension icon lookup with per-type colors

src/tui/widgets/explorer_panel.rs  # ExplorerPanel widget (rendering)
```

`spawn_command()` helper on `App` spawns arbitrary commands in a PTY without preset resolution. Takes `(cmd, args, cwd, preset_name, cols, rows)`.
