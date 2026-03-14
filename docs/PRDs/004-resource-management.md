# Resource Management

CRUD lifecycle for workspaces and rooms.

## Workspace

A workspace maps 1:1 to a git repository.

### Create

Three modes:

1. **Clone remote**: User provides a target directory + git URL (HTTP or SSH).
2. **Existing repo**: User provides a path to an existing local git directory.
3. **New project**: User provides a target directory → `git init`.

Workspace name is derived from the repo directory name. Name collisions get a numeric suffix (e.g., `infra-2`). The path field supports fuzzy filesystem autocomplete with cross-segment matching (e.g., `~/githhk` → `~/github/hhk7734/`).

On creation, humu auto-selects the new workspace and its default room (main branch), so the terminal is immediately usable.

### Select

- Click a workspace in `WorkspacePanel` (enters Workspace mode) or use `Ctrl+w`. The last-selected room for that workspace is restored.
- On startup, the last active workspace and room are restored.

### Delete

- Removes the workspace from humu. Prompts user: "Also delete the repo on disk?" If yes, removes the directory.
- Cascade: All associated rooms (worktrees) under `~/.humu/worktrees/<workspaceName>/` are removed either way.

---

## Room

A working context within a workspace.

### Default Room

- Represents the repository's main working directory.
- Always exists when a workspace exists. Cannot be created or deleted manually.

### Create (Additional Room)

- User provides: **branch name** and **base branch**.
- Worktree created at: `~/.humu/worktrees/<workspaceName>/<roomName>`
  - `workspaceName` = repo name
  - `roomName` = branch name
- Maps to: `git worktree add -b <branch> <path> <baseBranch>`

### Select

- Click a room in `RoomPanel` (enters Room mode) or use `Ctrl+r`. Terminal panes switch to the selected room's working directory.
- Terminal panes are room-scoped: if no room is selected, the terminal area is empty and pane/tab creation is blocked.

### Delete

- Removes the git worktree and its local branch: `git worktree remove <path>` then `git branch -D <branch>`
- The default room cannot be deleted.

---

## Configuration

### `~/.humu/config.toml` (user-edited)

```toml
[presets.claude]
command = "claude"
args = []

[presets.shell]
command = "$SHELL"
args = []

[ui]
simplified_ui = false
rounded_corners = true
```

### `~/.humu/state.toml` (auto-managed)

```toml
active_workspace_id = "550e8400-e29b-41d4-a716-446655440000"
active_room_id = "660e8400-e29b-41d4-a716-446655440001"

[workspaces.humu]
id = "550e8400-e29b-41d4-a716-446655440000"
path = "/home/user/github/humu"

[workspaces.humu.rooms.main]
id = "660e8400-e29b-41d4-a716-446655440001"

[workspaces.humu.rooms."feat/auth"]
id = "770e8400-e29b-41d4-a716-446655440002"

[workspaces.infra]
id = "880e8400-e29b-41d4-a716-446655440003"
path = "/home/user/github/infra"

[layout."550e8400-..."]["660e8400-..."]
active_tab = 0

[[layout."550e8400-..."]["660e8400-...".tabs]]
name = "claude"
tree = { preset = "claude", session_id = "abc123-def456" }

[[layout."550e8400-..."]["660e8400-...".tabs]]
name = "shell"
tree = { preset = "shell" }
```

Workspace and room IDs are UUIDs. Room IDs are assigned lazily on first discovery and persisted. On startup, stale room entries (worktrees that no longer exist) are pruned. Layout keys use UUID strings. Old-format `state.toml` (pre-UUID) is discarded on load with a log message.
