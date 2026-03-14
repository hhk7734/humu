# Humu

TUI-based multi-task manager built on git concepts. Workspaces = repos, rooms = worktrees, terminal panes = PTY processes running presets.

## Design Documents

Design documents live in `docs/PRDs/`. When any behavioral or architectural change is made, update the relevant PRD to keep documentation in sync with the implementation.

## Build & Test

```bash
cargo build          # Build
cargo test           # Run all tests
```

## Project Structure

```
src/
├── app.rs           # Main App struct, event loop, all action handling
├── main.rs          # Entry point
├── config.rs        # HumuConfig, HumuState, RoomLayout, SplitNode
├── preset.rs        # Preset resolution and env var expansion
├── lib.rs           # Public module exports
├── git/
│   ├── workspace.rs # Workspace CRUD (clone/init/register/delete)
│   └── room.rs      # Room CRUD (worktree add/remove/list)
├── pty/
│   └── pane.rs      # PTY spawn, background reader thread, vt100 emulation
├── id.rs            # Typed IDs (WorkspaceId, RoomId, TabId, PaneId)
├── hook/
│   └── http.rs      # HTTP hook server (axum) for Claude Code events
└── tui/
    ├── mod.rs
    ├── input.rs       # Modal keybinding dispatch (Terminal/Locked/Pane/Tab/Workspace/Room)
    ├── completion.rs  # Fuzzy path autocomplete with cross-segment matching
    ├── layout.rs      # SplitTree, TabContainer, PaneId
    ├── theme.rs       # Palette (GitHub Dark), UiConfig, BorderChars, TabChars
    └── widgets/
        ├── status_bar.rs
        ├── tab_bar.rs
        ├── terminal_widget.rs
        ├── workspace_panel.rs
        ├── room_panel.rs
        ├── preset_selector.rs
        └── dialog.rs
```
