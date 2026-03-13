# Architecture

## Tech Stack

| Layer     | Choice |
| --------- | ------ |
| Language  | Rust   |

## Workspace Management

Three creation modes:

1. **Clone remote** — user provides a directory path + git URL (HTTP or SSH) → `git clone <url> <path>`
2. **Existing repo** — user points to an existing local git directory
3. **New project** — user provides a directory path → `git init <path>`

## Room Management

- **Default room**: The repository's main working directory. Always exists, cannot be deleted.
- **Additional rooms**: Created as git worktrees branching from a user-specified base branch.
  - `git worktree add -b <branch> ~/.humu/worktrees/<workspaceName>/<roomName> <baseBranch>`
  - Removing a room: `git worktree remove <path>`

## Terminal Panes

Each room can have multiple terminal panes. A terminal pane spawns a shell process (PTY) with its `cwd` set to the room's working directory (repo root for default room, worktree path for additional rooms).
