# Server/Client Sessions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split humu into a background server and attachable client so closing the client does not stop running PTY-backed tasks.

**Architecture:** First migrate persistence to a session-scoped model without leaving two layout sources of truth, then introduce a complete daemon protocol and an attachable client while preserving the current `humu` UX until the new client can render real server snapshots. Server ownership includes PTYs, hook/Codex tracking, notifications, and session lifecycle; client ownership is limited to terminal I/O, rendering, and transient popup state.

**Tech Stack:** Rust, ratatui, crossterm, portable-pty, axum, serde/serde_yaml, Unix domain sockets, existing hook/Codex integration.

---

## File Structure

### Existing files to modify

- `Cargo.toml`
  - Add IPC/daemon support dependencies only if std + serde are insufficient.
- `src/main.rs`
  - Keep current single-process startup until the new attachable client exists; then switch default `humu` to daemon discovery + attach.
- `src/cli.rs`
  - Parse `server`, `attach`, `list-sessions`, and `detach --force` commands before the default entrypoint flips.
- `src/lib.rs`
  - Export shared, server, client, and test-support modules as they are introduced.
- `src/app.rs`
  - First switch layout persistence to the new session-scoped source of truth, then later shrink toward client-only TUI logic.
- `src/config.rs`
  - Add persisted session-scoped state and migration from the legacy singleton model.
- `src/notification/mod.rs`
  - Accept server-owned session focus state.
- `src/codex.rs`
  - Move tracker ownership into the daemon runtime.
- `src/hook/http.rs`
  - Move hook server lifecycle under the daemon.
- `docs/PRDs/002-architecture.md`
  - Update the steady-state architecture after implementation.
- `docs/PRDs/003-tui-layout.md`
  - Document session attach/list/detach UX once behavior changes.
- `docs/PRDs/006-notifications.md`
  - Document detached-session focus semantics.

### New files to create

- `src/shared/mod.rs`
  - Shared exports for protocol and render snapshot types.
- `src/shared/protocol.rs`
  - Full IPC request/response/event enums and wire helpers.
- `src/shared/render.rs`
  - Snapshot/update structs used by both daemon and client.
- `tests/daemon_restart.rs`
  - Integration coverage for daemon restart cold restore and detached hook/Codex continuity.
- `src/server/mod.rs`
  - Server module exports.
- `src/server/daemon.rs`
  - Socket bind/listen, metadata lifecycle, ping/readiness, stale-socket cleanup, lock-file startup.
- `src/server/session.rs`
  - `SessionManager`, attach locks, force detach, session registry.
- `src/server/runtime.rs`
  - Session-owned PTYs, layouts, hook/Codex tracking, notifications, and resize/input handling.
- `src/server/persistence.rs`
  - Session-scoped load/save and legacy migration helpers.
- `src/client/mod.rs`
  - Client module exports.
- `src/client/attach.rs`
  - Discovery, auto-launch, list/attach/detach commands.
- `src/client/state.rs`
  - Client view model built from snapshots/events.
- `src/client/tui_app.rs`
  - Ratatui event loop backed by server snapshots instead of local PTYs.
- `tests/support/mod.rs`
  - Shared test harnesses and fixture builders.
- `tests/session_persistence.rs`
  - Integration coverage for migration and multi-session persistence.
- `tests/server_attach.rs`
  - Integration coverage for daemon discovery, stale-socket handling, attach locking, and protocol round-trips.
- `tests/detach_survival.rs`
  - Integration coverage proving client exit does not terminate PTY children.
- `tests/notifications_focus.rs`
  - Integration coverage for focused vs detached notification behavior.

## Task 1: Migrate Layout Persistence To A Session-Scoped Source Of Truth

**Files:**
- Modify: `src/config.rs`
- Modify: `src/app.rs`
- Create: `src/server/persistence.rs`
- Create: `tests/support/mod.rs`
- Test: `tests/session_persistence.rs`

- [x] **Step 1: Write the failing migration and source-of-truth tests**

```rust
#[test]
fn legacy_state_migrates_into_default_session_layouts() {
    let legacy = support::legacy_state_fixture();
    let migrated = migrate_legacy_state(legacy);
    assert!(migrated.sessions.iter().any(|s| s.name == "default"));
    assert!(migrated.sessions[0].tabs_by_room.contains_key(&support::room_id("main")));
}

#[test]
fn app_persists_room_layouts_only_via_session_state() {
    let mut app = support::app_with_migrated_state();
    app.persist_layout();
    let saved = support::reload_state(&app);
    assert!(saved.sessions[0].tabs_by_room.contains_key(&support::room_id("main")));
    assert!(saved.workspaces[0].rooms[0].tabs.is_empty());
}

#[test]
fn named_sessions_persist_independent_room_layouts_and_selection() {
    let mut state = support::migrated_state_fixture();
    support::persist_named_session_layout(&mut state, "default", "main", "shell");
    support::persist_named_session_layout(&mut state, "review", "feat-x", "codex");
    let reloaded = support::round_trip_state(state);
    assert_ne!(
        reloaded.session_by_name("default").unwrap().active_room_id,
        reloaded.session_by_name("review").unwrap().active_room_id
    );
}
```

- [x] **Step 2: Run tests to verify they fail**

Run: `cargo test --test session_persistence -- --nocapture`
Expected: FAIL with missing session structs and legacy `RoomEntry.tabs` still used for persistence

- [x] **Step 3: Add session-scoped persisted structs and migration helpers**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionState {
    pub name: String,
    pub active_workspace_id: Option<WorkspaceId>,
    pub active_room_id: Option<RoomId>,
    #[serde(default)]
    pub tabs_by_room: HashMap<RoomId, PersistedRoomLayout>,
    #[serde(default)]
    pub attached: bool,
    #[serde(default)]
    pub last_size: Option<SessionSize>,
}
```

- [x] **Step 4: Switch `App` layout restore/save to the `default` session layout map immediately**

```rust
let layout = self
    .state
    .session_by_name("default")
    .and_then(|s| s.tabs_by_room.get(&room_id))
    .cloned();
```

- [x] **Step 5: Stop writing session-owned layout back into `RoomEntry.tabs` and `active_tab`**

```rust
room.tabs.clear();
room.active_tab = None;
```

- [x] **Step 6: Run tests to verify they pass**

Run: `cargo test --test session_persistence -- --nocapture`
Expected: PASS, including independent named-session persistence

- [ ] **Step 7: Commit**

```bash
git add src/config.rs src/app.rs src/server/persistence.rs tests/support/mod.rs tests/session_persistence.rs
git commit -m "refactor: migrate layout persistence to session state"
```

## Task 2: Define The Complete Protocol And Snapshot Contract

**Files:**
- Create: `src/shared/mod.rs`
- Create: `src/shared/protocol.rs`
- Create: `src/shared/render.rs`
- Modify: `src/lib.rs`
- Modify: `tests/server_attach.rs`

- [x] **Step 1: Write the failing protocol and snapshot tests**

```rust
#[test]
fn client_request_round_trips_with_all_core_variants() {
    let requests = vec![
        ClientRequest::Ping,
        ClientRequest::Detach,
        ClientRequest::ResizeSession { cols: 120, rows: 40 },
        ClientRequest::SubscribeUpdates,
        ClientRequest::FocusChanged { focused: true },
    ];
    for req in requests {
        let bytes = serde_json::to_vec(&req).unwrap();
        let decoded: ClientRequest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(req, decoded);
    }
}

#[test]
fn full_snapshot_exposes_all_spec_fields() {
    let snapshot = FullSnapshot::fixture();
    assert!(snapshot.active_workspace_id.is_some());
    assert!(snapshot.active_room_id.is_some());
    assert!(!snapshot.tabs.is_empty());
    assert!(snapshot.active_tab_index.is_some());
    assert!(snapshot.split_tree.is_some());
    assert!(snapshot.focused_pane_id.is_some());
    assert!(snapshot.fullscreen_pane_id.is_some());
    assert!(!snapshot.panes.is_empty());
    let pane = snapshot.panes.values().next().unwrap();
    assert!(!pane.preset_name.is_empty());
    assert!(pane.capabilities.mouse_protocol_mode.is_some());
    assert!(pane.agent_state.is_some());
    assert!(snapshot.explorer_root.is_some());
}
```

- [x] **Step 2: Run tests to verify they fail**

Run: `cargo test --test server_attach client_request_round_trips_with_all_core_variants -- --nocapture`
Expected: FAIL with missing protocol and snapshot types

- [x] **Step 3: Define all request, response, and event enums required by the PRD**

```rust
pub enum ClientRequest {
    Ping,
    ListSessions,
    CreateSession { name: String },
    AttachSession { name: String, cols: u16, rows: u16 },
    Detach,
    ForceDetachSession { name: String },
    SendInput { pane_id: PaneId, bytes: Vec<u8> },
    ResizeSession { cols: u16, rows: u16 },
    RunAction { action: ClientAction },
    SubscribeUpdates,
    FocusChanged { focused: bool },
}
```

- [x] **Step 4: Define `FullSnapshot`, `PaneSnapshot`, and incremental event payloads**

```rust
pub struct FullSnapshot {
    pub session_name: String,
    pub active_workspace_id: Option<WorkspaceId>,
    pub active_room_id: Option<RoomId>,
    pub tabs: Vec<TabSnapshot>,
    pub active_tab_index: Option<usize>,
    pub split_tree: Option<SplitTreeSnapshot>,
    pub focused_pane_id: Option<PaneId>,
    pub fullscreen_pane_id: Option<PaneId>,
    pub panes: HashMap<PaneId, PaneSnapshot>,
    pub explorer_root: Option<PathBuf>,
}
```

- [x] **Step 5: Run tests to verify they pass**

Run: `cargo test --test server_attach -- --nocapture`
Expected: PASS for protocol and snapshot contract tests

- [x] **Step 6: Commit**

```bash
git add src/shared/mod.rs src/shared/protocol.rs src/shared/render.rs src/lib.rs tests/server_attach.rs
git commit -m "feat: define server-client protocol and snapshots"
```

Task 2 follow-up fixes:
- Use framed wire helpers suitable for streaming Unix socket traffic
- Preserve per-cell terminal styling in the shared screen snapshot contract
- Include pane geometry payloads in `LayoutUpdated`

## Task 3: Add Test Harnesses For Daemon, Client, And PTY Survival

**Files:**
- Create: `tests/support/mod.rs`
- Modify: `tests/server_attach.rs`
- Modify: `tests/detach_survival.rs`
- Modify: `tests/notifications_focus.rs`

- [x] **Step 1: Write the failing harness smoke tests**

```rust
#[test]
fn support_can_spawn_isolated_humu_home() {
    let env = support::isolated_humu_home();
    assert!(env.home.path().exists());
}

#[test]
fn support_can_spawn_background_pty_fixture() {
    let harness = support::spawn_sleeping_shell();
    assert!(harness.child_is_alive());
}
```

- [x] **Step 2: Run tests to verify they fail**

Run: `cargo test support_can_spawn_isolated_humu_home -- --nocapture`
Expected: FAIL with missing support module

- [x] **Step 3: Implement reusable test fixtures and harness helpers**

```rust
pub fn isolated_humu_home() -> TestEnv { /* ... */ }
pub fn spawn_humu_attach(env: &TestEnv, session: &str) -> Child { /* ... */ }
pub fn run_humu_attach(env: &TestEnv, session: &str) -> ExitStatus { /* ... */ }
pub fn spawn_sleeping_shell() -> PtyHarness { /* ... */ }
```

- [x] **Step 4: Run tests to verify they pass**

Run: `cargo test support_can_spawn_isolated_humu_home -- --nocapture`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add tests/support/mod.rs tests/server_attach.rs tests/detach_survival.rs tests/notifications_focus.rs
git commit -m "test: add daemon and pty harness helpers"
```

## Task 4: Build The Daemon Shell Without Changing Default `humu` Yet

**Files:**
- Modify: `src/main.rs`
- Create: `src/cli.rs`
- Create: `src/server/mod.rs`
- Create: `src/server/daemon.rs`
- Create: `src/server/session.rs`
- Modify: `tests/server_attach.rs`

- [ ] **Step 1: Write the failing daemon discovery and stale-socket tests**

```rust
#[test]
fn server_ping_works_after_daemon_start() {
    let env = support::isolated_humu_home();
    support::spawn_humu_server(&env);
    assert!(support::ping_server(&env).is_ok());
}

#[test]
fn stale_socket_and_metadata_are_cleaned_when_pid_is_dead() {
    let env = support::isolated_humu_home();
    support::write_stale_server_files(&env);
    support::spawn_humu_server(&env);
    assert!(support::ping_server(&env).is_ok());
}

#[test]
fn session_manager_rejects_second_attach_with_machine_readable_payload() {
    let mut manager = SessionManager::default();
    manager.attach("default", support::client_id("a")).unwrap();
    match manager.attach("default", support::client_id("b")) {
        Err(AttachError::AlreadyAttached { session_name, owner_pid, .. }) => {
            assert_eq!(session_name, "default");
            assert!(owner_pid.is_some());
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn client_refuses_protocol_version_mismatch() {
    let env = support::isolated_humu_home();
    support::spawn_version_mismatched_server(&env);
    let status = support::run_humu_attach(&env, "default");
    assert!(!status.success());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test server_attach server_ping_works_after_daemon_start -- --nocapture`
Expected: FAIL with missing daemon entry and socket handling

- [ ] **Step 3: Add `humu server` entry path, socket metadata, ping, and startup lock**

```rust
match cli.command {
    Some(Command::Server { daemon }) => server::daemon::run(daemon),
    _ => App::new()?.run(),
}
```

- [ ] **Step 4: Introduce a minimal CLI parser module for future commands without flipping the default path yet**

```rust
pub enum Command {
    Server { daemon: bool },
    Attach { session: Option<String> },
    ListSessions,
    Detach { session: Option<String>, force: bool },
}
```

- [ ] **Step 5: Implement `SessionManager` with idempotent create and single-client attach lock**

```rust
pub struct SessionManager {
    sessions: HashMap<String, SessionEntry>,
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --test server_attach -- --nocapture`
Expected: PASS for ping, stale-socket cleanup, lock-file startup, unit-tested attach-lock rules, and version-mismatch refusal

- [ ] **Step 7: Commit**

```bash
git add src/main.rs src/cli.rs src/server/mod.rs src/server/daemon.rs src/server/session.rs tests/server_attach.rs
git commit -m "feat: add daemon shell and session registry"
```

## Task 5: Move Hook Server, Codex Tracker, And Notifications Into Server Runtime

**Files:**
- Create: `src/server/runtime.rs`
- Modify: `src/hook/http.rs`
- Modify: `src/codex.rs`
- Modify: `src/notification/mod.rs`
- Modify: `src/server/daemon.rs`
- Modify: `tests/notifications_focus.rs`

- [ ] **Step 1: Write the failing runtime-ownership tests**

```rust
#[test]
fn detached_session_is_treated_as_unfocused_for_notifications() {
    let runtime = support::runtime_fixture_detached();
    assert!(runtime.should_fire_only_unfocused_notifications());
}

#[test]
fn daemon_publishes_hook_port_file() {
    let env = support::isolated_humu_home();
    support::spawn_humu_server(&env);
    assert!(env.home.path().join("port").exists());
}

#[test]
fn detached_hook_and_codex_updates_continue_without_client() {
    let runtime = support::runtime_fixture_detached();
    support::emit_hook_event(&runtime, "needs_input");
    support::emit_codex_event(&runtime, "task_complete");
    assert!(runtime.recorded_agent_updates() >= 2);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test notifications_focus -- --nocapture`
Expected: FAIL with runtime still owned by `App`

- [ ] **Step 3: Introduce `SessionRuntime` and move hook/Codex/notification ownership there**

```rust
pub struct SessionRuntime {
    pub panes: HashMap<PaneId, PtyPane>,
    pub codex_tracker: CodexTracker,
    pub notification_focus: SessionFocusState,
}
```

- [ ] **Step 4: Keep daemon-owned `~/.humu/port` publication compatible with existing hook scripts**

```rust
write_hook_port_file(hook_port)?;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test notifications_focus -- --nocapture`
Expected: PASS for detached focus semantics, detached hook/Codex continuity, and hook port publication

- [ ] **Step 6: Commit**

```bash
git add src/server/runtime.rs src/hook/http.rs src/codex.rs src/notification/mod.rs src/server/daemon.rs tests/notifications_focus.rs
git commit -m "refactor: move runtime integrations into daemon"
```

## Task 6: Move PTY Ownership Off `App` And Emit Server-Owned Snapshots

**Files:**
- Modify: `src/app.rs`
- Modify: `src/server/runtime.rs`
- Modify: `src/shared/render.rs`
- Modify: `tests/detach_survival.rs`

- [ ] **Step 1: Write the failing PTY-survival and snapshot emission tests**

```rust
#[test]
fn runtime_emits_snapshot_from_server_owned_terminal_state() {
    let runtime = support::runtime_with_shell_output("hello");
    let snapshot = runtime.full_snapshot();
    assert!(snapshot.panes.values().any(|pane| pane.screen.contains("hello")));
}

#[test]
fn client_disconnect_does_not_kill_session_pty() {
    let env = support::isolated_humu_home();
    let session = support::spawn_server_session_running_sleep(&env);
    support::attach_then_disconnect(&env, &session);
    assert!(support::session_process_is_alive(&env, &session));
}

#[test]
fn reattach_resizes_session_to_new_client_geometry() {
    let env = support::isolated_humu_home();
    let session = support::spawn_server_session_running_shell(&env);
    support::attach_with_size(&env, &session, 80, 24);
    support::detach_client(&env, &session);
    support::attach_with_size(&env, &session, 120, 40);
    assert_eq!(support::session_size(&env, &session), (120, 40));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test detach_survival -- --nocapture`
Expected: FAIL because PTYs are still tied to `App`

- [ ] **Step 3: Move pane lifecycle, resize, input, exit cleanup, and snapshot generation into `SessionRuntime`**

```rust
impl SessionRuntime {
    pub fn send_input(&mut self, pane_id: PaneId, bytes: &[u8]) -> Result<()> { /* ... */ }
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> { /* ... */ }
    pub fn full_snapshot(&self) -> FullSnapshot { /* ... */ }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test detach_survival -- --nocapture`
Expected: PASS for PTY survival, resize-on-reattach, and server-owned snapshot generation

- [ ] **Step 5: Commit**

```bash
git add src/app.rs src/server/runtime.rs src/shared/render.rs tests/detach_survival.rs
git commit -m "refactor: move pty ownership into daemon sessions"
```

## Task 7: Build An Attachable Client Backed By Server Snapshots

**Files:**
- Create: `src/client/mod.rs`
- Create: `src/client/attach.rs`
- Create: `src/client/state.rs`
- Create: `src/client/tui_app.rs`
- Modify: `src/lib.rs`
- Modify: `tests/server_attach.rs`

- [ ] **Step 1: Write the failing attach-client tests**

```rust
#[test]
fn attach_receives_full_snapshot_and_streams_updates() {
    let env = support::isolated_humu_home();
    support::spawn_humu_server(&env);
    let mut client = support::attach_client(&env, "default");
    assert!(client.received_full_snapshot());
    assert!(client.subscribed_to_updates());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test server_attach attach_receives_full_snapshot_and_streams_updates -- --nocapture`
Expected: FAIL with missing attach client implementation

- [ ] **Step 3: Implement discovery, auto-launch, list/attach/detach commands in `client/attach.rs`**

```rust
pub fn ensure_server_running() -> Result<ServerEndpoint> { /* ping, stale cleanup, spawn */ }
pub fn attach(session: &str) -> Result<()> { /* connect, attach, subscribe */ }
```

- [ ] **Step 4: Implement `ClientState` and `TuiApp` to render `FullSnapshot` plus incremental events**

```rust
impl ClientState {
    pub fn from_snapshot(snapshot: FullSnapshot) -> Self { /* ... */ }
    pub fn apply(&mut self, event: ServerEvent) { /* ... */ }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test server_attach -- --nocapture`
Expected: PASS for attach, list-sessions, and update streaming

- [ ] **Step 6: Commit**

```bash
git add src/client/mod.rs src/client/attach.rs src/client/state.rs src/client/tui_app.rs src/lib.rs tests/server_attach.rs
git commit -m "feat: add attachable session client"
```

## Task 8: Switch Default `humu` To Daemon Discovery And Attach

**Files:**
- Modify: `src/main.rs`
- Modify: `src/client/attach.rs`
- Modify: `tests/server_attach.rs`

- [ ] **Step 1: Write the failing default-entry tests**

```rust
#[test]
fn bare_humu_autostarts_daemon_and_attaches_default_session() {
    let env = support::isolated_humu_home();
    let status = support::run_bare_humu(&env);
    assert!(status.success());
    assert!(support::default_session_exists(&env));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test server_attach bare_humu_autostarts_daemon_and_attaches_default_session -- --nocapture`
Expected: FAIL because bare `humu` still runs the legacy single-process app

- [ ] **Step 3: Flip the default command to `attach default` and preserve `humu server` / `humu list-sessions` / `humu detach --force`**

```rust
match cli.command {
    None => client::attach::attach("default"),
    Some(Command::Attach { session }) => client::attach::attach(session.as_deref().unwrap_or("default")),
    Some(Command::ListSessions) => client::attach::list_sessions(),
    Some(Command::Detach { session, force }) => client::attach::detach(session.as_deref().unwrap_or("default"), force),
    Some(Command::Server { daemon }) => server::daemon::run(daemon),
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test server_attach -- --nocapture`
Expected: PASS for bare `humu` auto-launch and attach

- [ ] **Step 5: Commit**

```bash
git add src/main.rs src/client/attach.rs tests/server_attach.rs
git commit -m "feat: make humu attach to daemon sessions by default"
```

## Task 9: Prove Daemon Restart Cold-Restore

**Files:**
- Create: `tests/daemon_restart.rs`
- Modify: `tests/support/mod.rs`
- Modify: `src/server/persistence.rs`
- Modify: `src/server/runtime.rs`

- [ ] **Step 1: Write the failing cold-restore tests**

```rust
#[test]
fn daemon_restart_cold_restores_session_layout() {
    let env = support::isolated_humu_home();
    support::spawn_server_with_named_session(&env, "default");
    support::persist_layout_for_restart(&env, "default", "shell");
    support::kill_server(&env);
    support::spawn_humu_server(&env);
    assert!(support::restored_session_contains_preset(&env, "default", "shell"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test daemon_restart -- --nocapture`
Expected: FAIL with missing restart harness or cold-restore path

- [ ] **Step 3: Implement restart harness and cold-restore assertions**

```rust
pub fn daemon_restart_round_trip(env: &TestEnv) -> RestartResult { /* ... */ }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test daemon_restart -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add tests/daemon_restart.rs tests/support/mod.rs src/server/persistence.rs src/server/runtime.rs
git commit -m "test: verify daemon restart cold restore"
```

## Task 10: Keep Floating Editor And Diff Panes Client-Local

**Files:**
- Modify: `src/app.rs`
- Modify: `src/client/tui_app.rs`
- Modify: `tests/detach_survival.rs`

- [ ] **Step 1: Write the failing floating-pane detach test**

```rust
#[test]
fn floating_editor_exits_when_client_detaches() {
    let env = support::isolated_humu_home();
    let client = support::spawn_client_with_floating_editor(&env);
    client.detach();
    assert!(support::floating_editor_exited(&env));
}

#[test]
fn diff_popup_exits_when_client_detaches() {
    let env = support::isolated_humu_home();
    let client = support::spawn_client_with_diff_popup(&env);
    client.detach();
    assert!(support::diff_popup_exited(&env));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test detach_survival floating_editor_exits_when_client_detaches -- --nocapture`
Expected: FAIL because popups still share session pane ownership

- [ ] **Step 3: Move popup PTYs into client-local state**

```rust
enum ClientPopup {
    FloatingPty { title: String, pty: ClientOnlyPty },
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test detach_survival floating_editor_exits_when_client_detaches -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/app.rs src/client/tui_app.rs tests/detach_survival.rs
git commit -m "refactor: keep floating panes client-local"
```

## Task 11: Update PRDs And Run Final Verification

**Files:**
- Modify: `docs/PRDs/002-architecture.md`
- Modify: `docs/PRDs/003-tui-layout.md`
- Modify: `docs/PRDs/006-notifications.md`

- [ ] **Step 1: Update architecture and behavior PRDs to match implementation**

```markdown
- daemon + client replace the old single-process architecture
- layout persistence is session-scoped
- detached sessions count as unfocused for notifications
```

- [ ] **Step 2: Run targeted acceptance tests**

Run: `cargo test --test session_persistence -- --nocapture`
Expected: PASS

Run: `cargo test --test server_attach -- --nocapture`
Expected: PASS

Run: `cargo test --test detach_survival -- --nocapture`
Expected: PASS

Run: `cargo test --test notifications_focus -- --nocapture`
Expected: PASS

Run: `cargo test --test daemon_restart -- --nocapture`
Expected: PASS

- [ ] **Step 3: Run the full suite**

Run: `cargo test`
Expected: PASS

- [ ] **Step 4: Manual smoke-test the core acceptance criteria**

Run: `cargo run --`
Expected: daemon auto-starts and client attaches to `default`

Run: close the client terminal
Expected: daemon remains alive and session PTY keeps running

Run: `cargo run --`
Expected: reattach to the existing `default` session with preserved output

Run: `cargo run -- list-sessions`
Expected: session list shows `default` plus attachment state

Run: kill the daemon, then `cargo run --`
Expected: daemon cold-restores from persisted session layout

- [ ] **Step 5: Commit**

```bash
git add docs/PRDs/002-architecture.md docs/PRDs/003-tui-layout.md docs/PRDs/006-notifications.md
git commit -m "docs: update PRDs for daemon session architecture"
```
