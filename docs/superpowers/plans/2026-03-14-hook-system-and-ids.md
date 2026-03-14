# Hook System Overhaul & Typed ID Adoption — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Unix socket hook system with HTTP (axum), adopt typed IDs (WorkspaceId, RoomId, TabId, PaneId), auto-inject Claude Code hooks, and capture session_id for session resumption.

**Architecture:** Introduce `src/id.rs` for typed IDs. Replace `src/hook/server.rs` with `src/hook/http.rs` (axum). Modify `src/config.rs` data model to use UUIDs for workspace/room. Update `src/app.rs` to pass IDs via env vars and process HTTP hook events.

**Tech Stack:** Rust 2024 edition, axum, uuid, tokio, serde

**Spec:** `docs/superpowers/specs/2026-03-14-hook-system-and-ids-design.md`

**Note on TabId:** `TabId(u64)` is defined in `src/id.rs` but `TabContainer` internals continue to use `usize` indices. `TabId` is only used externally (env vars, hook events). Converting `TabContainer` to use `TabId` internally is deferred — it would be a large refactor with no functional benefit at this stage.

---

## Chunk 1: Typed IDs and Config Migration

> **Build continuity note:** Tasks 3-5 form an atomic group. After Task 3 (config model change) and Task 4 (PaneId newtype), the binary crate (`src/app.rs`) will not compile until Task 5 updates all call sites. Targeted tests (`cargo test --test config_test`, `cargo test --test layout_test`) still pass because they compile only the library crate. Full `cargo build` is restored after Task 5. This is acceptable for a pre-1.0 project with a single developer.

### Task 1: Add uuid and axum dependencies

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add dependencies**

Add to `[dependencies]` section of `Cargo.toml`:

```toml
axum = "0.8"
uuid = { version = "1", features = ["v4", "serde"] }
```

- [ ] **Step 2: Verify build**

Run: `cargo build`
Expected: Compiles with new dependencies

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "build: add axum and uuid dependencies"
```

---

### Task 2: Create typed ID module

**Files:**
- Create: `src/id.rs`
- Modify: `src/lib.rs`
- Create: `tests/id_test.rs`

- [ ] **Step 1: Write tests for ID types**

Create `tests/id_test.rs`:

```rust
use humu::id::{WorkspaceId, RoomId, TabId, PaneId};

#[test]
fn workspace_id_new_is_unique() {
    let a = WorkspaceId::new();
    let b = WorkspaceId::new();
    assert_ne!(a, b);
}

#[test]
fn room_id_new_is_unique() {
    let a = RoomId::new();
    let b = RoomId::new();
    assert_ne!(a, b);
}

#[test]
fn tab_id_sequential() {
    let a = TabId(0);
    let b = TabId(1);
    assert_ne!(a, b);
    assert_eq!(a.0, 0);
    assert_eq!(b.0, 1);
}

#[test]
fn pane_id_sequential() {
    let a = PaneId(0);
    let b = PaneId(1);
    assert_ne!(a, b);
}

#[test]
fn workspace_id_serde_round_trip() {
    let id = WorkspaceId::new();
    let json = serde_json::to_string(&id).unwrap();
    let parsed: WorkspaceId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, parsed);
}

#[test]
fn room_id_serde_round_trip() {
    let id = RoomId::new();
    let json = serde_json::to_string(&id).unwrap();
    let parsed: RoomId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, parsed);
}

#[test]
fn pane_id_display() {
    let id = PaneId(42);
    assert_eq!(format!("{id}"), "42");
}

#[test]
fn workspace_id_display() {
    let id = WorkspaceId::new();
    let s = format!("{id}");
    // UUID format: 8-4-4-4-12
    assert_eq!(s.len(), 36);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test id_test`
Expected: FAIL — module `id` not found

- [ ] **Step 3: Implement ID types**

Create `src/id.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceId(pub Uuid);

impl WorkspaceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoomId(pub Uuid);

impl RoomId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for RoomId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TabId(pub u64);

impl fmt::Display for TabId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaneId(pub u64);

impl fmt::Display for PaneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
```

Add to `src/lib.rs` (add before existing `pub mod config;` line):

```rust
pub mod id;
```

- [ ] **Step 4: Run tests**

Run: `cargo test --test id_test`
Expected: All 8 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/id.rs src/lib.rs tests/id_test.rs
git commit -m "feat(id): add typed ID types for workspace, room, tab, and pane"
```

---

### Task 3: Update config data model with typed IDs

**Files:**
- Modify: `src/config.rs`
- Modify: `tests/config_test.rs`

- [ ] **Step 1: Update config_test.rs for new data model**

Update `tests/config_test.rs`. Replace the `state_round_trip` test with one that uses the new types. Add test for `SplitNode::Leaf` with `session_id`:

```rust
// Add to imports at top:
use humu::id::{WorkspaceId, RoomId};
```

Add new test:

```rust
#[test]
fn state_round_trip_with_ids() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.toml");

    let ws_id = WorkspaceId::new();
    let room_id = RoomId::new();

    let mut state = HumuState::default();
    state.active_workspace_id = Some(ws_id);
    state.active_room_id = Some(room_id);

    let mut rooms = std::collections::HashMap::new();
    rooms.insert("main".to_string(), RoomEntry { id: room_id });

    state.workspaces.insert(
        "humu".to_string(),
        WorkspaceEntry {
            id: ws_id,
            path: PathBuf::from("/tmp/humu"),
            rooms,
        },
    );

    state.save(&path).unwrap();
    let loaded = HumuState::load(&path).unwrap();

    assert_eq!(loaded.active_workspace_id, Some(ws_id));
    assert_eq!(loaded.active_room_id, Some(room_id));
    assert_eq!(loaded.workspaces["humu"].id, ws_id);
    assert_eq!(loaded.workspaces["humu"].rooms["main"].id, room_id);
}
```

Add test for `session_id` in `SplitNode`:

```rust
#[test]
fn split_node_leaf_with_session_id() {
    let node = SplitNode::Leaf {
        preset: "claude".to_string(),
        session_id: Some("abc123".to_string()),
    };
    let toml_str = toml::to_string(&node).unwrap();
    let parsed: SplitNode = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed, node);
}

#[test]
fn split_node_leaf_without_session_id() {
    let node = SplitNode::Leaf {
        preset: "shell".to_string(),
        session_id: None,
    };
    let toml_str = toml::to_string(&node).unwrap();
    let parsed: SplitNode = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed, node);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test config_test`
Expected: FAIL — fields don't exist yet

- [ ] **Step 3: Update config.rs data model**

In `src/config.rs`, make the following changes:

Add import at top:
```rust
use crate::id::{WorkspaceId, RoomId};
```

Replace `WorkspaceEntry` (lines 135-138):
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceEntry {
    pub id: WorkspaceId,
    pub path: PathBuf,
    #[serde(default)]
    pub rooms: HashMap<String, RoomEntry>,
}
```

Add `RoomEntry` after `WorkspaceEntry`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomEntry {
    pub id: RoomId,
}
```

Replace `HumuState` (lines 142-151):
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HumuState {
    pub active_workspace_id: Option<WorkspaceId>,
    pub active_room_id: Option<RoomId>,
    #[serde(default)]
    pub workspaces: HashMap<String, WorkspaceEntry>,
    /// layout[workspace_id][room_id] = RoomLayout
    #[serde(default)]
    pub layout: HashMap<String, HashMap<String, RoomLayout>>,
}
```

Update `SplitNode::Leaf` (lines 108-119) — add `session_id` field:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SplitNode {
    Leaf {
        preset: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
    },
    Split {
        direction: SplitDirection,
        ratio: f64,
        children: Vec<SplitNode>,
    },
}
```

- [ ] **Step 4: Add migration logic to HumuState::load()**

In `HumuState::load()`, wrap the TOML deserialization in a fallback. **Important:** Two migration cases:
1. Old workspace format (no `id` fields) — TOML deserialization fails → return default.
2. Old layout keys (name-based, e.g., `layout.humu.main`) — TOML parses successfully but keys are orphaned. Clear the `layout` map if `workspaces` has no entries (migration case 1 implies layout is also stale).

```rust
pub fn load(path: &Path) -> anyhow::Result<Self> {
    let content = std::fs::read_to_string(path)?;
    match toml::from_str::<Self>(&content) {
        Ok(mut state) => {
            // If workspaces have no UUIDs (empty after migration), clear layout too
            if state.workspaces.is_empty() && !state.layout.is_empty() {
                eprintln!("Clearing stale layout data from old format");
                state.layout.clear();
            }
            Ok(state)
        }
        Err(_) => {
            eprintln!("Migrated state.toml to new format (old state discarded)");
            Ok(Self::default())
        }
    }
}
```

- [ ] **Step 5: Add migration test**

```rust
#[test]
fn state_load_old_format_returns_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.toml");
    // Write old-format state with string active_workspace (no UUIDs)
    std::fs::write(&path, r#"
active_workspace = "humu"
active_room = "main"

[workspaces.humu]
path = "/tmp/humu"
"#).unwrap();
    let loaded = HumuState::load(&path).unwrap();
    // Old format can't deserialize into new types — returns default
    assert!(loaded.active_workspace_id.is_none());
    assert!(loaded.workspaces.is_empty());
}
```

- [ ] **Step 6: Fix existing config tests**

Update the existing `state_round_trip` test to use the new fields (add `id`, `rooms` to `WorkspaceEntry`, use `active_workspace_id`/`active_room_id`). Update `split_node_nested_round_trip` to include `session_id: None` on Leaf nodes.

- [ ] **Step 7: Run tests**

Run: `cargo test --test config_test`
Expected: All tests PASS

- [ ] **Step 8: Commit**

```bash
git add src/config.rs tests/config_test.rs
git commit -m "feat(config): adopt typed IDs and session_id in data model"
```

---

### Task 4: Update PaneId in layout.rs

**Files:**
- Modify: `src/tui/layout.rs`
- Modify: `tests/layout_test.rs`

- [ ] **Step 1: Replace PaneId type alias with newtype from id.rs**

In `src/tui/layout.rs`, remove:
```rust
pub type PaneId = usize;
```

Add import:
```rust
pub use crate::id::PaneId;
```

Keep re-exporting `PaneId` from this module so downstream `use humu::tui::layout::PaneId` still works.

- [ ] **Step 2: Fix all PaneId usages in layout.rs**

Replace all bare `usize` pane ID usage with `PaneId`. Key changes:
- `SplitTree::Leaf(PaneId)` — already uses the type name, no change needed
- `SplitTree::leaf(id: PaneId)` — parameter already typed
- `pane_ids()` returns `Vec<PaneId>` — already typed
- Any equality comparisons work since `PaneId` derives `PartialEq`

- [ ] **Step 3: Fix layout tests**

In `tests/layout_test.rs`, replace bare integer pane IDs with `PaneId(n)`:
```rust
use humu::tui::layout::PaneId;

// Change: SplitTree::leaf(1) → SplitTree::leaf(PaneId(1))
// Change: assert_eq!(ids, vec![1]) → assert_eq!(ids, vec![PaneId(1)])
```

- [ ] **Step 4: Run tests**

Run: `cargo test --test layout_test`
Expected: All 5 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/tui/layout.rs tests/layout_test.rs
git commit -m "refactor(layout): use PaneId newtype instead of usize alias"
```

---

### Task 5: Update app.rs for typed IDs

**Files:**
- Modify: `src/app.rs`

This is a large mechanical refactor. Key changes:

- [ ] **Step 1: Update imports**

Replace:
```rust
use humu::tui::layout::{PaneId, ...};
```
Add:
```rust
use humu::id::{WorkspaceId, RoomId, PaneId};
```

- [ ] **Step 2: Update App struct fields**

```rust
// Change next_pane_id type and initialization
pub next_pane_id: PaneId,  // was PaneId (usize alias), now PaneId newtype

// Change spinner_state key type (temporary — will be replaced in Chunk 2)
// For now, keep as HashMap<(String, String), Instant> — will become HashMap<PaneId, AgentStateEntry>
```

- [ ] **Step 3: Fix PaneId arithmetic**

Replace `self.next_pane_id += 1` with:
```rust
self.next_pane_id = PaneId(self.next_pane_id.0 + 1);
```

Replace initial value `0` with `PaneId(0)`.

Also fix `*next_id += 1` in `node_to_split_tree()` (line ~1985):
```rust
// Replace: *next_id += 1;
// With:
*next_id = PaneId(next_id.0 + 1);
```

- [ ] **Step 4: Fix all HashMap<PaneId, _> usages**

All existing `HashMap<PaneId, PtyPane>`, `HashMap<PaneId, String>`, `Option<PaneId>` calls should work since `PaneId` still derives `Hash`, `Eq`, `Copy`.

- [ ] **Step 5: Update state references (large blast radius)**

This step touches ~20 call sites. The fields change semantically: `active_workspace`/`active_room` were `Option<String>` (name-based), now `active_workspace_id`/`active_room_id` are `Option<WorkspaceId>`/`Option<RoomId>` (UUID-based).

**Key approach:** Most existing logic looks up workspace/room by name. With UUID-based active tracking, you need a helper to resolve name → id and id → name:

```rust
fn active_workspace_name(&self) -> Option<&str> {
    let ws_id = self.state.active_workspace_id?;
    self.state.workspaces.iter()
        .find(|(_, ws)| ws.id == ws_id)
        .map(|(name, _)| name.as_str())
}

fn active_room_name(&self) -> Option<&str> {
    // Similar: find room name by active_room_id in current workspace's rooms
}
```

**Full list of call sites to update (by line number):**
- Line 218: `self.state.active_room.is_none()` → `self.state.active_room_id.is_none()`
- Line 737: `self.state.active_workspace = Some(name.clone())` → look up workspace ID by name, set `active_workspace_id`
- Line 743: `self.state.active_room = Some(room.name.clone())` → look up room ID, set `active_room_id`
- Lines 769, 833, 1801: `match &self.state.active_workspace` → use `self.active_workspace_name()`
- Line 991-992: `active_workspace.as_deref() == Some(ws)` → compare by ID
- Line 1313: `self.state.active_room.is_none()` → `self.state.active_room_id.is_none()`
- Line 1406: `match &self.state.active_room` → use `self.active_room_name()`
- Lines 1430-1432: `active_workspace.as_ref()` / `active_room.as_ref()` → use helper methods
- Lines 1471-1476: similar
- Lines 1731-1753: `restore_selection()` — restore by ID instead of name
- Lines 1893-1897: `persist_layout()` — use workspace/room ID strings as layout keys
- Lines 2053-2064: `switch_workspace` / `switch_room` — set `active_workspace_id`/`active_room_id`

Also update workspace creation to assign `WorkspaceId::new()` and `WorkspaceEntry.rooms` population.

- [ ] **Step 6: Update layout save/restore**

In `split_tree_to_node()`: include `session_id` from `agent_states` (or `None` for non-claude panes). Add `session_id: None` to all Leaf construction.

In `node_to_split_tree()`: read `session_id` from `SplitNode::Leaf` and pass to spawn logic. Store in a new map `pane_sessions: HashMap<PaneId, String>` for later use.

- [ ] **Step 7: Build and fix all compile errors**

Run: `cargo build`
Fix all remaining compile errors iteratively. This is expected to touch many lines due to the PaneId and state field renames.

- [ ] **Step 8: Run all tests**

Run: `cargo test`
Expected: All tests PASS

- [ ] **Step 9: Commit**

```bash
git add src/app.rs
git commit -m "refactor(app): adopt typed IDs throughout application"
```

---

## Chunk 2: HTTP Hook Server

### Task 6: Implement axum HTTP hook server

**Files:**
- Create: `src/hook/http.rs`
- Modify: `src/hook/mod.rs`
- Create: `tests/hook_http_test.rs`

- [ ] **Step 1: Write tests for HTTP hook server**

Create `tests/hook_http_test.rs`:

```rust
use humu::hook::http::{HookServer, AgentState};
use humu::id::PaneId;

#[tokio::test]
async fn hook_server_starts_and_returns_port() {
    let server = HookServer::start().await.unwrap();
    let port = server.port();
    assert!(port > 0);
}

#[tokio::test]
async fn hook_event_updates_state() {
    let server = HookServer::start().await.unwrap();
    let port = server.port();
    let mut rx = server.subscribe();

    // Send a PostToolUse event
    let url = format!(
        "http://127.0.0.1:{port}/hook?workspaceId=550e8400-e29b-41d4-a716-446655440000&roomId=660e8400-e29b-41d4-a716-446655440001&tabId=1&paneId=42&eventType=PostToolUse&sessionId=sess123"
    );
    let resp = reqwest::Client::new().post(&url).send().await.unwrap();
    assert_eq!(resp.status(), 200);

    let event = rx.recv().await.unwrap();
    assert_eq!(event.pane_id, PaneId(42));
    assert_eq!(event.event_type, AgentState::Working);
    assert_eq!(event.session_id, Some("sess123".to_string()));
}

#[tokio::test]
async fn unknown_event_type_returns_200() {
    let server = HookServer::start().await.unwrap();
    let port = server.port();

    let url = format!(
        "http://127.0.0.1:{port}/hook?workspaceId=abc&roomId=def&tabId=1&paneId=1&eventType=FutureEvent"
    );
    let resp = reqwest::Client::new().post(&url).send().await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn missing_params_returns_400() {
    let server = HookServer::start().await.unwrap();
    let port = server.port();

    let url = format!("http://127.0.0.1:{port}/hook?paneId=1");
    let resp = reqwest::Client::new().post(&url).send().await.unwrap();
    assert_eq!(resp.status(), 400);
}
```

Add `reqwest` to `[dev-dependencies]` in `Cargo.toml`:
```toml
reqwest = { version = "0.12", features = ["json"] }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test hook_http_test`
Expected: FAIL — module not found

- [ ] **Step 3: Implement HTTP hook server**

Create `src/hook/http.rs`:

```rust
use crate::id::PaneId;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::post;
use axum::Router;
use serde::Deserialize;
use std::net::SocketAddr;
use tokio::sync::broadcast;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentState {
    Working,
    NeedsInput,
    Idle,
}

#[derive(Debug, Clone)]
pub struct HookEvent {
    pub pane_id: PaneId,
    pub event_type: AgentState,
    pub session_id: Option<String>,
}

pub struct HookServer {
    port: u16,
    tx: broadcast::Sender<HookEvent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HookParams {
    workspace_id: Option<String>,
    room_id: Option<String>,
    tab_id: Option<String>,
    pane_id: Option<u64>,
    event_type: Option<String>,
    session_id: Option<String>,
}

fn map_event_type(raw: &str) -> Option<AgentState> {
    match raw {
        "UserPromptSubmit" | "PostToolUse" | "PostToolUseFailure" => Some(AgentState::Working),
        "PermissionRequest" => Some(AgentState::NeedsInput),
        "Stop" => Some(AgentState::Idle),
        _ => None,
    }
}

impl HookServer {
    pub async fn start() -> anyhow::Result<Self> {
        let (tx, _) = broadcast::channel::<HookEvent>(256);
        let tx_clone = tx.clone();

        let app = Router::new().route(
            "/hook",
            post(move |Query(params): Query<HookParams>| {
                let tx = tx_clone.clone();
                async move {
                    let pane_id = match params.pane_id {
                        Some(id) => PaneId(id),
                        None => return StatusCode::BAD_REQUEST,
                    };
                    let event_type_str = match &params.event_type {
                        Some(s) => s.as_str(),
                        None => return StatusCode::BAD_REQUEST,
                    };

                    // Unknown event types return 200 (forward compatible)
                    let state = match map_event_type(event_type_str) {
                        Some(s) => s,
                        None => return StatusCode::OK,
                    };

                    let event = HookEvent {
                        pane_id,
                        event_type: state,
                        session_id: params.session_id.filter(|s| !s.is_empty()),
                    };
                    let _ = tx.send(event);
                    StatusCode::OK
                }
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;

        tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });

        Ok(Self {
            port: addr.port(),
            tx,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn subscribe(&self) -> broadcast::Receiver<HookEvent> {
        self.tx.subscribe()
    }
}
```

- [ ] **Step 4: Update hook/mod.rs**

Replace contents of `src/hook/mod.rs`:
```rust
pub mod http;
```

- [ ] **Step 5: Run tests**

Run: `cargo test --test hook_http_test`
Expected: All 4 tests PASS

- [ ] **Step 6: Commit**

```bash
git add src/hook/http.rs src/hook/mod.rs tests/hook_http_test.rs Cargo.toml Cargo.lock
git commit -m "feat(hook): replace Unix socket with axum HTTP hook server"
```

---

### Task 7: Remove old hook server and wire HTTP hook server into app.rs

> **Note:** These changes are combined into a single task/commit so the build never breaks between tasks (removing the old server and wiring the new one must happen atomically).

**Files:**
- Remove: `src/hook/server.rs`
- Remove: `scripts/humu-hook.sh`
- Remove: `tests/hook_test.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Delete old files**

```bash
rm src/hook/server.rs scripts/humu-hook.sh tests/hook_test.rs
```

- [ ] **Step 2: Update imports in app.rs**

Replace:
```rust
use humu::hook::server::{HookEvent, HookServer};
```
With:
```rust
use humu::hook::http::{HookEvent, HookServer, AgentState};
```

- [ ] **Step 3: Update App struct**

Replace `spinner_state` and `hook_rx`:
```rust
// Remove:
pub spinner_state: HashMap<(String, String), Instant>,
pub hook_rx: Option<mpsc::Receiver<HookEvent>>,

// Add:
pub agent_states: HashMap<PaneId, AgentStateEntry>,
pub hook_rx: Option<mpsc::Receiver<HookEvent>>,
pub hook_port: Option<u16>,
```

Add `AgentStateEntry` struct near top of app.rs:
```rust
pub struct AgentStateEntry {
    pub state: AgentState,
    pub session_id: Option<String>,
    pub updated_at: Instant,
}
```

- [ ] **Step 4: Update App::new() hook server startup**

Replace the tokio thread block that starts `HookServer::new(&sock_path)` with:

```rust
let (hook_tx, hook_rx) = mpsc::channel::<HookEvent>();
let hook_port;

// Start HTTP hook server in background thread
let (port_tx, port_rx) = mpsc::channel::<u16>();
thread::spawn(move || {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        match HookServer::start().await {
            Ok(server) => {
                let _ = port_tx.send(server.port());
                let mut rx = server.subscribe();
                loop {
                    match rx.recv().await {
                        Ok(event) => { let _ = hook_tx.send(event); }
                        Err(_) => break,
                    }
                }
            }
            Err(e) => eprintln!("hook server error: {e}"),
        }
    });
});

hook_port = port_rx.recv().ok();

// Write port file
if let Some(port) = hook_port {
    let port_path = humu_dir().join("port");
    let _ = std::fs::write(&port_path, port.to_string());
}
```

- [ ] **Step 5: Update process_hook_events()**

Replace the current implementation. **Important:** On `Stop`, set state to `Idle` but preserve the entry — the `session_id` is needed for layout persistence:

```rust
fn process_hook_events(&mut self) {
    if let Some(rx) = &self.hook_rx {
        while let Ok(event) = rx.try_recv() {
            let pane_id = event.pane_id;
            let new_session_id = event.session_id.clone();

            // Preserve existing session_id if the event doesn't include one
            let existing_session_id = self
                .agent_states
                .get(&pane_id)
                .and_then(|e| e.session_id.clone());
            let session_id = new_session_id.or(existing_session_id);

            self.agent_states.insert(pane_id, AgentStateEntry {
                state: event.event_type.clone(),
                session_id,
                updated_at: Instant::now(),
            });
        }
    }
}
```

- [ ] **Step 6: Update spinner rendering**

Replace all `self.spinner_state` references with derived state from `self.agent_states`. For workspace/room spinners, iterate `agent_states` and match panes belonging to the current workspace/room via `pane_presets`.

The exact rendering changes depend on how spinners are currently rendered — search for `spinner_state` usage and replace with `agent_states` lookups.

- [ ] **Step 7: Add Drop impl for port file cleanup and remove old socket cleanup**

Implement `Drop` for `App` to remove the port file on clean exit:
```rust
impl Drop for App {
    fn drop(&mut self) {
        let port_path = humu_dir().join("port");
        let _ = std::fs::remove_file(&port_path);
    }
}
```

Also remove the old socket cleanup lines in `App::run()` (around line 277-280):
```rust
// Remove these lines:
let sock_path = humu_dir().join("humu.sock");
let _ = std::fs::remove_file(&sock_path);
```

The `Drop` impl handles port file cleanup on both clean exit and panic unwind.

- [ ] **Step 8: Build and test**

Run: `cargo test`
Expected: All tests PASS

- [ ] **Step 9: Commit**

```bash
git add -u src/hook/server.rs scripts/humu-hook.sh tests/hook_test.rs
git add src/app.rs
git commit -m "feat(app): replace Unix socket with HTTP hook server and per-pane agent state"
```

---

## Chunk 3: Auto-Configuration and Session Resume

### Task 8: Generate hook files on startup

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add hook file generation function**

Add to `src/app.rs` (or a helper module):

```rust
fn generate_hook_files() -> anyhow::Result<()> {
    let hooks_dir = humu_dir().join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;

    // Generate notify.sh
    let notify_path = hooks_dir.join("notify.sh");
    std::fs::write(&notify_path, r#"#!/bin/bash
command -v curl &>/dev/null || exit 0
INPUT=$(cat)
EVENT=$(echo "$INPUT" | grep -oE '"hook_event_name"\s*:\s*"[^"]*"' | grep -oE '"[^"]*"$' | tr -d '"')
SESSION=$(echo "$INPUT" | grep -oE '"session_id"\s*:\s*"[^"]*"' | grep -oE '"[^"]*"$' | tr -d '"')
[ -z "$HUMU_PORT" ] && exit 0
curl -s --connect-timeout 1 --max-time 2 -X POST \
  "http://127.0.0.1:${HUMU_PORT}/hook?workspaceId=${HUMU_WORKSPACE_ID}&roomId=${HUMU_ROOM_ID}&tabId=${HUMU_TAB_ID}&paneId=${HUMU_PANE_ID}&eventType=${EVENT}&sessionId=${SESSION}" \
  >/dev/null 2>&1 || true
"#)?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&notify_path, std::fs::Permissions::from_mode(0o755))?;
    }

    // Generate claude-settings.json
    let settings_path = hooks_dir.join("claude-settings.json");
    let notify_abs = notify_path.to_string_lossy();
    let settings = serde_json::json!({
        "hooks": {
            "UserPromptSubmit": [{"hooks": [{"type": "command", "command": notify_abs}]}],
            "Stop": [{"hooks": [{"type": "command", "command": notify_abs}]}],
            "PostToolUse": [{"matcher": "*", "hooks": [{"type": "command", "command": notify_abs}]}],
            "PostToolUseFailure": [{"matcher": "*", "hooks": [{"type": "command", "command": notify_abs}]}],
            "PermissionRequest": [{"matcher": "*", "hooks": [{"type": "command", "command": notify_abs}]}]
        }
    });
    std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;

    Ok(())
}
```

- [ ] **Step 2: Call from App::new()**

Add at the start of `App::new()`:
```rust
if let Err(e) = generate_hook_files() {
    eprintln!("failed to generate hook files: {e}");
}
```

- [ ] **Step 3: Add test for generate_hook_files**

Extract `generate_hook_files` to accept a base directory parameter so it can be tested with a tempdir:

```rust
fn generate_hook_files(base_dir: &Path) -> anyhow::Result<()> {
    let hooks_dir = base_dir.join("hooks");
    // ... same logic but uses base_dir instead of humu_dir()
}
```

Add test to `tests/hook_http_test.rs` (or a new test file):
```rust
#[test]
fn generate_hook_files_creates_expected_files() {
    let dir = tempfile::tempdir().unwrap();
    generate_hook_files(dir.path()).unwrap();

    let notify = dir.path().join("hooks/notify.sh");
    assert!(notify.exists());
    let content = std::fs::read_to_string(&notify).unwrap();
    assert!(content.contains("HUMU_PORT"));
    assert!(content.contains("curl"));

    let settings = dir.path().join("hooks/claude-settings.json");
    assert!(settings.exists());
    let json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&settings).unwrap()
    ).unwrap();
    assert!(json["hooks"]["Stop"].is_array());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&notify).unwrap().permissions().mode();
        assert!(mode & 0o111 != 0, "notify.sh should be executable");
    }
}
```

- [ ] **Step 4: Build and test**

Run: `cargo test`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/app.rs tests/hook_http_test.rs
git commit -m "feat(hook): auto-generate notify.sh and claude-settings.json on startup"
```

---

### Task 9: Pass IDs as env vars, add --settings to claude preset, and fix restore path

**Files:**
- Modify: `src/app.rs` (spawn_pane, node_to_split_tree, split_tree_to_node)

> **Note:** The spec lists `src/preset.rs` under modified files, but `--settings` injection is handled entirely in `app.rs`'s `spawn_pane()` where env vars are already set. This keeps the logic co-located rather than splitting across files.

- [ ] **Step 1: Update spawn_pane signature and env vars**

Add `session_id` parameter to `spawn_pane`:
```rust
fn spawn_pane(&mut self, preset_name: &str, session_id: Option<String>) -> Option<PaneId>
```

In `spawn_pane()`, replace the current claude env var block with:

```rust
if preset_name == "claude" {
    let settings_path = humu_dir().join("hooks/claude-settings.json");
    // Append --settings to args
    args.push("--settings".to_string());
    args.push(settings_path.to_string_lossy().into_owned());

    // If we have a session_id for this pane, append --resume
    if let Some(sid) = session_id {
        args.push("--resume".to_string());
        args.push(sid);
    }

    if let Some(port) = self.hook_port {
        envs.push(("HUMU_PORT".to_string(), port.to_string()));
    }
    if let Some(ws_id) = self.state.active_workspace_id {
        envs.push(("HUMU_WORKSPACE_ID".to_string(), ws_id.to_string()));
    }
    if let Some(room_id) = self.state.active_room_id {
        envs.push(("HUMU_ROOM_ID".to_string(), room_id.to_string()));
    }
    // TabId and PaneId are set using the current tab/pane
    envs.push(("HUMU_TAB_ID".to_string(), self.tabs.active_index().to_string()));
    envs.push(("HUMU_PANE_ID".to_string(), id.0.to_string()));
}
```

- [ ] **Step 2: Update all spawn_pane call sites**

Pass `None` for fresh spawns. Known call sites:
- `new_tab_with_preset()` — `self.spawn_pane(preset, None)`
- `handle_pane()` split handlers — `self.spawn_pane(preset, None)`
- Any other direct calls — search for `spawn_pane(` to find all

- [ ] **Step 3: Refactor node_to_split_tree to use spawn_pane**

Currently `node_to_split_tree` is a static method that calls `PtyPane::spawn()` directly. This means restored panes do not get env vars or `--settings`. **Refactor it to an instance method** so it can call `self.spawn_pane()`:

```rust
fn node_to_split_tree(
    &mut self,
    node: &SplitNode,
) -> Option<SplitTree> {
    match node {
        SplitNode::Leaf { preset, session_id } => {
            let id = self.spawn_pane(preset, session_id.clone())?;
            Some(SplitTree::Leaf(id))
        }
        SplitNode::Split { direction, ratio, children } => {
            let left = self.node_to_split_tree(&children[0])?;
            let right = self.node_to_split_tree(&children[1])?;
            let dir = match direction {
                CfgDir::Vertical => SplitDirection::Vertical,
                CfgDir::Horizontal => SplitDirection::Horizontal,
            };
            Some(SplitTree::Split {
                direction: dir,
                ratio: *ratio,
                children: Box::new((left, right)),
            })
        }
    }
}
```

Update the call site in `restore_layout()` — it can now call `self.node_to_split_tree(node)` directly.

- [ ] **Step 4: Update split_tree_to_node to save session_id**

In `split_tree_to_node()`, when creating a `SplitNode::Leaf`, read `session_id` from `agent_states`:

```rust
SplitTree::Leaf(pane_id) => {
    let preset = pane_presets.get(&pane_id)?.clone();
    let session_id = self.agent_states.get(&pane_id).and_then(|e| e.session_id.clone());
    Some(SplitNode::Leaf { preset, session_id })
}
```

- [ ] **Step 5: Build and test**

Run: `cargo test`
Expected: All tests PASS

- [ ] **Step 6: Commit**

```bash
git add src/app.rs
git commit -m "feat(hook): pass IDs as env vars and auto-inject --settings for claude"
```

---

### Task 10: Clean up agent state on pane exit/close

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Clear agent state when pane is closed**

In `close_pane()`, after removing from `self.panes`:
```rust
self.agent_states.remove(&focused);
```

In `close_tab()`, in the loop that removes panes:
```rust
for id in tree.pane_ids() {
    self.panes.remove(&id);
    self.pane_presets.remove(&id);
    self.agent_states.remove(&id);
}
```

- [ ] **Step 2: Clear agent state when pane process exits**

In `process_hook_events()` or in the render loop where `exit_status()` is checked, remove agent state for exited panes:

```rust
// After checking exit_status for a pane that has exited:
self.agent_states.remove(&pane_id);
```

- [ ] **Step 3: Build and test**

Run: `cargo test`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add src/app.rs
git commit -m "fix(app): clean up agent state on pane exit and close"
```

---

### Task 11: Room ID lazy assignment and pruning

**Files:**
- Modify: `src/config.rs` (add functions here — lib crate, importable by tests)
- Modify: `src/app.rs` (wire the functions into room selection and startup)
- Modify: `tests/config_test.rs`

- [ ] **Step 1: Write tests for room ID assignment and pruning**

Add to `tests/config_test.rs`:

```rust
use humu::config::{ensure_room_id_for_workspace, prune_stale_rooms_for_workspace};
use std::collections::HashSet;

#[test]
fn ensure_room_id_creates_new_id() {
    let mut state = HumuState::default();
    let ws_id = WorkspaceId::new();
    state.workspaces.insert("test".to_string(), WorkspaceEntry {
        id: ws_id,
        path: PathBuf::from("/tmp/test"),
        rooms: HashMap::new(),
    });

    // First call creates ID
    let id1 = ensure_room_id_for_workspace(&mut state, "test", "main").unwrap();
    // Second call returns same ID
    let id2 = ensure_room_id_for_workspace(&mut state, "test", "main").unwrap();
    assert_eq!(id1, id2);

    // Different room gets different ID
    let id3 = ensure_room_id_for_workspace(&mut state, "test", "dev").unwrap();
    assert_ne!(id1, id3);
}

#[test]
fn prune_removes_stale_rooms() {
    let mut state = HumuState::default();
    let ws_id = WorkspaceId::new();
    let mut rooms = HashMap::new();
    rooms.insert("main".to_string(), RoomEntry { id: RoomId::new() });
    rooms.insert("deleted-branch".to_string(), RoomEntry { id: RoomId::new() });
    state.workspaces.insert("test".to_string(), WorkspaceEntry {
        id: ws_id,
        path: PathBuf::from("/tmp/test"),
        rooms,
    });

    // Only "main" exists on disk
    let discovered = HashSet::from(["main".to_string()]);
    prune_stale_rooms_for_workspace(&mut state, "test", &discovered);

    let ws = &state.workspaces["test"];
    assert!(ws.rooms.contains_key("main"));
    assert!(!ws.rooms.contains_key("deleted-branch"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test config_test`
Expected: FAIL — functions not found

- [ ] **Step 3: Implement functions in src/config.rs**

Add to `src/config.rs` (lib crate — importable by integration tests):

```rust
use crate::id::{RoomId};
use std::collections::HashSet;

pub fn ensure_room_id_for_workspace(
    state: &mut HumuState,
    workspace_name: &str,
    room_name: &str,
) -> Option<RoomId> {
    let ws = state.workspaces.get_mut(workspace_name)?;
    if let Some(entry) = ws.rooms.get(room_name) {
        Some(entry.id)
    } else {
        let id = RoomId::new();
        ws.rooms.insert(room_name.to_string(), RoomEntry { id });
        Some(id)
    }
}

pub fn prune_stale_rooms_for_workspace(
    state: &mut HumuState,
    workspace_name: &str,
    discovered_rooms: &HashSet<String>,
) {
    if let Some(ws) = state.workspaces.get_mut(workspace_name) {
        ws.rooms.retain(|name, _| discovered_rooms.contains(name));
    }
}
```

- [ ] **Step 4: Wire into app.rs**

In `src/app.rs`, call these functions:
- `ensure_room_id_for_workspace` during room selection / room listing (so IDs are assigned lazily)
- `prune_stale_rooms_for_workspace` on startup after room discovery from git worktrees

- [ ] **Step 5: Run tests**

Run: `cargo test --test config_test`
Expected: All tests PASS

- [ ] **Step 6: Commit**

```bash
git add src/config.rs src/app.rs tests/config_test.rs
git commit -m "feat(config): lazy room ID assignment and stale room pruning"
```

---

### Task 12: Update documentation

**Files:**
- Modify: `CLAUDE.md`
- Modify: `docs/PRDs/002-architecture.md`

- [ ] **Step 1: Update CLAUDE.md**

Update the project structure to reflect new files:
- `src/id.rs` — Typed IDs (WorkspaceId, RoomId, TabId, PaneId)
- `src/hook/http.rs` — HTTP hook server (axum)
- Remove reference to `scripts/humu-hook.sh`

- [ ] **Step 2: Update architecture PRD**

Update `docs/PRDs/002-architecture.md`:
- Replace Unix socket description with HTTP server
- Document the hook auto-configuration
- Update env var list
- Add session_id flow

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md docs/PRDs/002-architecture.md
git commit -m "docs: update architecture for HTTP hooks and typed IDs"
```
