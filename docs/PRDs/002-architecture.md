# Architecture

Single Rust binary. No client-server, no database.

```
humu (single binary)
├── TUI Layer (ratatui + crossterm)
│   ├── WorkspacePanel
│   ├── RoomPanel
│   ├── TerminalArea (tabs + split panes)
│   └── StatusBar (mode badge + key hints)
├── Git Layer
│   ├── Workspace manager (clone / init / register)
│   └── Room manager (worktree add / remove / list)
├── PTY Layer (portable-pty)
│   └── Spawn shell/claude processes per pane
├── Terminal Emulation (vt100 crate)
│   └── Parse PTY output → screen buffer → ratatui cells
├── Hook Layer
│   └── Unix socket server at ~/.humu/humu.sock
├── Theme Layer
│   ├── Palette (GitHub Dark color scheme)
│   └── UiConfig (simplified_ui, rounded_corners)
└── State Layer
    └── ~/.humu/
        ├── config.toml
        ├── state.toml
        └── worktrees/
```

## Tech Stack

| Layer              | Choice       |
| ------------------ | ------------ |
| Language           | Rust         |
| TUI framework      | ratatui      |
| Terminal backend   | crossterm    |
| PTY               | portable-pty |
| Terminal emulation | vt100        |

## Rendering Pipeline

```
PTY output → vt100 (parse ANSI/VT) → screen buffer → ratatui cells
User keystrokes → ratatui → PTY input
```

PTY reads run in a background thread with `mpsc::channel`, using `try_recv()` in the main event loop to avoid blocking. Resize events propagate to the PTY via `SIGWINCH`.

## Workspace Management

Three creation modes:

1. **Clone remote** — user provides a directory path + git URL (HTTP or SSH) → `git clone <url> <path>`
2. **Existing repo** — user points to an existing local git directory
3. **New project** — user provides a directory path → `git init <path>`

Workspace name is derived from the repo directory name. If a name collision occurs, humu appends a numeric suffix (e.g., `infra`, `infra-2`). The path field supports fuzzy filesystem autocomplete with cross-segment matching.

On creation, humu auto-selects the new workspace and its default room (main branch).

## Room Management

- **Default room**: The repository's main working directory. Always exists, cannot be deleted.
- **Additional rooms**: Created as git worktrees branching from a user-specified base branch.
  - `git worktree add -b <branch> ~/.humu/worktrees/<workspaceName>/<roomName> <baseBranch>`
  - Removing a room: `git worktree remove <path>` then `git branch -D <branch>`

## Terminal Panes

Terminal panes are room-scoped. Each room can have multiple terminal panes. A terminal pane spawns a shell process (PTY) with its `cwd` set to the room's working directory (repo root for default room, worktree path for additional rooms). If no room is selected, the terminal area is empty and pane/tab creation is blocked.

When a pane's process exits, it shows a distinct "exited" state with exit code. User can press `Enter` to restart or `x` (in Pane mode) to close.

## Presets

Two built-in presets, extensible via config. Environment variables in `command` and `args` are expanded at PTY spawn time.

```toml
# ~/.humu/config.toml

[presets.claude]
command = "claude"
args = []

[presets.shell]
command = "$SHELL"
args = []
```

## Layout Persistence

Tab and pane layout is saved per room in `state.toml` and restored on room switch or restart. The layout is a tree structure: each tab contains a `SplitNode` that is either a `Leaf` (single pane with preset) or a `Split` (binary split with direction, ratio, and children).

## Claude Hook Integration

1. Humu runs a Unix socket server at `~/.humu/humu.sock`
2. When launching a Claude preset, humu sets environment variables: `HUMU_SOCKET`, `HUMU_WORKSPACE`, `HUMU_ROOM`
3. Claude Code hooks run a script that sends JSON events to the socket
4. Humu receives events and drives spinner indicators (ON on `PreToolUse`, OFF on `Stop` or 10s timeout)

Spinners appear on: room name in RoomPanel, workspace name in WorkspacePanel, tab label in TerminalArea.

## Theme

GitHub Dark color palette (`#0d1117` base). Powerline-style separators in tab bar and status bar (Nerd Font). Rounded borders on all panels and panes. Configurable via `[ui]` section:

```toml
[ui]
simplified_ui = false    # true = plain separators instead of Powerline
rounded_corners = true
```

`Palette` and `UiConfig` are passed by reference from `App` to all widgets. No global state.
