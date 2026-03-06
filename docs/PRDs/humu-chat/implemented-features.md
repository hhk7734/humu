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

| Command              | Description                                            |
| :------------------- | :----------------------------------------------------- |
| `/invite <agent>`    | Add an existing agent to the current room              |
| `/kick <agent>`      | Remove an agent from the current room (not the leader) |
| `/agents`            | List all defined agents with their descriptions        |
| `/rooms`             | List all rooms in the current workspace                |
| `/status`            | Show current workspace, path, room, leader, and agents |
| `/compact`           | Summarize and clear conversation history               |
| `/help`              | Show command reference                                 |

Any unrecognised `/cmd` is treated as a skill invocation and forwarded to the router.

The `/` autocomplete dropdown (in `ChatInput`) surfaces both built-in commands and installed plugin skills in a single list, ordered: built-ins first, then skills.

## Double Ctrl+C to Quit

The first `Ctrl+C` press:

- Clears the chat input text if it is non-empty.
- Sets a "quit pending" flag and shows a notification if the input was already empty.

The second `Ctrl+C` within 2 seconds exits the application. After 2 seconds the flag resets.

## Message Queue & Processing Guard

Each `(workspace_name, room_name)` pair tracks whether a message is currently being processed in `_processing`. If the user submits another message while one is in flight, the new message is **queued** in `_pending_messages` rather than dropped. The pending queue is displayed in the `ChatPanel` so the user can see how many messages are waiting.

When processing finishes, `_process_next_queued()` is called automatically to start the next queued message. If processing is cancelled, the entire queue for that room is cleared.

## Processing Cancellation

Pressing `Escape` cancels the active agent task for the currently viewed room:

1. `action_cancel_processing()` looks up the `(loop, task)` stored in `_active_tasks`.
2. It calls `loop.call_soon_threadsafe(task.cancel)` to raise `asyncio.CancelledError` inside the background thread.
3. The `_process_message` coroutine catches `CancelledError`, clears the pending queue for the room, and posts a "⛔ Cancelled" system message to the chat.

## Processing Spinner

While any room is processing, a braille spinner (`⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏`) animates at 10 fps next to processing workspace and room names in their respective panels. The spinner timer starts with `_start_spinner()` and stops with `_stop_spinner()` once all rooms finish.

## Resizable Panels

`ResizeHandle` widgets are inserted between the four panels. Dragging a handle adjusts the adjacent panel's width in real time (subject to a configured `min_width`). An `invert` flag on the agent-panel handle means dragging left makes that panel wider.

## Panel Width Persistence

Panel widths are saved to `~/.humu/last_session.json` under a `panel_widths` key whenever the user finishes a drag. On startup, `_restore_panel_widths()` reads these values and applies them. Default widths fall back to:

| Panel              | Default width |
| :----------------- | :------------ |
| `workspace-panel`  | 18            |
| `room-panel`       | 14            |
| `agent-panel`      | 16            |

## Delete Confirmation Dialog

`Ctrl+D` now pushes a `ConfirmScreen` modal before deleting a workspace or room, showing a message like "Delete workspace 'my-app'? This cannot be undone." The delete operation only proceeds if the user confirms.

## Token Usage Display

Context usage is displayed inline in each agent's chat message header rather than in the `AgentPanel`. When an agent message is added to the chat:

- `_get_context_pct(agent_name)` is called inside `_process_message()` to compute the percentage at render time.
- It reads `Router.get_agent_tokens(ws_name, room_name, agent_name)` and divides by the model's context window size (`MODEL_CONTEXT_WINDOWS` in `config.py`, all currently 200,000 tokens).
- The result is passed as `context_pct` to `ChatPanel.add_message()` → `ChatMessage`, which renders `[leader] (42%)` in the sender label.
- Only non-system, non-user messages receive a percentage; `is_system=True` and `sender == "you"` are excluded.
- Token counts are accumulated by `Router._agent_tokens` from `TaskProgressMessage.usage` and `ResultMessage.usage` inside `AgentRunner`.

## Agent Edit (Double-Click)

Double-clicking an agent name in the `AgentPanel` within 0.5 seconds emits an `AgentEditRequested` message. The app handles this with `on_agent_edit_requested()`:

1. Loads the `AgentConfig` from storage.
2. Fetches the current token usage from the router.
3. Opens `CreateAgentScreen` in edit mode (name field is disabled; button label is "Save").
4. If the agent has a known token count > 0, the edit dialog shows a context usage bar: `Context: 12,345 / 200,000 tokens (6.2%)` with a block-character progress bar.

## Theme Persistence

Textual's `watch_theme()` reactor is overridden to call `storage.save_theme(theme)` whenever the user changes the theme. The theme name is stored in `last_session.json["theme"]`. On mount, `storage.load_theme()` is called and applied to `self.theme` before UI is rendered.

## System Event Handling (Context Compaction)

The Claude SDK emits `SystemMessage` objects for lifecycle events such as context compaction. The flow:

1. `AgentRunner` catches `SystemMessage` and appends a step with `type: "system"`, `subtype`, and `data`.
2. `Router._add_live_step()` detects `type == "system"` and fires the `on_system_event` callback with the room key, agent name, and step data.
3. `HumuApp._on_system_event()` builds a `🔄 <summary>` or `🔄 System event: <subtype>` text and posts it as a chat message.
4. Routine events are silently filtered by `_IGNORED_SYSTEM_SUBTYPES`: `init`, `task_started`, `task_progress`, `task_notification`.

## Input History Navigation

`ChatPanel` maintains an `_input_history` list of submitted messages (no consecutive duplicates). While the chat input is focused:

- **Up arrow** — saves the current draft text, then steps backward through history.
- **Down arrow** — steps forward through history; at the end, restores the saved draft.
- History navigation is disabled when the autocomplete dropdown is active.

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

## Agent Model Display

Each agent in the `AgentPanel` now shows its configured model name (e.g. `opus`, `sonnet`, `haiku`) below the agent name in muted text. The model information is loaded from `AgentConfig` via `storage.get_agent()` when `_refresh_agents()` is called and passed to `AgentPanel.set_agents()` as an `agent_models` dictionary.

## Key Bindings (Full Reference)

| Key         | Action                                                        |
| :---------- | :------------------------------------------------------------ |
| `Ctrl+N`    | Create new item (context-aware: workspace / room / agent)     |
| `Ctrl+D`    | Delete selected workspace or room (with confirmation dialog)  |
| `Ctrl+R`    | Hot-reload (restarts app, reloads all humu modules)           |
| `Ctrl+M`    | Open Plugin Manager                                           |
| `Ctrl+C`    | Clear chat input / quit on second press within 2 s            |
| `Escape`    | Cancel active processing task for the current room            |
| `Tab`       | Move focus to next panel                                      |
| `Shift+Tab` | Move focus to previous panel                                  |
| `Enter`     | Submit message (in chat input)                                |
