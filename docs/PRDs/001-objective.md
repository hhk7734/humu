# Objective

> This document covers only the minimal objective. Detailed requirements, design decisions, and implementation specifics are intentionally excluded.

## Core Concepts

- **Workspace**: A 1:1 mapping to a git repository. Created by cloning a remote repo, pointing to an existing local repo, or initializing a new project.
- **Room**: A working context within a workspace. The **default room** is the repository itself (main working directory). Additional rooms are git worktrees, each on its own branch.
- **Terminal Pane**: An embedded terminal within a room, spawned from a preset. Terminal panes are room-scoped: if no room is selected, the terminal area is empty and pane/tab creation is blocked.

## Interface

TUI-based. Implemented in Rust.

## Directory Structure

```
~/.humu/
├── config.toml
├── state.toml
├── port                      # HTTP hook server port (auto-managed)
├── hooks/
│   ├── notify.sh             # Hook script (auto-generated)
│   └── claude-settings.json  # Claude Code settings (auto-generated)
└── worktrees/
    └── <workspaceName>/      # workspaceName = repo name
        └── <roomName>/       # roomName = branch name (git worktree)
```
