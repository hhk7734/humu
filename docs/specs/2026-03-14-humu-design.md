# Humu — Design Specification

A TUI-based multi-task manager built on git concepts. Workspaces map to repositories, rooms map to worktrees, and terminal panes run presets (Claude Code, shell, etc.) within each room.

## Architecture

Single Rust binary. No client-server, no database.

```
humu (single binary)
├── TUI Layer (ratatui + crossterm)
│   ├── WorkspacePanel
│   ├── RoomPanel
│   ├── TerminalArea (tabs + split panes)
│   └── StatusBar (current mode keybindings)
├── Git Layer
│   ├── Workspace manager (clone / init / register)
│   └── Room manager (worktree add / remove / list)
├── PTY Layer (portable-pty)
│   └── Spawn shell/claude processes per pane
├── Terminal Emulation (vt100 crate)
│   └── Parse PTY output → screen buffer → ratatui cells
├── Hook Layer
│   └── Unix socket server at ~/.humu/humu.sock
└── State Layer
    └── ~/.humu/
        ├── config.toml
        ├── state.toml
        └── worktrees/
```

### Tech Stack

| Layer                | Choice       |
| -------------------- | ------------ |
| Language             | Rust         |
| TUI framework        | ratatui      |
| Terminal backend     | crossterm    |
| PTY                  | portable-pty |
| Terminal emulation   | vt100        |

### Rendering Pipeline

```
PTY output → vt100 (parse ANSI/VT) → screen buffer → ratatui cells
User keystrokes → ratatui → PTY input
```

Resize events propagate to the PTY via `SIGWINCH`.

## Domain Model

### Workspace

A 1:1 mapping to a git repository.

**Creation — three modes:**

1. **Clone remote**: directory path + git URL (HTTP or SSH) → `git clone <url> <path>`
2. **Existing repo**: point to an existing local git directory
3. **New project**: directory path → `git init <path>`

Workspace name is derived from the repo directory name. If a name collision occurs (two repos with the same directory name), humu appends a numeric suffix (e.g., `infra`, `infra-2`).

**Deletion:** Remove from humu config. Prompt user: "Also delete the repo on disk?" If yes, remove the directory. All worktrees under `~/.humu/worktrees/<workspace>/` are removed either way.

### Room

A working context within a workspace.

**Default room:** The repository's main working directory. Displayed as the repo's current branch name (e.g., `main`). Always exists when a workspace exists. Cannot be created or deleted manually.

**Additional rooms:** Git worktrees branching from a user-specified base branch.

- Create: `git worktree add -b <branch> ~/.humu/worktrees/<workspaceName>/<roomName> <baseBranch>`
  - `workspaceName` = repo name
  - `roomName` = branch name
- Delete: Prompt user for confirmation. Then `git worktree remove <path>` followed by `git branch -D <branch>`.
- List: derived from `git worktree list` + default room

### Terminal Pane

An embedded terminal within a room, spawned from a **preset**.

- Each pane spawns a PTY with `cwd` set to the room's working directory
- The vt100 crate maintains a virtual screen buffer per pane
- When a pane's process exits, the pane shows a distinct "exited" state with exit code. User can press `Enter` to restart the same preset or `x` (in Pane mode) to close the pane.

## Presets

Two built-in presets, extensible via config. Environment variables in `command` and `args` are expanded at PTY spawn time using the user's environment.

```toml
# ~/.humu/config.toml

[presets.claude]
command = "claude"
args = []

[presets.shell]
command = "$SHELL"
args = []
```

Custom presets:

```toml
[presets.cargo-watch]
command = "cargo"
args = ["watch", "-x", "test"]
```

## TUI Layout

```
┌──────────────┬───────────┬──────────────────────────────────────────┐
│              │           │ [claude ⠋] [shell] [+]                  │
│  WORKSPACES  │   ROOMS   │ ┌──────────────────────────────────────┐ │
│              │           │ │ $ claude                             │ │
│  ▸ humu ⠋   │   main    │ │ ⏵ Claude is working...              │ │
│    infra     │ ▸ feat/x ⠋│ │                                     │ │
│    docs      │   fix/y   │ ├──────────────────────────────────────┤ │
│              │           │ │ $ cargo test                         │ │
│              │           │ │ running 12 tests ... ok              │ │
│              │           │ │ $  ▋                                 │ │
│              │           │ └──────────────────────────────────────┘ │
├──────────────┴───────────┴──────────────────────────────────────────┤
│ Ctrl+g LOCK │ Ctrl+p PANE │ Ctrl+t TAB │ Ctrl+w WORKSPACE │ ...   │
└─────────────────────────────────────────────────────────────────────┘
```

### Panels

- **WorkspacePanel**: Lists workspaces (repo names). Selected prefixed with `▸`. Spinner `⠋` when Claude is active in any room.
- **RoomPanel**: Lists rooms. Default room always first. Spinner `⠋` when Claude is active.
- **TerminalArea**: Tab bar at top + split panes within each tab.
- **StatusBar**: Bottom line showing available keybindings for the current mode.

Draggable resize handles between panels.

### Terminal Area

- **Tabs**: Each tab is a container for one or more split panes. Tab bar shows preset name + spinner for Claude. `+` button opens the preset selector to create a new tab.
- **Splits within tabs**: Each tab can have its own split layout — vertical and horizontal splits, nested. One tab might be a single Claude pane; another might be a vertical split with shell + cargo watch.

### Preset Selector

When creating a new tab or pane (`+` button, `n` in Pane/Tab mode), a popup menu lists available presets from `config.toml`. Navigate with `j/k`, select with `Enter`, dismiss with `Esc`. The selected preset's command is spawned in a new PTY.

### Layout Persistence

Tab and pane layout is saved per room and restored on room switch or restart.

## Claude Hook Integration

### Mechanism

1. Humu runs a single Unix socket server at `~/.humu/humu.sock`
2. When launching a Claude preset, humu sets environment variables:
   - `HUMU_SOCKET=~/.humu/humu.sock`
   - `HUMU_WORKSPACE=<workspace>`
   - `HUMU_ROOM=<room>`
3. Claude Code hooks run a script that sends JSON events to the socket
4. Humu receives all events and stores them; what to act on is decided in code

### Event Format

All hook events use the same schema. Humu stores all received events; what to act on is decided in code.

```json
{"workspace": "humu", "room": "feat/auth", "hook_type": "PreToolUse", "tool": "Edit"}
```

Fields:
- `workspace` (required): workspace name
- `room` (required): room/branch name
- `hook_type` (required): Claude Code hook type (e.g., `PreToolUse`, `PostToolUse`, `Notification`, `Stop`)
- Additional fields vary by hook type and are passed through as-is

### Hook Script

Shipped with or installed by humu. Merges workspace/room identifiers into the hook's JSON payload as flat top-level fields:

```bash
#!/bin/bash
if [ -n "$HUMU_SOCKET" ] && command -v socat &> /dev/null && command -v jq &> /dev/null; then
  INPUT=$(cat)
  echo "$INPUT" | jq -c \
    --arg ws "$HUMU_WORKSPACE" \
    --arg rm "$HUMU_ROOM" \
    --arg ht "$CLAUDE_HOOK_TYPE" \
    '. + {workspace: $ws, room: $rm, hook_type: $ht}' \
    | socat - UNIX-CONNECT:"$HUMU_SOCKET" 2>/dev/null || true
fi
```

### In-Progress Indicator

- Binary: spinner on or spinner off
- **Spinner ON**: when humu receives any `PreToolUse` event (Claude is about to do work)
- **Spinner OFF**: when humu receives a `Stop` event, or no events received for a timeout period (e.g., 10 seconds)
- Shown on: room name in RoomPanel, workspace name in WorkspacePanel, tab label in TerminalArea
- Only for Claude presets — shell presets do not show activity indicators

## Keybindings

Zellij-style modal approach. Normal mode passes all input to the active terminal pane. Press a Ctrl key to enter a mode, single keys act within that mode.

### Status Bar

Bottom line displays available keybindings for the current mode:

- **Normal mode**: `Ctrl+ g LOCK │ p PANE │ t TAB │ w WORKSPACE │ n RESIZE`
- **Pane mode**: `n New │ d Split↓ │ r Split→ │ x Close │ hjkl Move │ f Fullscreen │ Esc Back`

### Locked Mode

`Ctrl+g` toggles between Normal and Locked. In Locked mode, all input passes directly to the terminal — no humu keybindings are intercepted. The status bar shows: `Ctrl+g UNLOCK`. This is for running programs that conflict with humu's Ctrl key combos.

### Mode Entry (from Normal mode)

| Key | Mode | Purpose |
| --- | ---- | ------- |
| `Ctrl+g` | Locked | Pass all input to terminal |
| `Ctrl+p` | Pane | Manage splits within a tab |
| `Ctrl+t` | Tab | Manage tabs |
| `Ctrl+w` | Workspace | Navigate workspaces/rooms |
| `Ctrl+n` | Resize | Resize panels and panes |

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
| `n` | Create: opens workspace-creation dialog when WorkspacePanel is focused (fields: mode, path, URL); opens room-creation dialog when RoomPanel is focused (fields: branch name, base branch) |
| `x` | Delete: deletes focused workspace or room with confirmation prompt |
| `Esc` / `Ctrl+w` | Back to Normal |

### Resize Mode

Targets the boundary nearest to the focused element. When focus is on a terminal pane, resizes the split boundary within the tab. When focus is on a panel (WorkspacePanel, RoomPanel), resizes the panel border.

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

Mouse clicks are supported for basic interactions: selecting a workspace/room in the list panels, clicking a tab, and clicking the `+` button. Dragging resize handles between panels. All mouse interactions have keyboard equivalents.

## Configuration

### `~/.humu/config.toml` (user-edited)

```toml
[presets.claude]
command = "claude"
args = []

[presets.shell]
command = "$SHELL"
args = []
```

### `~/.humu/state.toml` (auto-managed)

```toml
active_workspace = "humu"
active_room = "feat/auth"

[workspaces.humu]
path = "/home/user/github/humu"

[workspaces.infra]
path = "/home/user/github/infra"

# Layout is stored as JSON-in-TOML for nested tree structures
[layout."humu"."feat/auth"]
active_tab = 0
tabs = [
  # Tab with vertical split: claude on top, shell on bottom
  { name = "claude", split = { direction = "vertical", ratio = 0.5, children = [
    { preset = "claude" },
    { preset = "shell" },
  ]}},
  # Tab with a single pane
  { name = "shell", split = { preset = "shell" }},
]
```

### Directory Structure

```
~/.humu/
├── config.toml
├── state.toml
├── humu.sock
└── worktrees/
    └── <workspaceName>/
        └── <roomName>/
```
