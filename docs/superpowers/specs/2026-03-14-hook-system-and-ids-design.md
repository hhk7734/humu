# Hook System Overhaul & Typed ID Adoption

## Goal

Replace humu's Unix socket hook system with an HTTP-based approach (axum), adopt typed IDs for all entities (workspace, room, tab, pane), auto-inject Claude Code hooks via `--settings`, and capture `session_id` for session resumption.

## ID System

### Typed IDs

All entities get explicit ID types via the newtype pattern in `src/id.rs`:

| Entity | Type | Backing | Persistence |
|---|---|---|---|
| Workspace | `WorkspaceId(Uuid)` | UUID v4 | Permanent — stored in `state.toml` |
| Room | `RoomId(Uuid)` | UUID v4 | Permanent — stored in `state.toml` per workspace |
| Tab | `TabId(u64)` | Sequential counter | Session-scoped — reset on restart |
| Pane | `PaneId(u64)` | Sequential counter | Session-scoped — reset on restart |

All four types implement `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `Serialize`, `Deserialize`, and `Display`.

`PaneId` changes from a type alias (`pub type PaneId = usize`) to a newtype struct. This affects all HashMap keys, arithmetic (`next_pane_id`), split tree operations, and focus tracking in `app.rs`. Access the inner value via `.0` — no `Deref` impl.

### Data Model

```rust
pub struct WorkspaceEntry {
    pub id: WorkspaceId,
    pub path: PathBuf,
    pub rooms: HashMap<String, RoomEntry>,  // room name → entry
}

pub struct RoomEntry {
    pub id: RoomId,
}
```

`WorkspaceId` is generated on workspace creation. `RoomId` is generated on first room discovery (rooms are derived from git worktrees, so IDs are assigned lazily and persisted). On startup, persisted rooms are compared against discovered git worktrees — stale entries (worktrees that no longer exist) are pruned.

### State Storage

```toml
# ~/.humu/state.toml
active_workspace_id = "550e8400-e29b-41d4-a716-446655440000"
active_room_id = "660e8400-e29b-41d4-a716-446655440001"

[workspaces.humu]
id = "550e8400-e29b-41d4-a716-446655440000"
path = "/home/user/github/humu"

[workspaces.humu.rooms.main]
id = "660e8400-e29b-41d4-a716-446655440001"

[workspaces.humu.rooms."feat/auth"]
id = "770e8400-e29b-41d4-a716-446655440002"
```

### Migration

Humu is pre-1.0. On startup, if the existing `state.toml` has the old format (string-based `active_workspace`/`active_room`, no `id` fields on workspaces, no `rooms` sub-table), discard the old state and start fresh. A log message is emitted: "Migrated state.toml to new format".

### Layout Persistence with session_id

`SplitNode::Leaf` gains an optional `session_id` field. The existing `Split` variant is unchanged:

```rust
#[serde(untagged)]
pub enum SplitNode {
    Leaf { preset: String, session_id: Option<String> },
    Split { direction: SplitDirection, ratio: f64, children: Vec<SplitNode> },
}
```

Layout keys change from string names to UUID strings:

```toml
[layout."550e8400-..."]["660e8400-..."]
active_tab = 0

[[layout."550e8400-..."]["660e8400-...".tabs]]
name = "claude"
tree = { Leaf = { preset = "claude", session_id = "abc123-def456" } }
```

On layout restore, if a pane has `session_id` and preset is `"claude"`, spawn with `claude --resume SESSION_ID --settings <path>` instead of a fresh session.

### session_id Lifecycle

1. Claude Code includes `session_id` in the JSON payload of every hook event.
2. On receiving any hook event with a non-empty `sessionId` param, humu stores it in `AgentStateEntry.session_id` keyed by `PaneId`.
3. `session_id` is persisted to the layout's `SplitNode::Leaf` whenever state is saved (workspace/room switch, clean exit via `Ctrl+q`).
4. On restore, `--resume SESSION_ID` is passed. If the session is expired or invalid, Claude Code starts a fresh session — no error handling needed from humu.
5. `--resume` and `--settings` flags are compatible — `--settings` registers hooks, `--resume` restores conversation state.

## Hook Transport

### HTTP Server (axum)

Replace the Unix domain socket server with an axum HTTP server:

- Bind to `127.0.0.1:0` (OS-assigned port)
- Write allocated port to `~/.humu/port` for external discovery
- On startup, overwrite any stale `~/.humu/port` from a previous session
- On clean exit (`Drop` impl), remove `~/.humu/port`
- On crash, the stale port file is harmless — next startup overwrites it, and the hook script's `curl` timeout handles the dead port gracefully
- Runs in the existing tokio background thread

### Endpoint

```
POST /hook?workspaceId=UUID&roomId=UUID&tabId=1&paneId=42&eventType=PostToolUse&sessionId=abc123
```

POST is used because the endpoint modifies server-side state. All parameters are query strings. `sessionId` is optional (not all events include it). Unknown query params are ignored.

### Response

- `200 OK` with empty body on success
- `400 Bad Request` if required params missing
- Unknown `eventType` values return `200` (forward compatible)

## Hook Auto-Configuration

### Generated Files

On startup, humu generates two files:

**`~/.humu/hooks/notify.sh`** — hook script using only `curl` and `grep` (no `jq`/`socat`):

```bash
#!/bin/bash
command -v curl &>/dev/null || exit 0
INPUT=$(cat)
EVENT=$(echo "$INPUT" | grep -oE '"hook_event_name"\s*:\s*"[^"]*"' | grep -oE '"[^"]*"$' | tr -d '"')
SESSION=$(echo "$INPUT" | grep -oE '"session_id"\s*:\s*"[^"]*"' | grep -oE '"[^"]*"$' | tr -d '"')
[ -z "$HUMU_PORT" ] && exit 0
curl -s --connect-timeout 1 --max-time 2 -X POST \
  "http://127.0.0.1:${HUMU_PORT}/hook?workspaceId=${HUMU_WORKSPACE_ID}&roomId=${HUMU_ROOM_ID}&tabId=${HUMU_TAB_ID}&paneId=${HUMU_PANE_ID}&eventType=${EVENT}&sessionId=${SESSION}" \
  >/dev/null 2>&1 || true
```

**`~/.humu/hooks/claude-settings.json`** — Claude Code settings with hook registration:

```json
{
  "hooks": {
    "UserPromptSubmit": [{"hooks": [{"type": "command", "command": "~/.humu/hooks/notify.sh"}]}],
    "Stop": [{"hooks": [{"type": "command", "command": "~/.humu/hooks/notify.sh"}]}],
    "PostToolUse": [{"matcher": "*", "hooks": [{"type": "command", "command": "~/.humu/hooks/notify.sh"}]}],
    "PostToolUseFailure": [{"matcher": "*", "hooks": [{"type": "command", "command": "~/.humu/hooks/notify.sh"}]}],
    "PermissionRequest": [{"matcher": "*", "hooks": [{"type": "command", "command": "~/.humu/hooks/notify.sh"}]}]
  }
}
```

### Claude Preset Spawning

When spawning a claude preset, humu:

1. Passes env vars: `HUMU_PORT`, `HUMU_WORKSPACE_ID`, `HUMU_ROOM_ID`, `HUMU_TAB_ID`, `HUMU_PANE_ID`
2. Appends `--settings ~/.humu/hooks/claude-settings.json` to the command
3. If restoring with `session_id`, appends `--resume SESSION_ID`

## Event Processing

### Canonical States

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentState {
    Working,     // spinner ⠋
    NeedsInput,  // attention ⚠
    Idle,        // no indicator
}
```

### Event Normalization

| Raw Event | Canonical State |
|---|---|
| `UserPromptSubmit` | Working |
| `PostToolUse` | Working |
| `PostToolUseFailure` | Working |
| `PermissionRequest` | NeedsInput |
| `Stop` | Idle |
| Unknown | Ignored (forward compatible) |

### State Storage

```rust
agent_states: HashMap<PaneId, AgentStateEntry>,

struct AgentStateEntry {
    state: AgentState,
    session_id: Option<String>,
    updated_at: Instant,
}
```

Per-pane tracking. No staleness timeout — trust the `Stop` event. State is cleared when:
- `Stop` event received → state set to `Idle`
- Pane process exits (detected via `exit_status()`) → entry removed
- Pane is closed → entry removed

### Derived UI State

Workspace/room panel spinners are derived from pane states:
- Show `⠋` if any pane in that workspace/room is `Working`
- Show `⚠` if any pane is `NeedsInput` and none are `Working`
- Show nothing if all panes are `Idle` or no agent panes exist

## Files Changed

### New Files
- `src/id.rs` — typed ID definitions (`WorkspaceId`, `RoomId`, `TabId`, `PaneId`)
- `src/hook/http.rs` — axum HTTP hook server

### Modified Files
- `src/config.rs` — `HumuState` uses typed IDs, `WorkspaceEntry` gains `id` + `rooms`, `SplitNode::Leaf` gains `session_id`
- `src/tui/layout.rs` — `TabContainer` uses `TabId`, `PaneId` newtype (blast radius: all HashMap keys, arithmetic, split tree, focus tracking)
- `src/app.rs` — pass IDs as env vars, process events by `PaneId`, derive spinners, resume sessions, persist `session_id` on state save
- `src/preset.rs` — claude preset appends `--settings` flag
- `src/hook/mod.rs` — re-export `http` module instead of `server`

### Removed Files
- `src/hook/server.rs` — replaced by `src/hook/http.rs`
- `scripts/humu-hook.sh` — replaced by auto-generated `~/.humu/hooks/notify.sh`

### New Dependencies
- `axum` — HTTP server
- `uuid = { version = "1", features = ["v4", "serde"] }` — workspace/room ID generation
