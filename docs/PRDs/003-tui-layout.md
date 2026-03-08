# TUI Layout

## Main Layout

```
+──────────────────────────────────────── Header ─────────────────────────────────────────+
│                  │   │           │   │                           │   │                  │
│  WorkspacePanel  │⠿  │ RoomPanel │⠿  │         ChatPanel         │⠿  │   AgentPanel     │
│                  │   │           │   │  ┌─────────────────────┐  │   │                  │
│  > my-app    ⠹   │   │ > design  │   │  │   #chat-messages    │  │   │  * leader        │
│    infra         │   │   dev     │   │  │                     │  │   │    opus          │
│    docs          │   │   review  │   │  │  [you] How should   │  │   │    backend       │
│                  │   │           │   │  │  we structure this? │  │   │    sonnet        │
│                  │   │           │   │  │                     │  │   │    security      │
│                  │   │           │   │  │  [leader] Routing…  │  │   │    haiku         │
│                  │   │           │   │  │                     │  │   │                  │
│                  │   │           │   │  │  [backend] I would  │  │   │                  │
│                  │   │           │   │  │  recommend REST…    │  │   │                  │
│                  │   │           │   │  └─────────────────────┘  │   │                  │
│                  │   │           │   │  ┌─────────────────────┐  │   │                  │
│                  │   │           │   │  │   #queue-display    │  │   │                  │
│                  │   │           │   │  │  Queued (1) …       │  │   │                  │
│                  │   │           │   │  ├─────────────────────┤  │   │                  │
│                  │   │           │   │  │     #chat-input     │  │   │                  │
│                  │   │           │   │  │  > your message…    │  │   │                  │
│                  │   │           │   │  └─────────────────────┘  │   │                  │
│                  │   │           │   │  ┌─────────────────────┐  │   │                  │
│                  │   │           │   │  │  #autocomplete      │  │   │                  │
│                  │   │           │   │  │   ❯ src/            │  │   │                  │
│                  │   │           │   │  └─────────────────────┘  │   │                  │
+──────────────────────────────────────── Footer ─────────────────────────────────────────+
```

Four panels separated by draggable `ResizeHandle` widgets (`⠿`):

- **WorkspacePanel**: Lists workspaces. Selected prefixed with `>`, spinner badge on active tasks.
- **RoomPanel**: Lists rooms in the selected workspace. Same selection/spinner behavior.
- **ChatPanel**: Central area with scrollable messages, queue display, multi-line input (`Enter` to submit, `Shift+Enter` for newline), and autocomplete (paths, commands, skills, etc.).
- **AgentPanel**: Lists agents in the selected room. Leader prefixed with `*`. Double-click to edit agent config.
