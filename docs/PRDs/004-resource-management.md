# Resource Management

CRUD lifecycle for workspaces and rooms.

## Workspace

A workspace maps 1:1 to a git repository.

### Create

Three modes:

1. **Clone remote**: User provides a target directory + git URL (HTTP or SSH).
2. **Existing repo**: User provides a path to an existing local git directory.
3. **New project**: User provides a target directory → `git init`.

Workspace name is derived from the repo directory name.

### Select

- Click a workspace in `WorkspacePanel`. The last-selected room for that workspace is restored.
- On startup, the last active workspace and room are restored.

### Delete

- Removes the workspace from humu. Does **not** delete the git repository on disk.
- Cascade: All associated rooms (worktrees) under `~/.humu/worktrees/<workspaceName>/` are removed.

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

- Click a room in `RoomPanel`. Terminal panes switch to the selected room's working directory.

### Delete

- Removes the git worktree and its local branch: `git worktree remove <path>` then `git branch -D <branch>`
- The default room cannot be deleted.
