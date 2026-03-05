# Humu — TUI Multi-Agent Chat Application

## Overview

Humu is a terminal-based multi-agent chat application built with **Textual** and the **Claude Agent SDK**. Users create workspaces tied to project directories, organize conversations into rooms, and invite AI agents with different roles. A leader agent in each room reads user messages and either responds directly or routes them to the right member agents.

## Core Concepts

### Workspace

A workspace maps to a project root path (typically a git repo root). When agents operate in a workspace, their `cwd` is set to that path so they can read/edit files in the project.

### Room

A conversation space within a workspace. Each room has exactly one **leader agent** and zero or more **member agents**. Rooms maintain their own conversation history.

### Agent

A Claude-powered participant defined by:

| Property      | Type        | Description                                         |
| :------------ | :---------- | :-------------------------------------------------- |
| `name`        | `str`       | Unique identifier (e.g., `backend-expert`)          |
| `description` | `str`       | What the agent does (used by leader for routing)    |
| `prompt`      | `str`       | System prompt defining role/personality             |
| `model`       | `str`       | Claude model (`opus`, `sonnet`, `haiku`)            |
| `tools`       | `list[str]` | Allowed tools (default: `["Read", "Grep", "Glob"]`) |
| `streaming`   | `bool`      | Whether responses stream token-by-token             |

All agents are **room-scoped** — one `ClaudeSDKClient` session per (agent, room) pair.

### Leader Agent

A special agent in each room. Reads user messages and decides:

- **Direct answer** — respond to the user itself
- **Forward** — route the message to one or more member agents
- **Chain** — forward sequentially, passing one agent's output to the next

## Message Routing Flow

```mermaid
sequenceDiagram
    autonumber
    actor U as User
    participant L as Leader Agent
    participant A as Member Agent A
    participant B as Member Agent B

    U->>L: Send message
    activate L
    Note right of L: Evaluate message against<br/>agent descriptions

    alt Direct answer
        L-->>U: Response
    else Forward to agent(s)
        L->>A: Forward with context
        activate A
        A-->>L: Agent A response
        deactivate A
        Note right of L: Synthesize response
        L-->>U: Synthesized response
    else Chain agents
        L->>A: Forward with context
        activate A
        A-->>L: Agent A response
        deactivate A
        L->>B: Forward with Agent A output
        activate B
        B-->>L: Agent B response
        deactivate B
        Note right of L: Synthesize response
        L-->>U: Synthesized response
    end
    deactivate L
```

### Leader Routing Protocol

The leader agent uses `output_format` (JSON schema) to return structured decisions:

```json
{"action": "direct", "message": "..."}
```

```json
{"action": "forward", "targets": ["agent-a"], "context": "..."}
```

```json
{
  "action": "chain",
  "steps": [
    {"agent": "agent-a", "context": "..."},
    {"agent": "agent-b", "context": "use output from agent-a"}
  ]
}
```

## TUI Layout

```
+-- Workspace -+- Rooms --+- Chat ---------------------+- Agents --+
|              |          |                            |           |
| > my-app     | > design | [you] How should we       | * leader  |
|   infra      |   dev    | structure the API?        |   backend |
|   docs       |   review |                           |   security|
|              |          | [leader] Forwarding to    |           |
|              |          | backend...                |           |
|              |          |                           |           |
|              |          | [backend] I recommend     |           |
|              |          | REST with...              |           |
|              |          |                           |           |
|              |          | [leader] Based on         |           |
|              |          | backend's analysis...     |           |
|              |          +----------------------------+           |
|              |          | > your message here...    |           |
+--------------+----------+----------------------------+-----------+
```

### Panels

- **Workspace** — list/select workspaces
- **Rooms** — rooms in current workspace
- **Chat** — message history with sender labels + input at bottom
- **Agents** — agents in current room, leader marked with `*`

### Key Bindings

| Key      | Action                                              |
| :------- | :-------------------------------------------------- |
| `Ctrl+N` | Create new (workspace/room/agent per focused panel) |
| `Ctrl+D` | Delete selected item                                |
| `Tab`    | Cycle panel focus                                   |
| `Enter`  | Send message (in chat input)                        |
| `/`      | Command mode (`/invite`, `/kick`, etc.)             |

## Persistence

```
~/.humu/
+-- agents/                             # Agent definitions (shared)
|   +-- leader.json
|   +-- backend-expert.json
|   +-- security-reviewer.json
+-- workspaces.json                     # Workspace registry (name -> root path)
+-- projects/
    +-- <project>/                      # Per-project data
        +-- rooms/
            +-- <room>/
                +-- agents/
                    +-- <agent>/
                        +-- files       # Session data, history
```

- **`~/.humu/agents/`** — agent definitions, reusable across workspaces/rooms
- **`~/.humu/workspaces.json`** — maps workspace names to root paths
- **`~/.humu/projects/<project>/rooms/<room>/agents/<agent>/files`** — per-agent-per-room session state

## Claude Agent SDK Integration

Each agent wraps a `ClaudeSDKClient` instance:

- One session per (agent, room) pair
- `cwd` set to the workspace root path
- `session_id` stored for resume on restart
- Leader agent uses `output_format` with JSON schema for structured routing decisions
- Streaming controlled per-agent via `include_partial_messages`

## Project Structure

```
humu/
+-- __init__.py
+-- main.py                     # Entry point, Textual App
+-- models/
|   +-- workspace.py            # Workspace dataclass
|   +-- room.py                 # Room dataclass
|   +-- agent.py                # Agent dataclass
+-- services/
|   +-- agent_runner.py         # Manages ClaudeSDKClient sessions
|   +-- router.py               # Parses leader response, dispatches to agents
|   +-- storage.py              # Read/write JSON files under ~/.humu/
+-- tui/
|   +-- app.py                  # Textual App class, screen layout
|   +-- widgets/
|   |   +-- workspace_panel.py
|   |   +-- room_panel.py
|   |   +-- chat_panel.py
|   |   +-- agent_panel.py
|   +-- screens/
|       +-- create_workspace.py
|       +-- create_room.py
|       +-- create_agent.py
+-- config.py                   # Paths, defaults
```

### Responsibilities

- **models/** — pure data, no SDK dependency
- **services/agent_runner.py** — wraps `ClaudeSDKClient`, handles connect/query/resume/disconnect
- **services/router.py** — takes leader's structured JSON, calls agent runners, collects results, feeds back to leader
- **services/storage.py** — all file I/O for workspaces, rooms, agents, sessions
- **tui/** — Textual widgets and screens, calls services, never touches SDK directly

## Error Handling

| Scenario                 | Behavior                                                  |
| :----------------------- | :-------------------------------------------------------- |
| Agent session lost       | Start fresh session, log warning in chat                  |
| Leader routing malformed | Treat entire response as direct answer                    |
| Agent not in room        | Leader gets error, asked to re-route or answer directly   |
| API rate limit / error   | Display `[error]` in chat, user can retry                 |
| Workspace path missing   | Warn on switch, allow creating directory or updating path |
| Multiple agent forwards  | Run sequentially to keep chat readable                    |

## Dependencies

- `claude-agent-sdk` — Claude Agent SDK for Python
- `textual` — TUI framework

## Non-Goals

- No auth system (single user, local app)
- No plugin system (agents and tools are the extension points)
- No retry logic beyond what the SDK provides
- No parallel agent execution (sequential for readability)
