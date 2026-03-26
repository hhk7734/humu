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
- `humu` without arguments targets a default session named `default`
- `humu attach <session-name>` attaches to an existing named session or creates it if missing
- `humu list-sessions` prints known sessions and whether they are attached or detached
- `humu attach` with no name attaches to `default`
- Closing the client detaches from the session but leaves tasks running
- Re-running `humu` later reattaches to the existing session
- A server may manage multiple named sessions
- Each session allows exactly one active client attachment in v1
- If attach is rejected because the session is already attached, the CLI prints the owning client PID and offers `humu detach <session-name> --force` as the recovery path for stale attachments

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

Task 4 implementation status:

- `humu server` starts the daemon shell, writes `server.json`, binds `server.sock`, answers `Ping`, maintains the startup lock under `server.lock`, and `humu server --daemon` now launches that shell in a background child and returns after readiness
- Attached sessions are released both on explicit `Detach` and on socket disconnect, so Task 4 no longer wedges a session lock after the client goes away
- `humu attach` remains a reachable fallback to the current in-process TUI until the real attach client lands, but the fallback path only supports the default session and rejects named-session requests explicitly
- `humu list-sessions` and `humu detach --force` already exercise daemon discovery/version checks, attached sessions surface owning PID metadata when available, and daemon startup readiness now requires a matching protocol version rather than any responding socket
- The default `humu` command still runs the current in-process `App::new()?.run()` path until the attachable client is ready

Task 5 implementation status:

- The daemon now starts a server-owned `SessionRuntime` before advertising readiness, so hook-server lifecycle no longer depends on the foreground `App`
- `~/.humu/port` publication remains daemon-owned and keeps the existing hook-script contract unchanged
- Session notification focus is now tracked on the server with detached sessions treated as unfocused, preserving `only_unfocused` delivery semantics while no client is attached
- Hook events and Codex polling now live under the daemon runtime boundary so detached-session agent-state updates can continue without a client event loop

## Session Model

- The daemon manages multiple named sessions
- A session owns all runtime state needed to keep work alive after client detach
- Workspaces and room registries remain global, shared machine state
- Session-local state is scoped separately from the global workspace registry
- Persisted room layouts are session-owned state keyed by room ID
- `RoomEntry.tabs` and `RoomEntry.active_tab` remain legacy migration fields only and are cleared on save
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
- Notifications and focus-aware notification decisions
- Search index source data derived from terminal screens
- State save/load
- Session attach lock
- Workspace and room CRUD operations
- Room suspension and hot-restore behavior
- Explorer tree root selection for the active room
- Search match computation against server-owned terminal buffers

### Client-owned

- Terminal raw mode
- Keyboard and mouse event capture
- TUI rendering
- Local terminal size observation
- Attach and detach UX
- Modal and popup presentation state
- Explorer panel scroll position and local selection cursor
- Search mode cursor and current match navigation intent

The client must not own any state required to keep a task alive after detach.

### Persistence migration

- The singleton pre-session layout format is migrated into a `default` session record during load
- The `default` session becomes the immediate source of truth for current `App` layout restore and save paths before the daemon/client split lands
- Named sessions persist independent room selection and layout maps without reusing workspace-room registry fields
- Deleting a room or workspace, or pruning stale worktrees, removes the corresponding session layout entries and clears stale session selection pointers

### Shared model layer

The following types remain shared between client and server:

- IDs
- workspace and room registry structs
- split layout structs
- theme-independent render snapshot structs
- IPC protocol enums

## IPC Design

Use a Unix domain socket at `~/.humu/server.sock`.

### Wire framing

The socket protocol is frame-based rather than raw concatenated JSON. Each message is sent as a 4-byte big-endian payload length followed by one compact JSON document so the client and server can safely decode back-to-back messages from a long-lived stream.

### Control Flow

1. Client checks for server socket
2. If missing, client launches `humu server --daemon`
3. Client waits for readiness
4. Client requests session listing or creation
5. Client attaches to one session
6. Server streams updates until detach or disconnect

### Discovery and liveness

- Daemon endpoint: `~/.humu/server.sock`
- Daemon metadata file: `~/.humu/server.json`
- Metadata contains daemon PID, server start timestamp, socket path, and protocol version
- Client first loads `server.json`, then attempts a `Ping` request on `server.sock`
- If the socket exists but `Ping` fails, the client treats it as stale, removes only the stale socket and metadata after verifying the recorded PID is not alive, then retries auto-launch
- Daemon startup writes metadata only after the socket is bound and ready to answer `Ping`
- Task 4 daemon shell performs the same stale-socket cleanup before binding and refuses attach shell commands when the live `Ping` protocol version does not match the client build
- Hook port publication in `~/.humu/port` remains separate and continues to advertise only the hook HTTP server port

### Auto-launch race handling

- Daemon launch uses a lock file under `~/.humu/server.lock`
- Competing clients attempting auto-launch wait briefly and then retry discovery
- `CreateSession` is idempotent by session name
- `AttachSession` either succeeds for an unattached session or returns `AlreadyAttached`

### Request Types

- `Ping`
- `ListSessions`
- `CreateSession`
- `AttachSession`
- `Detach`
- `ForceDetachSession`
- `SendInput`
- `ResizeSession`
- `RunAction`
- `SubscribeUpdates`
- `FocusChanged`

### Server Responses

- `Pong` with protocol version
- `Sessions` with machine-readable session summaries
- `SessionCreated`
- `Attached` with the initial `FullSnapshot`
- `Detached`
- `Subscribed`
- `Ack`
- `AlreadyAttached` with session name, owning PID, and attach timestamp when known
- `VersionMismatch`
- `Error`

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

### Snapshot contract

`FullSnapshot` must include:

- session name
- active workspace ID and room ID
- tab list and active tab index
- split tree with pane IDs and per-pane geometry
- session geometry and per-pane runtime state
- focused pane ID and fullscreen pane ID
- per-pane terminal screen snapshot
- per-cell text plus fg/bg/style attributes needed by the renderer:
  - bold
  - dim
  - italic
  - underline
  - inverse
  - hidden
  - strike
- per-pane preset name
- per-pane terminal capability state needed for input routing:
  - alternate-screen
  - bracketed-paste
  - mouse protocol mode and encoding
  - scrollback offset
- agent state summary by pane
- explorer root path for the active room

Incremental events may update only the changed subset, but they must preserve the same schema boundaries as the full snapshot.

`LayoutUpdated` therefore includes updated pane geometry keyed by pane ID in addition to tabs, split tree, and focus/fullscreen metadata.

The shared render layer therefore includes explicit tab, split-tree, pane geometry, pane runtime-state, terminal capability, agent summary, and session metadata structs so client and server can exchange snapshots without depending on live PTY/parser objects.

## Resize Policy

Session terminal geometry is singular. PTYs and terminal emulators have one authoritative `(cols, rows)` pair per pane/session view.

For v1:

- The attached client is the authoritative source of terminal size
- When the client detaches, the last known size remains in effect
- When a new client attaches, the session is resized to the new client size
- Only one active client may attach, preventing conflicting resize streams

This deliberately avoids the complexity of multi-client geometry arbitration.

## Persistence Model

The current `state.yaml` schema cannot support multiple named sessions because it stores only one global `active_workspace_id`, one global `active_room_id`, and per-room layout fields that would be overwritten by multiple sessions. The server/client split therefore requires a schema change.

### Global vs session-scoped state

- `workspaces`, room registry data, and panel widths remain global machine state
- session selection, active room/workspace, runtime layout, focus/fullscreen, and pane/session metadata become session-scoped state

### New on-disk structure

Keep `state.yaml` as the top-level file, but extend it with an explicit session registry:

```yaml
active_workspace_id: <legacy/global optional during migration>
active_room_id: <legacy/global optional during migration>
workspaces: [...]
panel_widths: [25, 25]
sessions:
  - name: default
    active_workspace_id: <uuid>
    active_room_id: <uuid>
    last_size: { cols: 180, rows: 48 }
    attached: false
    tabs_by_room:
      "<room-uuid>":
        active_tab: 0
        tabs: [...]
```

`tabs_by_room` becomes the session-scoped replacement for using `RoomEntry.tabs` and `RoomEntry.active_tab` as the only persisted layout source.

### Migration

On first startup with the new daemon:

1. Load the legacy singleton `state.yaml`
2. Create a `default` session entry
3. Copy legacy `active_workspace_id` and `active_room_id` into that `default` session
4. Copy each room's legacy `tabs` and `active_tab` into `default.tabs_by_room`
5. Preserve existing workspace and room registry data unchanged
6. Mark the file as migrated and stop writing session-owned runtime layout back into legacy singleton fields

This preserves existing users' layouts while making future session data non-conflicting.

### Preserved across client detach

- Live PTY processes
- In-memory terminal emulator state
- Session runtime state
- Hook server and Codex tracker activity

### Preserved across server restart

- Workspace and room metadata
- Session-scoped persisted split layout
- Session-scoped active workspace and room IDs
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

## Notification Ownership

Notification delivery remains server-owned so background sessions can still notify while no client is attached.

### Focus semantics

- The attached client sends explicit focus updates derived from crossterm focus events
- Each session tracks `client_focus_state`
- If no client is attached, the session is treated as unfocused
- `only_unfocused: true` channels therefore fire while detached

This preserves PRD 006 semantics while extending them to detached sessions.

### Event resolution

- Agent state transitions are computed on the server
- Notification routing decisions are computed on the server using the session's current focus state
- The client does not emit notifications directly

## Attach UX

### Default behavior

- `humu` and `humu attach` target the `default` session
- Existing users who do nothing continue to get one persistent session, now backed by the daemon

### Named sessions

- `humu attach <name>` attaches to or creates a named session
- `humu list-sessions` shows all known sessions with attachment state
- `humu detach <name> --force` clears a stale active-client lock when the previous client is wedged

### Rejected attach

If a session is already attached, the server returns:

- session name
- attached client PID if known
- attach timestamp if known

The client prints this information and exits non-zero.

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
- Move workspace/room CRUD into server commands so all persistent mutations happen in one process
- Keep explorer widget rendering, search UI state, and dialog interaction client-side
- Move search text extraction and terminal-backed match calculation server-side because terminal buffers are server-owned

## Floating Pane Policy

Floating editor and diff panes are not core long-lived task panes. For v1 they should remain client-local and non-persistent across detach. This keeps the first server split focused on primary room session panes.

Implementation rule:

- editor and diff popups stop using the session `PaneId` map
- they are spawned as separate client-only PTYs owned by the attached client process
- detaching or closing the client terminates these popups immediately
- server-side pane cleanup logic ignores them because they never enter session runtime state

If needed later, they can be promoted to server-owned panes with explicit persistence semantics.

## Failure Handling

- Stale socket file: client attempts connection, removes or ignores stale metadata only after failed liveness checks
- Duplicate attach: server rejects attach when the session already has an active client
- Unexpected client disconnect: server detaches the client and keeps the session alive
- Server crash: next startup cold-restores persisted sessions and layouts
- Attach/create races: server serializes session creation by name and never creates duplicate session entries
- Version mismatch: client refuses attach when the daemon protocol version differs

## Acceptance Criteria

The implementation is complete only when all of the following are true:

- Closing an attached client does not terminate PTY child processes in that session
- Reattaching to the same session restores visible terminal output without respawning panes while the daemon stayed alive
- Restarting the daemon cold-restores the session from persisted layout and respawns panes
- Two named sessions keep independent active workspace/room selection and room layouts on disk
- A rejected second attach leaves the first attachment intact and returns machine-readable `AlreadyAttached` data
- Stale `server.sock` and `server.json` are recovered automatically without manual cleanup
- Hook-driven agent state and Codex-driven state continue updating while the session is detached
- `only_unfocused` notifications remain suppressed while a focused client is attached and fire while the session is detached
- Reattaching from a different terminal size resizes the session to the new client geometry
- Floating editor and diff panes terminate on client detach and do not become server-owned session panes

## Testing Strategy

- Unit tests for IPC message encoding and session registry rules
- Shared integration harnesses in `tests/support/mod.rs` provide isolated `HUMU_DIR` homes, preconfigured humu command builders/spawners, and PTY fixtures so daemon/client tests do not share machine state
- Integration tests for daemon auto-launch and attach handshake
- Integration tests proving client exit does not terminate PTY child processes
- Integration tests for detach and reattach with preserved terminal output
- Regression tests for server-restart cold restore behavior
- Migration tests from legacy singleton `state.yaml` into the new session-scoped schema
- Integration tests proving two named sessions persist independent active room/layout state
- Notification tests covering focused, unfocused, and detached session behavior
- Integration tests proving hook and Codex state continue updating while the client is detached
- Regression tests proving floating editor/diff panes are client-local and do not survive detach

## Rollout Plan

1. Extract session runtime state out of `App`
2. Introduce an in-process server abstraction
3. Replace the in-process boundary with Unix socket IPC
4. Add daemon auto-launch and attach flow
5. Add session registry and one-client attach enforcement
6. Update architecture documentation to reflect the new steady state
