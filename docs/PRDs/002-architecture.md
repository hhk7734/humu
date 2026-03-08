# Architecture

## Tech Stack

| Layer               | Choice                      |
| ------------------- | --------------------------- |
| Agent orchestration | LangGraph                   |
| TUI client          | Textual                     |
| Communication       | WebSocket                   |
| Server framework    | FastAPI                     |
| Persistence         | SQLite                      |
| LLM providers       | Anthropic (default), OpenAI |

## System Overview

```
┌─────────────┐  ┌─────────────┐  ┌─────────────┐
│  TUI Client │  │  TUI Client │  │  TUI Client │
│  (Textual)  │  │  (Textual)  │  │  (Textual)  │
└──────┬──────┘  └──────┬──────┘  └──────┬──────┘
       │ ws             │ ws             │ ws
       └────────────────┼────────────────┘
                        │
                 ┌──────┴──────┐
                 │   Server    │
                 │  (FastAPI)  │
                 ├─────────────┤
                 │  LangGraph  │
                 │  (Rooms /   │
                 │   Agents)   │
                 ├─────────────┤
                 │   SQLite    │
                 └──────┬──────┘
                        │
              ┌─────────┼─────────┐
              │         │         │
         ┌────┴───┐ ┌───┴────┐ ┌─┴──────┐
         │Anthropic│ │ OpenAI │ │  MCP   │
         │  API   │ │  API   │ │Servers │
         └────────┘ └────────┘ └────────┘
```

- **Server** (FastAPI + LangGraph): Manages workspaces, rooms, agents, LLM calls, MCP connections, and persistence. Runs persistently as a daemon.
- **TUI Client** (Textual): Connects to the server via WebSocket. Multiple instances can run simultaneously, each viewing a room.
- **SQLite DB**: Single file storing conversation history, room state, LangGraph checkpoints, and configuration.

## Room & Agent Model

Each room is a LangGraph StateGraph. The leader receives user input and constructs a dynamic DAG of agent tasks.

Agents can run in parallel or sequentially — the leader decides the execution plan per request.

```
Room (StateGraph)
┌──────────────────────────────────────┐
│                                      │
│   User ──► Leader                    │
│              │                       │
│         (builds DAG)                 │
│              │                       │
│         ┌────┴────┐                  │
│         ▼         ▼                  │
│      Agent A   Agent B  (parallel)   │
│         │         │                  │
│         └────┬────┘                  │
│              ▼                       │
│           Agent C    (depends on AB) │
│              │                       │
│              ▼                       │
│           Leader ──► User            │
│        (aggregate)                   │
│                                      │
└──────────────────────────────────────┘
```

The leader:
1. Receives user input
2. Plans the execution as a DAG (which agents, dependencies between them)
3. LangGraph executes the DAG — parallel branches run concurrently, sequential ones wait for dependencies
4. Leader aggregates final results and responds

Each agent node can have its own:
- System prompt
- Tool set (including MCP tools)
- LLM provider (Anthropic or OpenAI)

LangGraph's checkpointing saves the full state at each step, so even if the server restarts, rooms resume from where they left off.

## LLM Provider Interface

```python
class LLMProvider(Protocol):
    async def chat(
        self,
        messages: list[Message],
        tools: list[Tool] | None = None,
        **kwargs,
    ) -> LLMResponse: ...

    async def chat_stream(
        self,
        messages: list[Message],
        tools: list[Tool] | None = None,
        **kwargs,
    ) -> AsyncIterator[LLMStreamChunk]: ...
```

- Each provider (Anthropic, OpenAI) implements this interface with its own SDK.
- Agents reference a provider by name in their config. Default is Anthropic.
- Streaming is required for real-time TUI output through WebSocket.
- Adding a new provider means implementing this interface and registering it.

## Notification Interface

```python
class NotificationProvider(Protocol):
    async def send(self, notification: Notification) -> None: ...

@dataclass
class Notification:
    title: str
    body: str
    room: str
    workspace: str
    severity: Literal["info", "warning", "error"]
```

Built-in providers:
- **Sound**: Terminal bell or system sound
- **Telegram**: Bot API message
- **Slack**: Webhook or Bot API message

Users select which providers to enable in their config. Multiple providers can be active at the same time. The server tracks which rooms have an active focused client. When an agent completes work in a room with no focused client, the server triggers notifications through the enabled providers.

## MCP Integration

MCP servers are registered globally. Each agent opts in to the servers it needs, configurable via the agent management view in the TUI.

```yaml
# ~/.humu/config.yaml
mcp:
  servers:
    - name: github
      command: npx
      args: ["-y", "@modelcontextprotocol/server-github"]
      env:
        GITHUB_TOKEN: "..."
    - name: filesystem
      command: npx
      args: ["-y", "@modelcontextprotocol/server-filesystem", "/path"]
    - name: slack
      command: npx
      args: ["-y", "@modelcontextprotocol/server-slack"]
```

```yaml
# Agent config (per room)
agents:
  - name: coder
    provider: anthropic
    mcp_servers: [github, filesystem]
  - name: reviewer
    provider: anthropic
    mcp_servers: [github]
```

Agents see MCP tools alongside built-in tools — no distinction from the agent's perspective.

## Skills & Plugin System

### Marketplace Structure

```
marketplace repo (<owner>/<repo>)
├── .claude-plugin/
│   └── marketplace.json
└── plugins/
    ├── plugin-a/
    │   ├── plugin.yaml
    │   └── skills/
    │       ├── skill-a/
    │       │   └── SKILL.md
    │       └── skill-b/
    │           └── SKILL.md
    └── plugin-b/
        ├── plugin.yaml
        └── skills/
            └── skill-c/
                └── SKILL.md
```

### Local Storage

```
~/.humu/
├── config.yaml              # global config (providers, notifications, MCP)
├── marketplaces/
│   └── <owner>/<repo>/      # cloned marketplace repos
├── plugins/
│   └── <plugin-name>/
│       ├── plugin.yaml
│       └── skills/
│           ├── skill-a/
│           │   └── SKILL.md
│           └── skill-b/
│               └── SKILL.md
└── workspaces/
    └── <workspace-id>/
        ├── config.yaml      # workspace-level overrides
        └── humu.db          # SQLite (rooms, history, checkpoints)
```

### Install Flow

1. `humu marketplace add <owner>/<repo>` — clones the repo, reads `.claude-plugin/marketplace.json` to discover plugins
2. `humu plugin install <plugin-name>` — copies the selected plugin to local storage

## WebSocket Protocol

### Client → Server

- `user_message`: Send a message to a room
- `subscribe_room`: Start receiving updates for a room
- `unsubscribe_room`: Stop receiving updates
- `focus_room`: Mark a room as actively focused (suppresses notifications)

### Server → Client

- `stream_chunk`: LLM response token streaming
- `agent_status`: Agent started/completed/errored
- `dag_update`: DAG execution progress (which nodes running/done)
- `room_state_sync`: Full room state for initial load or reconnection
