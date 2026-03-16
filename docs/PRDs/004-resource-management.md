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
- **State preservation**: Switching workspaces suspends the current room's live PTY panes. The target workspace's last-active room is restored with live panes if previously suspended.

### Delete

- Removes the workspace from humu. Prompts user: "Also delete the repo on disk?" If yes, removes the directory.
- Cascade: All associated rooms (worktrees) under `~/.humu/worktrees/<workspaceName>/` are removed either way.
- Cleanup: Any suspended room states for the deleted workspace are discarded.
- If the deleted workspace was active, its live panes are cleared and humu auto-switches to the next available workspace.

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

- Click a room in `RoomPanel` (enters Room mode) or use `Ctrl+r`. Terminal panes switch to the selected room's context.
- Terminal panes are room-scoped: if no room is selected, the terminal area is empty and pane/tab creation is blocked.
- **State preservation**: Switching rooms suspends the current room's live PTY panes rather than killing them. Switching back restores panes instantly with full terminal history. See Architecture > Room Suspension for details.

### Delete

- Removes the git worktree and its local branch: `git worktree remove <path>` then `git branch -D <branch>`
- The default room cannot be deleted.

---

## Data Directory

The humu data directory defaults to `~/.humu/`. Override with the `HUMU_DIR` environment variable:

```bash
HUMU_DIR=/tmp/humu-test humu        # run with isolated data dir
HUMU_DIR=/tmp/humu-test cargo test  # tests against a throwaway directory
```

## Configuration

### `<HUMU_DIR>/config.yaml` (user-edited)

```yaml
presets:
  claude:
    command: claude
    args: []
  shell:
    command: $SHELL
    args: []

ui:
  simplified_ui: false
  rounded_corners: true
```

### `<HUMU_DIR>/state.yaml` (auto-managed)

```yaml
active_workspace_id: 550e8400-e29b-41d4-a716-446655440000
active_room_id: 660e8400-e29b-41d4-a716-446655440001
panel_widths:
  - 20
  - 18
workspaces:
  - name: humu
    id: 550e8400-e29b-41d4-a716-446655440000
    path: /home/user/github/humu
    rooms:
      - name: main
        id: 660e8400-e29b-41d4-a716-446655440001
        active_tab: 0
        tabs:
          - name: claude
            split:
              preset: claude
              session_id: abc123-def456
          - name: shell
            split:
              preset: shell
      - name: feat/auth
        id: 770e8400-e29b-41d4-a716-446655440002
  - name: infra
    id: 880e8400-e29b-41d4-a716-446655440003
    path: /home/user/github/infra
```

Workspaces and rooms are lists with `name` and `id` fields. Room layout (tabs, panes) is stored directly in the room entry. IDs are UUIDs assigned lazily on first discovery. On startup, stale room entries (worktrees that no longer exist) are pruned.
