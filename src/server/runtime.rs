use humu::codex::{CodexTracker, CodexUpdate};
use humu::config::{HumuState, NotificationsConfig};
use humu::hook::http::{
    AgentState, HookEvent, HookServer, remove_hook_port_file, write_hook_port_file,
};
use humu::id::{PaneId, RoomId, WorkspaceId};
use humu::notification::{NotificationEvent, NotificationManager, SessionFocusState};
use humu::shared::render::{
    AgentStatus, AgentSummary, ColorSnapshot, CursorSnapshot, FullSnapshot, PaneRuntimeState,
    PaneSnapshot, TabSnapshot, TerminalCapabilitiesSnapshot, TerminalCellSnapshot,
    TerminalScreenSnapshot,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};
use tokio::sync::oneshot;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeUpdateSource {
    Hook,
    Codex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeUpdateRecord {
    pub source: RuntimeUpdateSource,
    pub pane_id: PaneId,
    pub state: AgentState,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone)]
struct AgentStateEntry {
    state: AgentState,
    session_id: Option<String>,
}

#[derive(Debug, Clone)]
struct RegisteredPane {
    preset_name: String,
    cwd: Option<PathBuf>,
}

struct SessionRuntimeState {
    state_path: PathBuf,
    notification_manager: NotificationManager,
    codex_tracker: CodexTracker,
    focus_by_session: HashMap<String, SessionFocusState>,
    pane_sessions: HashMap<PaneId, String>,
    panes_by_session: HashMap<String, HashMap<PaneId, RegisteredPane>>,
    agent_states: HashMap<PaneId, AgentStateEntry>,
    session_snapshots: HashMap<String, FullSnapshot>,
    recorded_updates: Vec<RuntimeUpdateRecord>,
}

impl SessionRuntimeState {
    fn new(
        state_path: PathBuf,
        notifications: NotificationsConfig,
        codex_sessions_root: PathBuf,
    ) -> Self {
        Self {
            state_path,
            notification_manager: NotificationManager::from_config(&notifications),
            codex_tracker: CodexTracker::new(codex_sessions_root),
            focus_by_session: HashMap::new(),
            pane_sessions: HashMap::new(),
            panes_by_session: HashMap::new(),
            agent_states: HashMap::new(),
            session_snapshots: HashMap::new(),
            recorded_updates: Vec::new(),
        }
    }

    fn focus_for_session(&self, session_name: &str) -> SessionFocusState {
        self.focus_by_session
            .get(session_name)
            .copied()
            .unwrap_or_default()
    }

    fn attach_session(&mut self, session_name: &str) {
        self.focus_by_session
            .insert(session_name.to_string(), SessionFocusState::attached());
    }

    fn detach_session(&mut self, session_name: &str) {
        self.focus_by_session
            .insert(session_name.to_string(), SessionFocusState::detached());
    }

    fn clear_session_panes(&mut self, session_name: &str) {
        let pane_ids = self
            .panes_by_session
            .get(session_name)
            .map(|panes| panes.keys().copied().collect::<Vec<_>>())
            .unwrap_or_default();
        for pane_id in pane_ids {
            self.remove_pane(pane_id);
        }
        self.session_snapshots.remove(session_name);
    }

    fn update_session_focus(&mut self, session_name: &str, focused: bool) {
        let state = self
            .focus_by_session
            .entry(session_name.to_string())
            .or_default();
        state.update_client_focus(focused);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn register_pane(
        &mut self,
        session_name: &str,
        pane_id: PaneId,
        preset_name: &str,
        cwd: Option<PathBuf>,
        agent_session_id: Option<String>,
        started_at: SystemTime,
    ) {
        self.pane_sessions.insert(pane_id, session_name.to_string());
        self.panes_by_session
            .entry(session_name.to_string())
            .or_default()
            .insert(
                pane_id,
                RegisteredPane {
                    preset_name: preset_name.to_string(),
                    cwd: cwd.clone(),
                },
            );
        if preset_name == "codex"
            && let Some(cwd) = cwd
        {
            self.codex_tracker
                .track_pane(pane_id, cwd, agent_session_id, started_at);
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn remove_pane(&mut self, pane_id: PaneId) {
        if let Some(session_name) = self.pane_sessions.remove(&pane_id)
            && let Some(panes) = self.panes_by_session.get_mut(&session_name)
        {
            panes.remove(&pane_id);
            if panes.is_empty() {
                self.panes_by_session.remove(&session_name);
            }
        }
        self.agent_states.remove(&pane_id);
        self.codex_tracker.remove_pane(pane_id);
    }

    fn pane_session_name(&self, pane_id: PaneId) -> Option<String> {
        self.pane_sessions.get(&pane_id).cloned()
    }

    fn snapshot_for_session(&self, session_name: &str, mut base: FullSnapshot) -> FullSnapshot {
        if let Some(snapshot) = self.session_snapshots.get(session_name) {
            let mut snapshot = snapshot.clone();
            snapshot.session_name = session_name.to_string();
            return snapshot;
        }

        let Some(panes) = self.panes_by_session.get(session_name) else {
            return base;
        };

        let mut pane_ids = panes.keys().copied().collect::<Vec<_>>();
        pane_ids.sort_by_key(|pane_id| pane_id.to_string());

        let mut pane_snapshots = HashMap::new();
        for pane_id in &pane_ids {
            let Some(pane) = panes.get(pane_id) else {
                continue;
            };
            pane_snapshots.insert(
                *pane_id,
                PaneSnapshot {
                    geometry: None,
                    state: PaneRuntimeState::Running,
                    screen: TerminalScreenSnapshot {
                        rows: 24,
                        cols: 80,
                        cursor: CursorSnapshot {
                            row: 0,
                            col: 0,
                            visible: false,
                        },
                        cells: vec![vec![TerminalCellSnapshot {
                            text: String::new(),
                            fg: ColorSnapshot::Default,
                            bg: ColorSnapshot::Default,
                            bold: false,
                            dim: false,
                            italic: false,
                            underline: false,
                            inverse: false,
                            hidden: false,
                            strike: false,
                            wide: false,
                            wide_continuation: false,
                        }]],
                        title: pane.preset_name.clone(),
                    },
                    preset_name: pane.preset_name.clone(),
                    capabilities: TerminalCapabilitiesSnapshot {
                        alternate_screen: false,
                        bracketed_paste: false,
                        mouse_protocol_mode: None,
                        mouse_protocol_encoding: None,
                        scrollback_offset: 0,
                    },
                    agent_state: self.agent_states.get(pane_id).map(|state| AgentSummary {
                        status: match state.state {
                            AgentState::Working => AgentStatus::Working,
                            AgentState::NeedsInput => AgentStatus::NeedsInput,
                            AgentState::Idle => AgentStatus::Idle,
                        },
                        session_id: state.session_id.clone(),
                    }),
                },
            );
        }

        base.tabs = if pane_ids.is_empty() {
            Vec::new()
        } else {
            vec![TabSnapshot {
                tab_id: None,
                name: "runtime".to_string(),
                pane_ids: pane_ids.clone(),
            }]
        };
        base.active_tab_index = (!pane_ids.is_empty()).then_some(0);
        base.split_tree = None;
        base.focused_pane_id = pane_ids.first().copied();
        base.fullscreen_pane_id = None;
        base.panes = pane_snapshots;
        if base.explorer_root.is_none() {
            base.explorer_root = panes.values().find_map(|pane| pane.cwd.clone());
        }
        base
    }

    fn apply_hook_event(&mut self, event: HookEvent) {
        let pane_id = event.pane_id;
        let prev_state = self
            .agent_states
            .get(&pane_id)
            .map(|entry| entry.state.clone());
        let session_id = event.session_id.clone().or_else(|| {
            self.agent_states
                .get(&pane_id)
                .and_then(|entry| entry.session_id.clone())
        });

        self.agent_states.insert(
            pane_id,
            AgentStateEntry {
                state: event.event_type.clone(),
                session_id: session_id.clone(),
            },
        );
        self.recorded_updates.push(RuntimeUpdateRecord {
            source: RuntimeUpdateSource::Hook,
            pane_id,
            state: event.event_type.clone(),
            session_id,
        });

        let should_notify = matches!(
            (&prev_state, &event.event_type),
            (Some(AgentState::Working), AgentState::NeedsInput)
                | (Some(AgentState::Working), AgentState::Idle)
        );
        if !should_notify {
            return;
        }

        let (workspace_name, room_name) = self.resolve_notification_names(&event);
        let notification_event = match event.event_type {
            AgentState::NeedsInput => NotificationEvent::AgentNeedsInput {
                workspace: workspace_name,
                room: room_name,
            },
            AgentState::Idle => NotificationEvent::AgentFinished {
                workspace: workspace_name,
                room: room_name,
            },
            AgentState::Working => return,
        };

        let focus_state = self
            .pane_sessions
            .get(&pane_id)
            .map(|session_name| self.focus_for_session(session_name))
            .unwrap_or_default();
        self.notification_manager
            .notify_with_session_focus(notification_event, focus_state);
    }

    fn apply_codex_update(&mut self, update: CodexUpdate) {
        let existing_session_id = self
            .agent_states
            .get(&update.pane_id)
            .and_then(|entry| entry.session_id.clone());
        let session_id = update.session_id.clone().or(existing_session_id);

        self.agent_states.insert(
            update.pane_id,
            AgentStateEntry {
                state: update.state.clone(),
                session_id: session_id.clone(),
            },
        );
        self.recorded_updates.push(RuntimeUpdateRecord {
            source: RuntimeUpdateSource::Codex,
            pane_id: update.pane_id,
            state: update.state,
            session_id,
        });
    }

    fn resolve_notification_names(&self, event: &HookEvent) -> (String, String) {
        let Some(state) = self
            .state_path
            .exists()
            .then(|| HumuState::load(&self.state_path).ok())
            .flatten()
        else {
            return (
                event
                    .workspace_id
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                event
                    .room_id
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
            );
        };

        let workspace = event
            .workspace_id
            .as_deref()
            .and_then(parse_workspace_id)
            .and_then(|workspace_id| state.ws_by_id(workspace_id).map(|ws| ws.name.clone()))
            .or_else(|| event.workspace_id.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let room = event
            .room_id
            .as_deref()
            .and_then(parse_room_id)
            .and_then(|room_id| {
                state.workspaces.iter().find_map(|workspace| {
                    workspace.room_by_id(room_id).map(|room| room.name.clone())
                })
            })
            .or_else(|| event.room_id.clone())
            .unwrap_or_else(|| "unknown".to_string());
        (workspace, room)
    }
}

fn parse_workspace_id(raw: &str) -> Option<WorkspaceId> {
    Uuid::parse_str(raw).ok().map(WorkspaceId)
}

fn parse_room_id(raw: &str) -> Option<RoomId> {
    Uuid::parse_str(raw).ok().map(RoomId)
}

pub struct SessionRuntime {
    base_dir: PathBuf,
    #[cfg_attr(not(test), allow(dead_code))]
    hook_port: u16,
    state: Arc<Mutex<SessionRuntimeState>>,
    shutdown: Arc<AtomicBool>,
    hook_shutdown: Option<oneshot::Sender<()>>,
    hook_thread: Option<JoinHandle<()>>,
    worker_thread: Option<JoinHandle<()>>,
}

impl SessionRuntime {
    pub fn start(
        base_dir: PathBuf,
        notifications: NotificationsConfig,
        codex_sessions_root: PathBuf,
    ) -> anyhow::Result<Self> {
        let state = Arc::new(Mutex::new(SessionRuntimeState::new(
            base_dir.join("state.yaml"),
            notifications,
            codex_sessions_root,
        )));
        let (hook_tx, hook_rx) = mpsc::channel::<HookEvent>();
        let (port_tx, port_rx) = mpsc::channel::<anyhow::Result<u16>>();
        let (hook_shutdown, hook_shutdown_rx) = oneshot::channel::<()>();
        let hook_thread = thread::spawn(move || {
            let runtime = match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime,
                Err(err) => {
                    let _ = port_tx.send(Err(err.into()));
                    return;
                }
            };
            runtime.block_on(async move {
                let server = match HookServer::start().await {
                    Ok(server) => server,
                    Err(err) => {
                        let _ = port_tx.send(Err(err));
                        return;
                    }
                };
                let _ = port_tx.send(Ok(server.port()));

                let mut events = server.subscribe();
                let mut shutdown_rx = hook_shutdown_rx;
                loop {
                    tokio::select! {
                        _ = &mut shutdown_rx => break,
                        result = events.recv() => match result {
                            Ok(event) => {
                                if hook_tx.send(event).is_err() {
                                    break;
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
            });
        });

        let hook_port = port_rx
            .recv()
            .map_err(|_| anyhow::anyhow!("hook server thread exited before publishing a port"))??;
        write_hook_port_file(&base_dir, hook_port)?;

        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);
        let worker_state = Arc::clone(&state);
        let worker_thread = thread::spawn(move || {
            while !worker_shutdown.load(Ordering::Relaxed) {
                while let Ok(event) = hook_rx.try_recv() {
                    if let Ok(mut state) = worker_state.lock() {
                        state.apply_hook_event(event);
                    }
                }
                if let Ok(mut state) = worker_state.lock() {
                    for update in state.codex_tracker.poll() {
                        state.apply_codex_update(update);
                    }
                }
                thread::sleep(Duration::from_millis(50));
            }
        });

        Ok(Self {
            base_dir,
            hook_port,
            state,
            shutdown,
            hook_shutdown: Some(hook_shutdown),
            hook_thread: Some(hook_thread),
            worker_thread: Some(worker_thread),
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn hook_port(&self) -> u16 {
        self.hook_port
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn session_focus(&self, session_name: &str) -> SessionFocusState {
        self.state
            .lock()
            .expect("session runtime state lock")
            .focus_for_session(session_name)
    }

    pub fn attach_session(&self, session_name: &str) {
        self.state
            .lock()
            .expect("session runtime state lock")
            .attach_session(session_name);
    }

    pub fn detach_session(&self, session_name: &str) {
        self.state
            .lock()
            .expect("session runtime state lock")
            .detach_session(session_name);
    }

    pub fn clear_session_panes(&self, session_name: &str) {
        self.state
            .lock()
            .expect("session runtime state lock")
            .clear_session_panes(session_name);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_session_snapshot(&self, session_name: &str, snapshot: FullSnapshot) {
        self.state
            .lock()
            .expect("session runtime state lock")
            .session_snapshots
            .insert(session_name.to_string(), snapshot);
    }

    pub fn update_session_focus(&self, session_name: &str, focused: bool) {
        self.state
            .lock()
            .expect("session runtime state lock")
            .update_session_focus(session_name, focused);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn register_pane(
        &self,
        session_name: &str,
        pane_id: PaneId,
        preset_name: &str,
        cwd: Option<PathBuf>,
        agent_session_id: Option<String>,
        started_at: SystemTime,
    ) {
        self.state
            .lock()
            .expect("session runtime state lock")
            .register_pane(
                session_name,
                pane_id,
                preset_name,
                cwd,
                agent_session_id,
                started_at,
            );
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn remove_pane(&self, pane_id: PaneId) {
        self.state
            .lock()
            .expect("session runtime state lock")
            .remove_pane(pane_id);
    }

    pub fn pane_session_name(&self, pane_id: PaneId) -> Option<String> {
        self.state
            .lock()
            .expect("session runtime state lock")
            .pane_session_name(pane_id)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn recorded_updates(&self) -> Vec<RuntimeUpdateRecord> {
        self.state
            .lock()
            .expect("session runtime state lock")
            .recorded_updates
            .clone()
    }

    pub fn snapshot_for_session(&self, session_name: &str, base: FullSnapshot) -> FullSnapshot {
        self.state
            .lock()
            .expect("session runtime state lock")
            .snapshot_for_session(session_name, base)
    }
}

impl Drop for SessionRuntime {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(tx) = self.hook_shutdown.take() {
            let _ = tx.send(());
        }
        let _ = remove_hook_port_file(&self.base_dir);
        if let Some(thread) = self.worker_thread.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.hook_thread.take() {
            let _ = thread.join();
        }
    }
}
