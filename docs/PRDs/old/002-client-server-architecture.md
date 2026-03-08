# Humu — Client-Server Architecture

## Problem

Currently, Humu runs as a single monolithic TUI process. `HumuApp` directly instantiates `Storage`, `AgentRunner`, and `Router`. If a user opens two terminal windows and runs `humu` in both, they are completely independent — messages sent in one instance are invisible to the other.

## Goal

Separate Humu into a **backend server** and a **frontend client** so that multiple TUI instances connected to the same server see the same chat messages in real time.

## Architecture Overview

```
┌──────────────┐      WebSocket       ┌─────────────────────────┐
│  humu (TUI)  │ ◄──────────────────► │                         │
└──────────────┘                      │      humu-server        │
                                      │                         │
┌──────────────┐      WebSocket       │  ┌────────┐ ┌────────┐  │
│  humu (TUI)  │ ◄──────────────────► │  │Storage │ │ Router │  │
└──────────────┘                      │  └────────┘ └────────┘  │
                                      │  ┌─────────────────┐    │
                                      │  │  AgentRunner    │    │
                                      │  └─────────────────┘    │
                                      └─────────────────────────┘
```

### Backend (Server)

- Started with `humu serve` (or auto-started on first `humu` if not running)
- Owns all state: `Storage`, `AgentRunner`, `Router`
- Listens on a Unix domain socket (`~/.humu/humu.sock`) for local connections
- Broadcasts events to all connected clients
- Manages agent sessions, processing queues, and chat history

### Frontend (Client / TUI)

- Started with `humu` (default command)
- Pure UI — no direct access to `Storage`, `AgentRunner`, or `Router`
- Connects to the backend via WebSocket over Unix socket
- Sends **commands** to the server
- Receives **events** from the server and renders them

## Communication Protocol

JSON messages over WebSocket. Each message has a `type` field.

### Client → Server (Commands)

| Command             | Fields                              | Description                               |
| :------------------ | :---------------------------------- | :---------------------------------------- |
| `submit_message`    | `workspace`, `room`, `text`         | Send a chat message                       |
| `cancel_processing` | `workspace`, `room`                 | Cancel active task                        |
| `create_workspace`  | `name`, `root_path`                 | Create a workspace                        |
| `delete_workspace`  | `name`                              | Delete a workspace                        |
| `create_room`       | `workspace`, `room_name`            | Create a room (auto-creates leader)       |
| `delete_room`       | `workspace`, `room_name`            | Delete a room                             |
| `invite_agent`      | `workspace`, `room`, `agent_name`   | Add agent to room                         |
| `kick_agent`        | `workspace`, `room`, `agent_name`   | Remove agent from room                    |
| `create_agent`      | `agent_config`                      | Create or update an agent                 |
| `list_workspaces`   |                                     | Request workspace list                    |
| `list_rooms`        | `workspace`                         | Request room list                         |
| `list_agents`       |                                     | Request all agents                        |
| `get_chat_history`  | `workspace`, `room`                 | Request chat history                      |
| `get_skills`        |                                     | Request skill list                        |
| `subscribe_room`    | `workspace`, `room`                 | Subscribe to real-time updates for a room |
| `unsubscribe_room`  | `workspace`, `room`                 | Unsubscribe from room updates             |

### Server → Client (Events)

| Event                  | Fields                                                                            | Description                             |
| :--------------------- | :-------------------------------------------------------------------------------- | :-------------------------------------- |
| `message_added`        | `workspace`, `room`, `sender`, `text`, `is_system`, `raw`, `steps`                | New chat message                        |
| `stream_chunk`         | `workspace`, `room`, `sender`, `text`                                             | Streaming text chunk                    |
| `processing_started`   | `workspace`, `room`, `sender`                                                     | Agent started processing (show loading) |
| `processing_done`      | `workspace`, `room`                                                               | Processing finished (hide loading)      |
| `processing_cancelled` | `workspace`, `room`                                                               | Task was cancelled                      |
| `live_step`            | `workspace`, `room`, `step`                                                       | Tool use / thinking / progress step     |
| `workspace_list`       | `workspaces`                                                                      | Response to `list_workspaces`           |
| `room_list`            | `workspace`, `rooms`                                                              | Response to `list_rooms`                |
| `agent_list`           | `agents`                                                                          | Response to `list_agents`               |
| `chat_history`         | `workspace`, `room`, `messages`                                                   | Response to `get_chat_history`          |
| `skills_list`          | `skills`                                                                          | Response to `get_skills`                |
| `queue_updated`        | `workspace`, `room`, `pending_count`                                              | Pending message queue changed           |
| `system_event`         | `workspace`, `room`, `agent`, `text`                                              | System event                            |
| `error`                | `message`                                                                         | Error response                          |

### Subscription Model

Clients **subscribe** to rooms they are viewing. The server only sends `message_added`, `stream_chunk`, `live_step`, and other per-room events to clients subscribed to that room. This avoids unnecessary traffic.

When a client switches rooms, it sends `unsubscribe_room` for the old room and `subscribe_room` + `get_chat_history` for the new one.

## Package Structure

```
humu/
├── models/           # Shared data models (unchanged)
├── services/         # Backend services (unchanged)
│   ├── storage.py
│   ├── router.py
│   └── agent_runner.py
├── server/           # NEW — backend server
│   ├── __init__.py
│   ├── server.py     # WebSocket server, event broadcasting
│   └── handler.py    # Command handler (bridges WS commands → services)
├── client/           # NEW — client-side service proxy
│   ├── __init__.py
│   └── connection.py # WebSocket client, sends commands, receives events
├── tui/              # Frontend (modified)
│   ├── app.py        # Uses client.Connection instead of direct services
│   └── ...
├── config.py
└── main.py           # Entry point: `humu` connects, `humu serve` starts server
```

## Entry Points

```
humu              # Start TUI client (auto-starts server if not running)
humu serve        # Start backend server only (foreground)
humu serve -d     # Start backend server as daemon
```

### Auto-start Flow

```
humu
  → Check if ~/.humu/humu.sock exists and is connectable
  → If not: spawn `humu serve -d` in background, wait for socket
  → Connect to server via WebSocket
  → Start TUI
```

## Migration Strategy

### Phase 1 — Extract Server Interface

1. Define the command/event protocol as Python dataclasses
2. Create `server/handler.py` that wraps existing `Router`, `Storage`, `AgentRunner` calls
3. Create `server/server.py` with WebSocket server using `websockets` library
4. Create `client/connection.py` with WebSocket client

### Phase 2 — Adapt TUI

1. Replace direct `self._storage`, `self._router`, `self._runner` calls in `HumuApp` with calls through `client.Connection`
2. Replace `call_from_thread` patterns with event-driven handlers
3. Handle reconnection and server-down scenarios

### Phase 3 — Multi-Client Support

1. Server tracks connected clients and their room subscriptions
2. Broadcasting: when a message is added, server sends `message_added` to all subscribed clients
3. Processing state is shared — if one client starts processing, all clients see the loading indicator

## State Ownership

| State                              | Owner                | Notes                                           |
| :--------------------------------- | :------------------- | :---------------------------------------------- |
| Chat history                       | Server (Storage)     | Persisted to disk, served to clients on request |
| Agent sessions                     | Server (AgentRunner) | Session IDs managed server-side                 |
| Processing queue                   | Server               | Shared across all clients                       |
| Room subscriptions                 | Server (per-client)  | Which rooms each client is watching             |
| UI state (selected workspace/room) | Client               | Each TUI tracks its own view                    |
| Theme, panel widths                | Client               | Per-client preferences stored locally           |

## Dependencies

- `websockets` — async WebSocket library (both server and client)

## Open Questions

1. **Should `humu serve` auto-stop when no clients are connected?** — Probably not, since agents may be mid-processing.
2. **Authentication?** — Not needed for local Unix socket. If TCP is added later, consider token auth.
3. **Multiple workspaces per server?** — Yes, the server manages all workspaces (same as current Storage).
