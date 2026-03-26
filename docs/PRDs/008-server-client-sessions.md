# PRD 008: Server/Client Sessions

## Goal

Split humu into a background server and an attachable client so closing the client does not stop running tasks. The design should follow Zellij's session model as closely as practical while keeping the first version constrained to one active client per session.

## Motivation

Today humu is a single process. The TUI owns PTYs, room runtime state, the hook HTTP server, and Codex tracking. When the process exits, all in-memory runtime state is dropped. Room switching already proves that humu can preserve live PTYs inside one process by suspending room runtime state, but that preservation ends when the app process exits.

The new architecture moves long-lived runtime ownership into a daemon process and makes the TUI a detachable client.

## Non-Goals

- Multi-client attachment to the same session in v1
- Cross-server PTY checkpoint/restore
- Windows-first transport support
- Full protocol compatibility with Zellij internals

## Product Model

### Terminology

- **Server**: Long-lived background daemon process
- **Client**: TUI process that attaches to one session
- **Session**: Named runtime container managed by the server

### User Experience

- Running `humu` starts or reuses the background server automatically
- The client attaches to a named session
- If the session does not exist, the client can create it and attach
- Closing the client detaches from the session but leaves tasks running
- Re-running `humu` later reattaches to the existing session
- A server may manage multiple named sessions
- Each session allows exactly one active client attachment in v1

## Approaches Considered

### 1. Single background daemon with named sessions

One daemon owns all sessions. Clients connect over local IPC and attach to one session.

Pros:
- Closest match to Zellij's session model
- Centralized ownership for PTYs, hooks, and state persistence
- Straightforward detach and reattach story

Cons:
- Requires a new IPC protocol
- Requires splitting current `App` responsibilities

### 2. One server process per session

Each session has its own server process.

Pros:
- Strong process isolation
- Easier per-session debugging

Cons:
- Extra process discovery and lifecycle complexity
- A higher-level registry is still needed
- Further from Zellij's user model

### 3. Headless mode reuse of current `App`

Keep `App` mostly intact and run it in headless or attached modes.

Pros:
- Smaller first refactor

Cons:
- Keeps server and client responsibilities tangled
- Makes later protocol and rendering cleanup harder

## Decision

Use approach 1: a single background daemon that manages multiple named sessions.

## Process Architecture

```
humu client
    |
    | Unix domain socket
    v
humu server daemon
├── SessionManager
├── Session runtime(s)
│   ├── PTY panes
│   ├── Terminal emulators
│   ├── Layout state
│   ├── Room/workspace runtime state
│   ├── Agent state
│   └── Notifications
├── Hook HTTP server
└── Codex tracker
```

### Executables

- `humu`: client entry point
- `humu server`: internal and debug entry point for the daemon

The default `humu` command performs server discovery and auto-launch before attaching.

## Session Model

- The daemon manages multiple named sessions
- A session owns all runtime state needed to keep work alive after client detach
- Session state includes:
  - active workspace and room
  - tabs and split layout
  - live panes and their PTYs
  - pane preset metadata
  - agent/session identifiers
  - focused pane and fullscreen state
- v1 enforces at most one attached client per session

## Ownership Boundary

### Server-owned

- PTY spawn, input, output, resize, exit detection
- Terminal emulation state
- Workspace and room runtime state
- Session registry
- Hook HTTP server
- Codex tracking
- State save/load
- Session attach lock

### Client-owned

- Terminal raw mode
- Keyboard and mouse event capture
- TUI rendering
- Local terminal size observation
- Attach and detach UX

The client must not own any state required to keep a task alive after detach.

## IPC Design

Use a Unix domain socket at `~/.humu/server.sock`.

### Control Flow

1. Client checks for server socket
2. If missing, client launches `humu server --daemon`
3. Client waits for readiness
4. Client requests session listing or creation
5. Client attaches to one session
6. Server streams updates until detach or disconnect

### Request Types

- `ListSessions`
- `CreateSession`
- `AttachSession`
- `Detach`
- `SendInput`
- `ResizeSession`
- `RunAction`
- `SubscribeUpdates`

### Server Events

- `FullSnapshot`
- `PaneUpdated`
- `LayoutUpdated`
- `AgentStateUpdated`
- `SessionMetadataUpdated`
- `Error`
- `Detached`

### Snapshot Strategy

The server performs PTY reads and terminal emulation. Clients receive renderable snapshots or incremental updates derived from server-owned screen state.

This avoids reintroducing lifetime coupling between client detach and parser state ownership.

## Resize Policy

Session terminal geometry is singular. PTYs and terminal emulators have one authoritative `(cols, rows)` pair per pane/session view.

For v1:

- The attached client is the authoritative source of terminal size
- When the client detaches, the last known size remains in effect
- When a new client attaches, the session is resized to the new client size
- Only one active client may attach, preventing conflicting resize streams

This deliberately avoids the complexity of multi-client geometry arbitration.

## Persistence Model

### Preserved across client detach

- Live PTY processes
- In-memory terminal emulator state
- Session runtime state
- Hook server and Codex tracker activity

### Preserved across server restart

- Workspace and room metadata
- Persisted split layout
- Active workspace and room IDs
- Stored agent `session_id` values used for preset resume
- Session metadata such as last terminal size

### Not preserved across server restart in v1

- Live PTY process execution
- In-memory terminal emulator buffers

If the daemon restarts, humu falls back to cold restore from persisted layout and respawns panes, matching today's restart behavior.

## Startup And Detach Flows

### Startup and attach

1. `humu` starts
2. Discover or auto-launch daemon
3. Connect to `server.sock`
4. Resolve target session
5. Create session if needed
6. Attach with current terminal size
7. Receive full snapshot
8. Enter interactive render loop

### Detach and reattach

1. Client exits normally or disconnects unexpectedly
2. Server marks the session detached
3. Session runtime continues running
4. A later client attaches to the same session
5. Server resizes the session to the new client size and sends a fresh snapshot

## Integration With Existing Code

The current `App` type mixes client and server responsibilities. The refactor should separate these into explicit modules.

### Proposed modules

```text
src/
├── client/
│   ├── attach.rs
│   ├── state.rs
│   └── tui_app.rs
├── server/
│   ├── daemon.rs
│   ├── ipc.rs
│   ├── runtime.rs
│   └── session.rs
```

### Migration boundary

- Move `PtyPane`, hook integration, Codex tracking, room suspension, and save/load logic behind a server runtime boundary
- Keep ratatui rendering and input decoding in the client
- Share IDs, config/state structs, and layout structs between client and server

## Floating Pane Policy

Floating editor and diff panes are not core long-lived task panes. For v1 they should remain client-local and non-persistent across detach. This keeps the first server split focused on primary room session panes.

If needed later, they can be promoted to server-owned panes with explicit persistence semantics.

## Failure Handling

- Stale socket file: client attempts connection, removes or ignores stale metadata only after failed liveness checks
- Duplicate attach: server rejects attach when the session already has an active client
- Unexpected client disconnect: server detaches the client and keeps the session alive
- Server crash: next startup cold-restores persisted sessions and layouts

## Testing Strategy

- Unit tests for IPC message encoding and session registry rules
- Integration tests for daemon auto-launch and attach handshake
- Integration tests proving client exit does not terminate PTY child processes
- Integration tests for detach and reattach with preserved terminal output
- Regression tests for server-restart cold restore behavior

## Rollout Plan

1. Extract session runtime state out of `App`
2. Introduce an in-process server abstraction
3. Replace the in-process boundary with Unix socket IPC
4. Add daemon auto-launch and attach flow
5. Add session registry and one-client attach enforcement
6. Update architecture documentation to reflect the new steady state
