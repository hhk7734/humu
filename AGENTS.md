# Humu

TUI-based multi-task manager built on git concepts. Workspaces = repos, rooms = worktrees, terminal panes = PTY processes running presets.

## Design Documents

Design documents live in `docs/PRDs/`. When any behavioral or architectural change is made, update the relevant PRD to keep documentation in sync with the implementation.

## Path Ownership

Treat `HUMU_DIR` and `HOME` as separate roots and do not mix them when implementing paths.

- Use `HUMU_DIR` for humu-owned data and runtime files such as `state.yaml`, `config.yaml`, `hooks/`, `server.sock`, `port`, and `projects/`.
- Use `HOME` only for external user-home conventions or other tools' state such as `~/.codex/...`.
- Do not reconstruct humu paths from `HOME` (for example `HOME/.humu/...`) when `HUMU_DIR` exists; always resolve humu-owned paths from `humu_dir()`.
- Tests that exercise path behavior should keep `HOME` and `HUMU_DIR` distinct so incorrect path usage is detectable.

## Release Checklist

When tagging a new version:
1. Update `version` in `Cargo.toml`
2. Commit as `chore: bump version to vX.Y.Z`
3. Tag, push, and create GitHub release with handwritten notes (compare `git log <prev-tag>..HEAD` to summarize features, fixes, and breaking changes — do NOT use `--generate-notes`)

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
├── config.rs        # HumuConfig, HumuState, WorkspaceEntry, RoomEntry, SplitNode
├── log.rs           # File-based logger (~/.humu/humu.log, 1MB rotation)
├── preset.rs        # Preset resolution and env var expansion
├── lib.rs           # Public module exports
├── git/
│   ├── workspace.rs # Workspace CRUD (clone/init/register/delete)
│   └── room.rs      # Room CRUD (worktree add/remove/list)
├── pty/
│   └── pane.rs      # PTY spawn, background reader thread, vt100 emulation
├── id.rs            # Typed IDs (WorkspaceId, RoomId, TabId, PaneId)
├── explorer/
│   ├── mod.rs       # ExplorerState, FileEntry, tree scan/toggle operations
│   └── icons.rs     # Nerd Font file extension icon lookup with per-type colors
├── notification/
│   ├── mod.rs       # NotificationManager, NotificationEvent, Channel<T>
│   ├── crypto.rs    # AES-256-GCM encrypt/decrypt with machine-derived key
│   ├── os.rs        # OsNotifier (notify-send) + SoundNotifier (paplay)
│   └── telegram.rs  # TelegramNotifier (Bot API via ureq)
├── hook/
│   └── http.rs      # HTTP hook server (axum) for AI agent events (Claude, Gemini)
└── tui/
    ├── mod.rs
    ├── input.rs       # Modal keybinding dispatch (Terminal/Locked/Pane/Tab/Workspace/Room/Explorer/Search)
    ├── search.rs      # SearchState, SearchMatch, regex engine, scrollback text extraction
    ├── completion.rs  # Fuzzy path autocomplete with cross-segment matching
    ├── layout.rs      # SplitTree, TabContainer, PaneId
    ├── theme.rs       # Palette (GitHub Dark), UiConfig, BorderChars, TabChars
    └── widgets/
        ├── status_bar.rs
        ├── tab_bar.rs
        ├── terminal_widget.rs
        ├── workspace_panel.rs
        ├── room_panel.rs
        ├── explorer_panel.rs
        ├── preset_selector.rs
        └── dialog.rs
```
