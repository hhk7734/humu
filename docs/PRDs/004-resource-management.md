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
active_workspace = "humu"
active_room = "feat/auth"

[workspaces.humu]
path = "/home/user/github/humu"

[workspaces.infra]
path = "/home/user/github/infra"

[layout."humu"."feat/auth"]
active_tab = 0
tabs = [
  { name = "claude", split = { direction = "vertical", ratio = 0.5, children = [
    { preset = "claude" },
    { preset = "shell" },
  ]}},
  { name = "shell", split = { preset = "shell" }},
]
```
