# Humu — Implemented Features

This document captures features that were built during development and extend or refine what the original [README.md](README.md) described.

## Session Restoration

When Humu starts, it automatically restores the last active workspace and room.

- `~/.humu/last_session.json` stores `last_workspace` and a `rooms` map (workspace name → last room name).
- On startup, `_restore_last_session()` loads this file, resolves the workspace and room from storage, and refreshes all panels.
- When a workspace is selected, its last room (if any) is also restored automatically.

## Hot Reload

`Ctrl+R` exits the current Textual app with a sentinel return value (`"reload"`) instead of terminating the process. The `main()` entry point detects this and reloads all `humu.*` modules before starting a new `HumuApp` instance. This allows developers to apply code changes without restarting the process manually.

## Streaming Responses

Each agent can be configured with `streaming: true`. When streaming is enabled:

- `AgentRunner.query_streaming()` is used instead of `AgentRunner.query()`.
- `TextBlock` chunks are yielded as `StreamChunk` objects and emitted to the `ChatPanel` incrementally.
- A final `StreamChunk(done=True, steps=[...])` carries the accumulated tool-use steps.
- The chat panel renders a "Process log (right-click for details)" message for agents with tool steps.

Leader agents always use non-streaming mode (they must return a JSON routing decision atomically).

## Live Step Tracking

While an agent is processing, the `Router` collects steps (tool calls, thinking blocks, task progress) via a `step_callback`. The `ChatPanel` shows a loading indicator with a live step feed that updates every second while processing is in progress.

Step types tracked:

| Type            | Source                | Fields                               |
| :-------------- | :-------------------- | :----------------------------------- |
| `task_progress` | `TaskProgressMessage` | `description`, `tool` (optional)     |
| `thinking`      | `ThinkingBlock`       | `content`                            |
| `tool_use`      | `ToolUseBlock`        | `id`, `name`, `input`                |
| `tool_result`   | `ToolResultBlock`     | `tool_use_id`, `content`, `is_error` |

## Message Detail Screen

Right-clicking a chat message with tool steps opens a `MessageDetailScreen` modal. It renders the step log in a scrollable view so users can inspect exactly what the agent did (tool calls, inputs, results).

## Chat Commands

In addition to the routing-based message flow, the following slash commands are handled directly by `HumuApp` without being forwarded to agents:

| Command           | Description                                               |
| :---------------- | :-------------------------------------------------------- |
| `/invite <agent>` | Add an existing agent to the current room                 |
| `/kick <agent>`   | Remove an agent from the current room (not the leader)    |
| `/agents`         | List all defined agents with their descriptions           |
| `/rooms`          | List all rooms in the current workspace                   |
| `/status`         | Show current workspace, path, room, leader, and agents    |
| `/help`           | Show command reference                                    |
| `/skills`         | (forwarded to router as skill invocation if skill exists) |

Any unrecognised `/cmd` is treated as a skill invocation and forwarded to the router.

## Double Ctrl+C to Quit

The first `Ctrl+C` press:

- Clears the chat input text if it is non-empty.
- Sets a "quit pending" flag and shows a notification if the input was already empty.

The second `Ctrl+C` within 2 seconds exits the application. After 2 seconds the flag resets.

## Processing Guard

Each `(workspace_name, room_name)` pair tracks whether a message is currently being processed in `_processing`. If the user submits another message while one is in flight, a warning notification is shown and the new message is dropped. This prevents concurrent agent sessions on the same room.

## Chat History Persistence

Every message (user and agent) is appended to `~/.humu/projects/<workspace_slug>/rooms/<room_name>/history.json` immediately as it arrives. When the user switches to a room, the full history is loaded and rendered in the `ChatPanel`.

History entries include:

```json
{
  "sender": "you | <agent-name> | system | error",
  "text": "...",
  "is_system": false,
  "raw": "...",
  "steps": [...]
}
```

## Structured Output for Leader Routing

The leader agent is always queried with `output_format` set to a JSON schema (`ROUTING_SCHEMA` from `humu/config.py`). If the SDK returns a structured output, it takes priority over the raw text. If JSON parsing fails, the raw text is displayed as a direct answer.

## Background Message Processing

Agent queries run in a background thread (`run_worker(..., thread=True)`) to avoid blocking the Textual event loop. UI updates (loading indicators, new messages) are dispatched back to the main thread via `call_from_thread`.

## Auto-Leader Creation on Room Create

When a new room is created via `Ctrl+N` in the Room panel, Humu automatically:

1. Derives the leader agent name as `<room-name>-leader`.
2. Creates the agent with a default system prompt if it does not already exist.
3. Saves the agent and creates the room with that leader.

This ensures every room is immediately functional without manual agent setup.

## Key Bindings (Full Reference)

| Key         | Action                                                    |
| :---------- | :-------------------------------------------------------- |
| `Ctrl+N`    | Create new item (context-aware: workspace / room / agent) |
| `Ctrl+D`    | Delete selected workspace or room                         |
| `Ctrl+R`    | Hot-reload (restarts app, reloads all humu modules)       |
| `Ctrl+M`    | Open Plugin Manager                                       |
| `Ctrl+C`    | Clear chat input / quit on second press within 2 s        |
| `Tab`       | Move focus to next panel                                  |
| `Shift+Tab` | Move focus to previous panel                              |
| `Enter`     | Submit message (in chat input)                            |
