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
├── Hook Layer (axum HTTP server)
│   └── HTTP server at 127.0.0.1:<random port>
├── Theme Layer
│   ├── Palette (GitHub Dark color scheme)
│   └── UiConfig (simplified_ui, rounded_corners)
├── ID Layer (src/id.rs)
│   └── Typed IDs: WorkspaceId, RoomId, TabId, PaneId
└── State Layer
    └── $HUMU_DIR (default: ~/.humu/)
        ├── config.toml
        ├── state.toml
        ├── port
        └── hooks/
```

## Tech Stack

| Layer              | Choice       |
| ------------------ | ------------ |
| Language           | Rust         |
| TUI framework      | ratatui      |
| Terminal backend   | crossterm    |
| PTY               | portable-pty |
| Terminal emulation | vt100        |
| HTTP server        | axum         |
| ID generation      | uuid         |

## Typed IDs

All entities use explicit ID types via the newtype pattern in `src/id.rs`:

| Entity | Type | Backing | Persistence |
|---|---|---|---|
| Workspace | `WorkspaceId(Uuid)` | UUID v4 | Permanent — stored in `state.toml` |
| Room | `RoomId(Uuid)` | UUID v4 | Permanent — stored in `state.toml` per workspace |
| Tab | `TabId(u64)` | Sequential counter | Session-scoped — reset on restart |
| Pane | `PaneId(u64)` | Sequential counter | Session-scoped — reset on restart |

All four types implement `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `Serialize`, `Deserialize`, and `Display`. `PaneId` is a newtype struct — access the inner value via `.0`.

`RoomId` is assigned lazily on first discovery and persisted. On startup, persisted rooms are compared against git worktrees — stale entries are pruned.

## Rendering Pipeline

```
PTY output → vt100 (parse ANSI/VT) → screen buffer → ratatui cells
User keystrokes → ratatui → PTY input
```

PTY reads run in a background thread with `mpsc::channel`, using `try_recv()` in the main event loop to avoid blocking. Resize events propagate to the PTY via `SIGWINCH`. Each parser is created with 10,000 lines of scrollback (`vt100::Parser::new(rows, cols, 10_000)`), and `Parser::set_scrollback(offset)` shifts the viewport into history. Scrollback auto-resets to live view on new output or keypress.

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

Tab and pane layout is saved per room in `state.toml` and restored on room switch or restart. The layout is a tree structure: each tab contains a `SplitNode` that is either a `Leaf` (single pane with preset and optional `session_id`) or a `Split` (binary split with direction, ratio, and children).

Layout is persisted via **event-driven persistence** — `persist_layout()` is called on every structural mutation (tab add/remove, pane split/close), not on a timer or only at shutdown. This ensures crash safety: if humu is killed unexpectedly, the layout reflects the last structural change. When all tabs are closed, the layout entry is removed from `state.toml` so that a restart creates a fresh default shell instead of restoring stale panes.

Layout keys use UUID strings for workspace and room identification.

### Room Suspension (Hot Restore)

When switching rooms or workspaces, live PTY panes are **suspended** rather than killed. The switch sequence is: (1) resolve the target workspace/room from selection indices, (2) suspend the current room under the **current** active IDs, (3) update active IDs to the target, (4) restore the target room. The room list for the target workspace is resolved independently of `active_workspace_id` to avoid miskeying suspended state during workspace creation.

The runtime state (`RoomState`: panes, tabs, pane_presets, focused_pane, fullscreen_pane) is moved into `suspended_rooms: HashMap<(WorkspaceId, RoomId), RoomState>`. When switching back:

1. **Hot restore**: If the room has suspended state, swap it back in — PTY processes resume instantly with full terminal history intact.
2. **Cold restore**: If no suspended state exists (e.g., after restart), rebuild from the persisted layout in `state.toml`, spawning new PTY processes.
3. **Default**: If no persisted layout exists either, create a single shell tab.

Suspended panes continue running in the background — their reader threads accumulate output in unbounded `mpsc` channels, which is drained on restore. `PaneId` remains globally unique (monotonically increasing `next_pane_id` is never saved/restored per room). `agent_states` is global since hook events can arrive for any pane.

On graceful shutdown, all suspended rooms have their layouts persisted to `state.toml` before PTY processes are dropped.

When a workspace is deleted, all its entries in `suspended_rooms` are discarded. If the deleted workspace was active, live panes are cleared and humu auto-switches to the next available workspace.

## Claude Hook Integration

### HTTP Hook Server

1. Humu starts an axum HTTP server bound to `127.0.0.1:0` (OS-assigned port)
2. The allocated port is written to `~/.humu/port` for external discovery
3. On clean exit (`Drop` impl), the port file is removed
4. On crash, the stale port file is harmless — next startup overwrites it

### Hook Auto-Configuration

On startup, humu generates two files:

- **`~/.humu/hooks/notify.sh`** — hook script using `curl` and `grep` (no `jq`/`socat`)
- **`~/.humu/hooks/claude-settings.json`** — Claude Code settings with hook registration for: `UserPromptSubmit`, `Stop`, `PostToolUse`, `PostToolUseFailure`, `PermissionRequest`

### Claude Preset Spawning

When spawning a claude preset, humu:

1. Passes env vars: `HUMU_PORT`, `HUMU_WORKSPACE_ID`, `HUMU_ROOM_ID`, `HUMU_TAB_ID`, `HUMU_PANE_ID`
2. Appends `--settings ~/.humu/hooks/claude-settings.json` to the command
3. If restoring with `session_id`, appends `--resume SESSION_ID`

### Event Processing

Hook events are normalized to three canonical agent states:

| Raw Event | Canonical State |
|---|---|
| `UserPromptSubmit` | Working |
| `PostToolUse` | Working |
| `PostToolUseFailure` | Working |
| `PermissionRequest` | NeedsInput |
| `Stop` | Idle |
| Unknown | Ignored (forward compatible) |

Per-pane agent state is tracked in `HashMap<PaneId, AgentStateEntry>`. State is cleared when the pane process exits or is closed.

### Derived UI State

Workspace/room panel spinners are derived from pane states:
- Show animated spinner if any pane in that workspace/room is `Working`
- Show `⚠` if any pane is `NeedsInput` and none are `Working`
- Show nothing if all panes are `Idle` or no agent panes exist

### Session Resumption

1. Claude Code includes `session_id` in hook event payloads
2. Humu stores `session_id` in `AgentStateEntry` keyed by `PaneId`
3. `session_id` is persisted to `SplitNode::Leaf` on state save
4. On restore, `--resume SESSION_ID` is passed to Claude Code

## Theme

GitHub Dark color palette (`#0d1117` base). Powerline-style separators in tab bar and status bar (Nerd Font). Rounded borders on all panels and panes. Configurable via `[ui]` section:

```toml
[ui]
simplified_ui = false    # true = plain separators instead of Powerline
rounded_corners = true
```

`Palette` and `UiConfig` are passed by reference from `App` to all widgets. No global state.
