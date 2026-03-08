# Resource Management

CRUD lifecycle for workspaces, rooms, and agents, including TUI interactions.

## Workspace

A workspace maps to a project root path (typically a git repo root). When agents operate in a workspace, their `cwd` is set to that path.

### Storage

SQLite `workspaces` table:

| Column      | Type | Description                   |
|:------------|:-----|:------------------------------|
| `name`      | TEXT | Primary key                   |
| `root_path` | TEXT | Absolute path to project root |

Computed `slug` (name lowercased, spaces replaced with `-`) is used for display but not stored.

### Create

- **Trigger**: `Ctrl+N` when focus is in `WorkspacePanel`.
- **Screen**: Fields for **Name** and **Root path** (with directory autocomplete).
- **API**: `POST /api/workspaces` with `{"name": "...", "root_path": "..."}`.

### Select

- Click a workspace in `WorkspacePanel`. The last-selected room for that workspace is automatically restored.
- On startup, the last active workspace and room are restored.

### Delete

- **Trigger**: `Ctrl+D` when focus is in `WorkspacePanel`.
- **Confirmation**: "Delete workspace '...'? This will remove all rooms, agents, and chat history. This cannot be undone."
- **API**: `DELETE /api/workspaces/{name}`.
- **Cascade**: All rooms, agents, and messages belonging to the workspace are deleted via foreign key cascade.

---

## Room

A conversation space within a workspace. Each room has exactly one **leader agent** and zero or more **member agents**.

### Storage

SQLite `rooms` table:

| Column      | Type | Description                                 |
|:------------|:-----|:--------------------------------------------|
| `workspace` | TEXT | FK → workspaces(name), part of composite PK |
| `name`      | TEXT | Room name, part of composite PK             |
| `leader`    | TEXT | Name of the leader agent                    |
| `agents`    | TEXT | JSON array of member agent names            |

Chat history stored in `messages` table with `(workspace, room)` reference.

### Create

- **Trigger**: `Ctrl+N` when focus is in `RoomPanel`.
- **Screen**: Single field for **Room name**.
- **API**: `POST /api/workspaces/{workspace}/rooms` with `{"name": "..."}`.
- **Auto-leader**: A leader agent named `leader` is automatically created with a default system prompt. This ensures every room is immediately functional without manual agent setup.

### Select

- Click a room in `RoomPanel`. The client subscribes to real-time updates via WebSocket (`subscribe_room`) and loads chat history (`room_state_sync`).

### Delete

- **Trigger**: `Ctrl+D` when focus is in `RoomPanel`.
- **Confirmation**: "Delete room '...'? Chat history will be lost. This cannot be undone."
- **API**: `DELETE /api/workspaces/{workspace}/rooms/{name}`.
- **Cascade**: All agents and messages for this room are deleted.

---

## Agent

A task executor within a room. Each agent belongs to exactly one room and is configured per-room.

### Storage

SQLite `agents` table:

| Column      | Type | Description                                 |
|:------------|:-----|:--------------------------------------------|
| `workspace` | TEXT | FK → workspaces(name), part of composite PK |
| `room`      | TEXT | FK → rooms(name), part of composite PK      |
| `name`      | TEXT | Agent name, part of composite PK            |
| `config`    | TEXT | JSON-serialized AgentConfig                 |

AgentConfig fields:

| Field           | Type        | Default                      | Description                                      |
|:----------------|:------------|:-----------------------------|:-------------------------------------------------|
| `name`          | `str`       | —                            | Unique identifier (e.g., `backend-expert`)       |
| `description`   | `str`       | —                            | What the agent does (used by leader for routing) |
| `system_prompt` | `str`       | —                            | System prompt defining role/personality          |
| `provider`      | `str`       | `"anthropic"`                | LLM provider name                                |
| `model`         | `str`       | `"claude-opus-4-6"`          | Model identifier                                 |
| `mcp_servers`   | `list[str]` | `[]`                         | Opted-in MCP server names                        |
| `streaming`     | `bool`      | `false`                      | Whether responses stream token-by-token          |

### Create

- **Trigger**: `Ctrl+N` when focus is in `AgentPanel`.
- **Screen**: Fields for Name, Description, System prompt, Provider (dropdown), Model, MCP servers (multi-select from global config), Enable streaming.
- **API**: `POST /api/workspaces/{workspace}/rooms/{room}/agents` with AgentConfig JSON.

### Edit

- **Trigger**: Double-click an agent in `AgentPanel`.
- **Screen**: Same as create but name field is disabled, button label "Save".
- **API**: `PUT /api/workspaces/{workspace}/rooms/{room}/agents/{name}` with updated AgentConfig.

### Delete

- **Trigger**: `Ctrl+D` when focus is in `AgentPanel`.
- **Confirmation**: "Delete agent '...'? This cannot be undone."
- **API**: `DELETE /api/workspaces/{workspace}/rooms/{room}/agents/{name}`.
- **Constraint**: The leader agent cannot be deleted. Show error message.

### Display

Each agent in `AgentPanel` shows its configured model name below the agent name in muted text. The leader is prefixed with `*`.
