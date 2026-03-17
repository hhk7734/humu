# PRD 007: File Explorer Panel

## Overview

A file explorer panel on the right side of the layout that displays the workspace directory tree with Nerd Font icons and git status indicators. Users can navigate the tree, open files in `$EDITOR`, and view diffs of modified files via `delta`.

## Layout

Layout changes from `[Workspace | Room | Terminal]` to `[Workspace | Room | Terminal | Explorer]`.

- Explorer is the rightmost panel with `panel_widths[2]` (default: 25 columns, clamped 5-60)
- Resizable with `Shift+←/→` in Explorer mode
- `PanelRects` gains an `explorer: Rect` field

### panel_widths Migration

`panel_widths` in `state.yaml` changes from `Option<[u16; 2]>` to `Option<[u16; 3]>`. To handle existing `state.yaml` files with 2-element arrays:

- Use a custom `Deserialize` impl (or intermediate `Vec<u16>`) that accepts both lengths
- 2-element: map to `[widths[0], widths[1], 25]` (default explorer width)
- 3-element: use as-is
- Missing/null: use `[20, 18, 25]`

This avoids deserialization errors on upgrade.

## Mode & Focus

- New `Mode::Explorer` entered via `Ctrl+E` from Terminal mode
- `Ctrl+E` is added to `handle_terminal()` as an explicit match (like `Ctrl+P`, `Ctrl+W`, etc.) — NOT in `check_mode_switch()` since Explorer mode is only entered from Terminal, not toggled from sub-modes
- `FocusedPanel::Explorer` added for focus tracking
- In `handle_action(EnterMode)`: the mode→focus match must map `Mode::Explorer → FocusedPanel::Explorer` (not fall through to the `_ => Terminal` catch-all)
- All exhaustive `FocusedPanel` matches in `app.rs` must add `Explorer` arms (resize dispatch, navigate, select, delete dialog, etc. — approximately 10 match sites)
- Explorer panel border turns `accent_blue` when focused
- Clicking the explorer panel enters Explorer mode (same pattern as workspace/room panels)

**Note on Ctrl+E:** This overrides the readline end-of-line binding in terminal mode. This is an acceptable tradeoff — the binding follows the same pattern as `Ctrl+W` (which overrides readline's kill-word) and `Ctrl+R` (which overrides reverse search). Users who need the readline binding can use Locked mode (`Ctrl+G`).

## Keybindings (Explorer Mode)

| Key | Action |
|---|---|
| `↑/↓` | Navigate tree |
| `Enter` | Toggle dir expand/collapse, or open file in `$EDITOR` pane |
| `Shift+Enter` | Open `git diff <file> \| delta` in a pane (modified files only) |
| `Shift+←/→` | Resize explorer panel |
| `Shift+I` | Toggle show/hide gitignored files |
| `Esc` | Return to Terminal mode |

## File Tree Data Model

```rust
struct FileEntry {
    name: String,
    path: PathBuf,                  // relative to workspace root
    kind: FileKind,                 // File or Directory
    git_status: Option<GitStatus>,  // Modified, Added, or None
    depth: usize,                   // indentation level
    expanded: bool,                 // directories only
}

enum FileKind { File, Directory }
enum GitStatus { Modified, Added }
```

### Tree Building

- Read filesystem under `current_room_path()` using `std::fs::read_dir` recursively
- Only expand directories that the user has toggled open (lazy — don't read collapsed dirs)
- Skip `.git/` directory always
- When hiding ignored files (default): use `git ls-files` + `git ls-files --others --exclude-standard` to get tracked + untracked-but-not-ignored files
- When showing ignored files: use plain `std::fs::read_dir` (still skip `.git/`)
- Sort: directories first, then alphabetical within each group

### Git Status

- Run `git status --porcelain` once, parse into `HashMap<PathBuf, GitStatus>`
- Two-character XY format: first char = index (staging area), second char = worktree
- Mapping rules:
  - `M` in either column → `GitStatus::Modified`
  - `A` in index column → `GitStatus::Added`
  - `??` (untracked) → `GitStatus::Added`
  - `R` (rename): parse the `-> new_path` portion, treat new path as `Added`
  - `D` (deleted): skip entirely (file doesn't exist on disk)
  - `C` (copy): treat destination as `Added`
  - All other codes (unmerged `U`, etc.): treat as `Modified`
- Status propagated upward: if any child is modified/added, parent directory inherits the highest-priority status (Modified > Added)

### Refresh

- Rescan on entering Explorer mode (mode switch trigger)
- No continuous polling — rescan is synchronous and runs on the main thread (consistent with existing git operations in `workspace.rs` and `room.rs`)
- In large repos, `git ls-files` and `git status` may take noticeable time; this is an accepted tradeoff for v1

## Rendering

### Directory Icons

| State | Icon |
|---|---|
| Collapsed | `` |
| Expanded | `` |

### File Icons (Nerd Font, nvim-web-devicons)

| Extension | Icon | Extension | Icon |
|---|---|---|---|
| `rs` | `` | `py` | `` |
| `js` | `` | `ts` | `` |
| `jsx` | `` | `tsx` | `` |
| `go` | `` | `java` | `` |
| `c` | `` | `cpp` | `` |
| `h` | `` | `hpp` | `` |
| `sh`/`bash`/`zsh` | `` | `lua` | `` |
| `json` | `` | `yaml`/`yml` | `` |
| `toml` | `` | `xml` | `󰗀` |
| `html` | `` | `css` | `` |
| `md` | `` | `txt` | `󰈙` |
| `lock` | `` | `Dockerfile` | `󰡨` |
| `Makefile` | `` | `git` | `` |
| (default) | `` | | |

### Git Status Indicators

| Status | Indicator | Color |
|---|---|---|
| Modified | `✗` | `accent_orange` |
| Added/Untracked | `★` | `accent_green` |
| Clean | (none) | default |

Status propagated to parent directories.

### Render Format

```
 ▸  src ✗
     app.rs ✗
     main.rs
    Cargo.toml
```

- Indentation: `depth * 2` spaces
- Selected line: `▸` indicator + highlight background
- Scrolling keeps selected item visible
- Panel title: `" Explorer "` or `" Explorer [+ignored] "` when gitignored files are shown

## Actions

### Enter on a file

Spawns a new PTY pane running `$EDITOR <filepath>` (falls back to `vi` if `$EDITOR` unset). Uses a new `spawn_command()` helper (separate from `spawn_pane()` which is preset-based). The pane is inserted into the active tab's split tree as a horizontal split on the focused pane. CWD set to workspace/room root. The pane is tracked in `pane_presets` with a synthetic name (e.g., `"_editor"`) for layout persistence.

### Enter on a directory

Toggles expand/collapse. Collapsed → reads directory contents and expands. Expanded → collapses and removes children from visible list.

### Shift+Enter on a modified file

1. Check if `delta` is available: `which delta` (synchronous, cached after first check)
2. If not found: show error via `last_error` ("delta not installed — install from https://github.com/dandavison/delta")
3. If found: spawn PTY pane running `sh -c "git diff <filepath> | delta --paging=always"` (shell required for pipe)
4. Pane inserted into active tab's split tree, CWD set to workspace/room root
5. Tracked with synthetic preset name `"_diff"`

### Shift+Enter on clean/added file or directory

No-op.

### Shift+I

Toggles `show_ignored: bool` flag. Triggers full tree rescan. Session-only, not persisted. Resets on workspace/room switch along with the rest of `ExplorerState`.

### spawn_command() Helper

New method on `App` separate from `spawn_pane()`:

```rust
fn spawn_command(&mut self, cmd: &str, args: &[&str], cwd: &Path, preset_name: &str) -> Option<PaneId>
```

- Calls `PtyPane::spawn_with_envs()` directly with the given command, args, and CWD
- Sets `HUMU_*` env vars (workspace/room/tab/pane IDs) for consistency
- Inserts into `self.panes` and `self.pane_presets` (with `preset_name`)
- Returns the `PaneId` for split-tree insertion by the caller
- Does NOT inject Claude-specific args (`--settings`, `--resume`)

## Explorer State

```rust
struct ExplorerState {
    entries: Vec<FileEntry>,          // flattened visible tree
    selected: usize,                  // cursor index
    scroll_offset: usize,             // viewport scrolling
    expanded_dirs: HashSet<PathBuf>,  // which directories are open
    show_ignored: bool,               // gitignore toggle (default: false)
    root: PathBuf,                    // current workspace/room root
    delta_available: Option<bool>,    // cached `which delta` result
}
```

- Lives on `App` as `explorer_state: ExplorerState`
- Resets when switching workspace/room (different file tree)
- Rescans on entering Explorer mode
- `delta_available` is lazily checked on first Shift+Enter, then cached for the session

## Module Architecture

### New Files

```
src/explorer/
├── mod.rs       # ExplorerState, FileEntry, tree scan/toggle operations
└── icons.rs     # Extension-to-icon lookup table

src/tui/widgets/explorer_panel.rs  # ExplorerPanel widget (rendering)
```

### Changes to Existing Files

| File | Changes |
|---|---|
| `src/app.rs` | Add `explorer_state` field, `FocusedPanel::Explorer` (+ all exhaustive match arms), Explorer mode handling in `handle_action`, `Ctrl+E` in `handle_terminal`, `PanelRects.explorer`, extend layout to 4 panels, `spawn_command()` helper, pane spawning for editor/diff, rescan on mode enter, click handling for explorer panel |
| `src/tui/input.rs` | Add `Mode::Explorer`, `handle_explorer()` keybindings, `Ctrl+E` in `handle_terminal()` |
| `src/tui/widgets/status_bar.rs` | Add Explorer mode hints, mode label, mode color |
| `src/tui/theme.rs` | Add Explorer mode color to `Palette::mode_color()` |
| `src/config.rs` | Extend `panel_widths` to support 3 elements with backward-compatible deserialization |
| `src/lib.rs` | Add `pub mod explorer;` |

### No New Crate Dependencies

Uses `std::fs` for filesystem, `std::process::Command` for git and delta commands, existing crates for everything else.
