# TUI Layout

## Main Layout

```
+──────────────────────────────────────────────────────────────────────────────+
│                  │   │           │   │                                       │
│  WorkspacePanel  │ ⠿ │ RoomPanel │ ⠿ │            Terminal Area              │
│                  │   │           │   │                                       │
│  > my-app        │   │ > main    │   │  ┌─────────────────────────────────┐  │
│    infra         │   │   feat/x  │   │  │  $ cargo build                  │  │
│    docs          │   │   fix/y   │   │  │  Compiling humu v0.1.0          │  │
│                  │   │           │   │  │  Finished dev [unoptimized]     │  │
│                  │   │           │   │  │  $                              │  │
│                  │   │           │   │  ├─────────────────────────────────┤  │
│                  │   │           │   │  │  $ git log --oneline -5         │  │
│                  │   │           │   │  │  abc1234 feat: add router       │  │
│                  │   │           │   │  │  $                              │  │
│                  │   │           │   │  └─────────────────────────────────┘  │
│                  │   │           │   │                                       │
+──────────────────────────────────────────────────────────────────────────────+
```

Three panels separated by draggable `ResizeHandle` widgets (`⠿`):

- **WorkspacePanel**: Lists workspaces (repo names). Selected prefixed with `>`.
- **RoomPanel**: Lists rooms in the selected workspace. Default room (`main`) is always first. Selected prefixed with `>`.
- **Terminal Area**: Contains one or more terminal panes for the selected room. Each pane runs a shell with `cwd` set to the room's working directory. Panes can be split horizontally.
