# Humu — Workspace, Room & Agent Management

This document describes how workspaces, rooms, and agents are created, edited, and deleted in Humu.

## Workspace

A workspace maps to a project root path (typically a git repo root). When agents operate in a workspace, their `cwd` is set to that path so they can read/edit files in the project.

### Storage

| Path                       | Contents                              |
| :------------------------- | :------------------------------------ |
| `~/.humu/workspaces.json`  | Workspace registry (name → root path) |
| `~/.humu/projects/<slug>/` | Per-workspace data (rooms, sessions)  |

### Create

- **Trigger**: `Ctrl+N` when focus is in `WorkspacePanel` (or no panel focused).
- **Screen**: `CreateWorkspaceScreen` — fields for **Name** and **Root path** (with directory suggestion dropdown).
- **Server command**: `create_workspace` with `name` and `root_path`.

### Select

- Click a workspace in `WorkspacePanel`. The last-selected room for that workspace is automatically restored.
- On startup, the last active workspace and room are restored from `~/.humu/last_session.json`.

### Delete

- **Trigger**: `Ctrl+D` when focus is in `WorkspacePanel`.
- **Confirmation**: `ConfirmScreen` — "Delete workspace '...'? This cannot be undone."
- **Server command**: `delete_workspace` with `name`.
- **What is deleted**: the workspace entry from `workspaces.json` AND the entire `~/.humu/projects/<slug>/` directory (all rooms, chat histories, and agent session data).

---

## Room

A conversation space within a workspace. Each room has exactly one **leader agent** and zero or more **member agents**. Rooms maintain their own conversation history.

### Storage

| Path                                                   | Contents                     |
| :----------------------------------------------------- | :--------------------------- |
| `~/.humu/projects/<slug>/rooms/<room>.json`            | Room config (leader, agents) |
| `~/.humu/projects/<slug>/rooms/<room>/history.json`    | Chat history                 |
| `~/.humu/projects/<slug>/rooms/<room>/agents/<agent>/` | Per-agent session data       |

### Create

- **Trigger**: `Ctrl+N` when focus is in `RoomPanel` or `ChatPanel`.
- **Screen**: `CreateRoomScreen` — single field for **Room name**.
- **Server command**: `create_room` with `workspace` and `room_name`.
- **Auto-leader**: A leader agent `<room-name>-leader` is automatically created with a default system prompt if it does not already exist. This ensures every room is immediately functional without manual agent setup.

### Select

- Click a room in `RoomPanel`. The client subscribes to real-time updates for the selected room and loads its chat history.

### Delete

- **Trigger**: `Ctrl+D` when focus is in `RoomPanel`.
- **Confirmation**: `ConfirmScreen` — "Delete room '...'? This cannot be undone."
- **Server command**: `delete_room` with `workspace` and `room_name`.
- **What is deleted**: the room config file AND the room data directory (chat history and all agent session data).

---

## Agent

A Claude-powered participant defined by:

| Property      | Type        | Description                                         |
| :------------ | :---------- | :-------------------------------------------------- |
| `name`        | `str`       | Unique identifier (e.g., `backend-expert`)          |
| `description` | `str`       | What the agent does (used by leader for routing)    |
| `prompt`      | `str`       | System prompt defining role/personality             |
| `model`       | `str`       | Claude model (`opus`, `sonnet`, `haiku`)            |
| `tools`       | `list[str]` | Allowed tools (default: `["Read", "Grep", "Glob"]`) |
| `streaming`   | `bool`      | Whether responses stream token-by-token             |

Agents are **workspace-scoped** — stored under the workspace's project directory. Agent **sessions** are room-scoped (one `ClaudeSDKClient` session per agent-room pair).

### Storage

| Path                                              | Contents         |
| :------------------------------------------------ | :--------------- |
| `~/.humu/projects/<slug>/agents/<name>.json`      | Agent definition |

### Create

- **Trigger**: `Ctrl+N` when focus is in `AgentPanel`.
- **Screen**: `CreateAgentScreen` — fields for Name, Description, System prompt, Model, Tools, Enable streaming.
- **Server command**: `create_agent` with `workspace` and agent config.

### Edit

- **Trigger**: Double-click an agent name in `AgentPanel`.
- **Screen**: `CreateAgentScreen` in edit mode (name field disabled, button label "Save").
- When token data is available, a context usage bar is shown: `Context: 12,345 / 200,000 tokens (6.2%)`.

### Model Display

Each agent in the `AgentPanel` shows its configured model name (e.g., `opus`, `sonnet`, `haiku`) below the agent name in muted text.

### Invite to Room

- **Command**: `/invite <agent-name>` in chat input.
- **Server command**: `invite_agent` with `workspace`, `room`, and `agent_name`.
- Adds the agent to the room's member list. A system message is broadcast to all subscribed clients.

### Kick from Room

- **Command**: `/kick <agent-name>` in chat input.
- **Server command**: `kick_agent` with `workspace`, `room`, and `agent_name`.
- The leader agent cannot be kicked. A system message is broadcast on success.

---

## Leader Agent

A special agent in each room responsible for message routing. Reads user messages and decides:

- **Direct answer** — respond to the user itself.
- **Forward** — route the message to one or more member agents.
- **Chain** — forward sequentially, passing one agent's output to the next.

Leader agents always use non-streaming mode (they must return a JSON routing decision atomically). See [README.md](README.md) for the routing protocol details.

---

## Management Commands

| Command           | Description                                            |
| :---------------- | :----------------------------------------------------- |
| `/invite <agent>` | Add an existing agent to the current room              |
| `/kick <agent>`   | Remove an agent from the current room (not the leader) |
| `/agents`         | List all defined agents with their descriptions        |
| `/rooms`          | List all rooms in the current workspace                |
| `/status`         | Show current workspace, path, room, leader, and agents |
