# TUI Layout

## Main Layout

```
┌──────────────┬───────────┬──────────────────────────────────────────┐
│              │           │ [claude ⠋] [shell] [+]                  │
│  WORKSPACES  │   ROOMS   │ ┌──────────────────────────────────────┐ │
│              │           │ │ $ claude                             │ │
│  ▸ humu ⠋   │   main    │ │ ⏵ Claude is working...              │ │
│    infra     │ ▸ feat/x ⠋│ │                                     │ │
│    docs      │   fix/y   │ ├──────────────────────────────────────┤ │
│              │           │ │ $ cargo test                         │ │
│              │           │ │ running 12 tests ... ok              │ │
│              │           │ │ $  ▋                                 │ │
│              │           │ └──────────────────────────────────────┘ │
├──────────────┴───────────┴──────────────────────────────────────────┤
│ Ctrl+g LOCK │ Ctrl+p PANE │ Ctrl+t TAB │ Ctrl+w WORKSPACE │ ...   │
└─────────────────────────────────────────────────────────────────────┘
```

Three panels separated by draggable resize handles, plus a status bar:

- **WorkspacePanel**: Lists workspaces (repo names). Selected prefixed with `▸`. Spinner `⠋` when Claude is active.
- **RoomPanel**: Lists rooms in the selected workspace. Default room always first. Spinner `⠋` when Claude is active.
- **Terminal Area**: Tab bar at top with `+` button for new tabs. Each tab contains one or more split panes (vertical/horizontal). Panes run presets (Claude, shell, etc.) with `cwd` set to the room's working directory.
- **StatusBar**: Bottom line showing available keybindings for the current mode.
