# Objective

> This document covers only the minimal objective. Detailed requirements, design decisions, and implementation specifics are intentionally excluded.

## Core Concepts

- **Workspace**: A 1:1 mapping to a git repository. Created by cloning a remote repo, pointing to an existing local repo, or initializing a new project.
- **Room**: A working context within a workspace. The **default room** is the repository itself (main working directory). Additional rooms are git worktrees, each on its own branch.
- **Terminal Pane**: An embedded terminal within a room. Users can add multiple terminal panes per room, each running in the room's working directory.

## Interface

TUI-based, designed to be fully usable over SSH for remote work. Implemented in Rust.

## Directory Structure

```
~/.humu/
└── worktrees/
    └── <workspaceName>/      # workspaceName = repo name
        └── <roomName>/       # roomName = branch name (git worktree)
```
