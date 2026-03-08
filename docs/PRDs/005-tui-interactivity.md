# TUI Interactivity

Full CRUD for workspaces, rooms, and agents via TUI with modal screens, panel selection, and real-time data loading.

## Connection & Data Loading

On app mount:
- Start an HTTP client (`httpx.AsyncClient`) pointed at `http://{DEFAULT_HOST}:{DEFAULT_PORT}`
- Start the WebSocket connection as a Textual worker for real-time events

Data loading is lazy and top-down:
- On mount: `GET /api/workspaces` → populate WorkspacePanel
- On workspace select: `GET /api/workspaces/{ws}/rooms` → populate RoomPanel
- On room select: `GET /api/workspaces/{ws}/rooms/{room}/agents` → populate AgentPanel, subscribe to room via WebSocket

State tracked on `HumuApp`:
- `_current_workspace: str | None`
- `_current_room: str | None`

No caching — always fetch fresh on selection change.

## Panel Selection

**WorkspacePanel:**
- `ListView` with workspace names.
- On `ListView.Selected`: set `_current_workspace`, clear `_current_room`, fetch rooms → populate RoomPanel, clear AgentPanel.

**RoomPanel:**
- `ListView` with room names.
- On `ListView.Selected`: set `_current_room`, fetch agents → populate AgentPanel. Subscribe to room via WebSocket.

**AgentPanel:**
- `ListView` with agent name (leader prefixed with `*`) and model name below in muted text.
- Display only. Double-click to edit.

**ChatPanel:**
- On room subscribe, `room_state_sync` renders message history in `#chat-messages`.
- `#chat-input`: `Enter` to submit via WebSocket `send_message`, `Shift+Enter` for newline.
- Incoming `message_added` events append to `#chat-messages`.

## Create Screens (Ctrl+N)

Three modal screens triggered by `Ctrl+N` based on focused panel:

**CreateWorkspaceScreen:**
- Fields: Name (`Input`), Root path (`Input`).
- `POST /api/workspaces` → dismiss, refresh workspace list.

**CreateRoomScreen:**
- Field: Room name (`Input`).
- `POST /api/workspaces/{ws}/rooms` → dismiss, refresh room list.
- Requires a workspace to be selected.

**CreateAgentScreen:**
- Fields: Name (`Input`), Description (`Input`), System prompt (`TextArea`), Provider (`Select` — anthropic/openai), Model (`Input`), Streaming (`Switch`).
- `POST /api/workspaces/{ws}/rooms/{room}/agents` → dismiss, refresh agent list.
- Reused for edit (double-click): pre-filled, name disabled, `PUT` instead of `POST`.
- Requires a room to be selected.

## Delete Flow (Ctrl+D)

**ConfirmDeleteScreen:**
- Generic modal taking a message string and callback.
- "Delete" and "Cancel" buttons. `Escape` to cancel.

Behavior per focused panel:
- **WorkspacePanel**: `DELETE /api/workspaces/{name}` → refresh workspace list, clear room and agent panels.
- **RoomPanel**: `DELETE /api/workspaces/{ws}/rooms/{name}` → refresh room list, clear agent panel.
- **AgentPanel**: `DELETE /api/workspaces/{ws}/rooms/{room}/agents/{name}` → refresh agent list. Server returns 400 if leader — show error toast.

`Ctrl+D` does nothing if no item is selected in the focused panel.
