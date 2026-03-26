use humu::codex::{CodexTracker, CodexUpdate};
use humu::config::{HumuConfig, HumuState, NotificationsConfig};
use humu::hook::http::{
    AgentState, HookEvent, HookServer, remove_hook_port_file, write_hook_port_file,
};
use humu::id::{PaneId, RoomId, WorkspaceId};
use humu::notification::{NotificationEvent, NotificationManager, SessionFocusState};
use humu::preset::resolve_preset;
use humu::pty::pane::PtyPane;
use humu::pty::terminal::{
    Color as TerminalColor, MouseProtocolEncoding, MouseProtocolMode,
};
use humu::shared::render::{
    AgentStatus, AgentSummary, ColorSnapshot, CursorSnapshot, FullSnapshot, PaneRuntimeState,
    PaneGeometrySnapshot, PaneSnapshot, SessionGeometrySnapshot, TabSnapshot,
    TerminalCapabilitiesSnapshot, TerminalCellSnapshot, TerminalScreenSnapshot,
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
    started_at: SystemTime,
    session_id: Option<String>,
}

struct RuntimePane {
    pane: PtyPane,
    preset_name: String,
    cwd: Option<PathBuf>,
    started_at: SystemTime,
    session_id: Option<String>,
}

struct SessionRuntimeState {
    state_path: PathBuf,
    config: HumuConfig,
    notification_manager: NotificationManager,
    codex_tracker: CodexTracker,
    focus_by_session: HashMap<String, SessionFocusState>,
    pane_sessions: HashMap<PaneId, String>,
    panes_by_session: HashMap<String, HashMap<PaneId, RegisteredPane>>,
    runtime_panes_by_session: HashMap<String, HashMap<PaneId, RuntimePane>>,
    session_geometry_by_name: HashMap<String, SessionGeometrySnapshot>,
    agent_states: HashMap<PaneId, AgentStateEntry>,
    recorded_updates: Vec<RuntimeUpdateRecord>,
}

impl SessionRuntimeState {
    fn new(
        state_path: PathBuf,
        config: HumuConfig,
        notifications: NotificationsConfig,
        codex_sessions_root: PathBuf,
    ) -> Self {
        Self {
            state_path,
            config,
            notification_manager: NotificationManager::from_config(&notifications),
            codex_tracker: CodexTracker::new(codex_sessions_root),
            focus_by_session: HashMap::new(),
            pane_sessions: HashMap::new(),
            panes_by_session: HashMap::new(),
            runtime_panes_by_session: HashMap::new(),
            session_geometry_by_name: HashMap::new(),
            agent_states: HashMap::new(),
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
        let pane_ids = self.runtime_panes_by_session.get(session_name).map_or_else(
            Vec::new,
            |panes| panes.keys().copied().collect::<Vec<_>>(),
        );
        for pane_id in pane_ids {
            self.remove_pane(pane_id);
        }
        self.session_geometry_by_name.remove(session_name);
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
    ) -> anyhow::Result<()> {
        self.pane_sessions
            .insert(pane_id, session_name.to_string());
        self.panes_by_session
            .entry(session_name.to_string())
            .or_default()
            .insert(
                pane_id,
                RegisteredPane {
                    preset_name: preset_name.to_string(),
                    cwd: cwd.clone(),
                    started_at,
                    session_id: agent_session_id.clone(),
                },
            );

        let session_size = self
            .session_geometry_by_name
            .get(session_name)
            .cloned()
            .unwrap_or(SessionGeometrySnapshot { cols: 80, rows: 24 });
        let session_panes = self
            .runtime_panes_by_session
            .entry(session_name.to_string())
            .or_default();
        if session_panes.contains_key(&pane_id) {
            return Ok(());
        }
        self.session_geometry_by_name
            .entry(session_name.to_string())
            .or_insert(session_size.clone());

        let preset = self
            .config
            .presets
            .get(preset_name)
            .or_else(|| self.config.presets.get("shell"))
            .ok_or_else(|| anyhow::anyhow!("unknown preset: {preset_name}"))?;
        let (command, args) = resolve_preset(
            &preset.command,
            &preset.args.iter().map(|arg| arg.as_str()).collect::<Vec<_>>(),
        );
        let pane = PtyPane::spawn_with_envs(
            &command,
            &args,
            cwd.as_deref(),
            session_size.cols,
            session_size.rows,
            &[],
        )?;
        session_panes.insert(
            pane_id,
            RuntimePane {
                pane,
                preset_name: preset_name.to_string(),
                cwd: cwd.clone(),
                started_at,
                session_id: agent_session_id.clone(),
            },
        );

        if preset_name == "codex"
            && let Some(cwd) = cwd
        {
            self.codex_tracker
                .track_pane(pane_id, cwd, agent_session_id, started_at);
        }

        Ok(())
    }

    fn send_input(&mut self, session_name: &str, pane_id: PaneId, bytes: &[u8]) -> anyhow::Result<()> {
        let Some(session_panes) = self.runtime_panes_by_session.get_mut(session_name) else {
            return Err(anyhow::anyhow!("unknown session: {session_name}"));
        };
        let Some(runtime_pane) = session_panes.get_mut(&pane_id) else {
            return Err(anyhow::anyhow!("unknown pane: {pane_id}"));
        };
        runtime_pane.pane.write_input(bytes)?;
        Ok(())
    }

    fn resize_session(&mut self, session_name: &str, cols: u16, rows: u16) -> anyhow::Result<()> {
        self.session_geometry_by_name.insert(
            session_name.to_string(),
            SessionGeometrySnapshot { cols, rows },
        );
        let Some(session_panes) = self.runtime_panes_by_session.get_mut(session_name) else {
            return Ok(());
        };
        for runtime_pane in session_panes.values_mut() {
            runtime_pane.pane.resize(cols, rows)?;
        }
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn remove_pane(&mut self, pane_id: PaneId) {
        if let Some(session_name) = self.pane_sessions.remove(&pane_id) {
            if let Some(panes) = self.panes_by_session.get_mut(&session_name) {
                panes.remove(&pane_id);
                if panes.is_empty() {
                    self.panes_by_session.remove(&session_name);
                }
            }
            if let Some(runtime_panes) = self.runtime_panes_by_session.get_mut(&session_name)
                && let Some(mut runtime_pane) = runtime_panes.remove(&pane_id)
            {
                if runtime_pane.pane.exit_status().is_none() {
                    let _ = runtime_pane.pane.kill();
                }
                if runtime_panes.is_empty() {
                    self.runtime_panes_by_session.remove(&session_name);
                }
            }
        }
        self.agent_states.remove(&pane_id);
        self.codex_tracker.remove_pane(pane_id);
    }

    fn pane_session_name(&self, pane_id: PaneId) -> Option<String> {
        self.pane_sessions.get(&pane_id).cloned()
    }

    fn process_live_panes(&mut self) {
        for runtime_panes in self.runtime_panes_by_session.values_mut() {
            for runtime_pane in runtime_panes.values_mut() {
                let _ = runtime_pane.pane.process_output();
            }
        }
    }

    fn cleanup_exited_panes(&mut self) {
        let pane_ids = self
            .runtime_panes_by_session
            .values_mut()
            .flat_map(|panes| {
                panes.iter_mut().filter_map(|(pane_id, runtime_pane)| {
                    runtime_pane.pane.exit_status().is_some().then_some(*pane_id)
                })
            })
            .collect::<Vec<_>>();

        for pane_id in pane_ids {
            self.remove_pane(pane_id);
        }
    }

    fn snapshot_for_session(&mut self, session_name: &str, mut base: FullSnapshot) -> FullSnapshot {
        self.process_live_panes();
        self.cleanup_exited_panes();
        let Some(panes) = self.runtime_panes_by_session.get_mut(session_name) else {
            return base;
        };

        let mut pane_ids = panes.keys().copied().collect::<Vec<_>>();
        pane_ids.sort_by_key(|pane_id| pane_id.to_string());

        let mut pane_snapshots = HashMap::new();
        for pane_id in &pane_ids {
            let Some(pane) = panes.get_mut(pane_id) else {
                continue;
            };
            let screen = pane.pane.screen_snapshot();
            let (rows, cols) = screen.size();
            let (cursor_row, cursor_col) = screen.cursor_position();
            let cells = (0..rows)
                .map(|row| {
                    (0..cols)
                        .map(|col| {
                            let cell = screen.cell(row, col);
                            TerminalCellSnapshot {
                                text: cell.map(|cell| cell.contents()).unwrap_or_default(),
                                fg: cell.map(|cell| color_to_snapshot(cell.fgcolor())).unwrap_or(ColorSnapshot::Default),
                                bg: cell.map(|cell| color_to_snapshot(cell.bgcolor())).unwrap_or(ColorSnapshot::Default),
                                bold: cell.map(|cell| cell.bold()).unwrap_or(false),
                                dim: cell.map(|cell| cell.dim()).unwrap_or(false),
                                italic: cell.map(|cell| cell.italic()).unwrap_or(false),
                                underline: cell.map(|cell| cell.underline()).unwrap_or(false),
                                inverse: cell.map(|cell| cell.inverse()).unwrap_or(false),
                                hidden: cell.map(|cell| cell.hidden()).unwrap_or(false),
                                strike: cell.map(|cell| cell.strike()).unwrap_or(false),
                                wide: cell.map(|cell| cell.is_wide()).unwrap_or(false),
                                wide_continuation: cell
                                    .map(|cell| cell.is_wide_continuation())
                                    .unwrap_or(false),
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            pane_snapshots.insert(
                *pane_id,
                PaneSnapshot {
                    geometry: Some(PaneGeometrySnapshot {
                        x: 0,
                        y: 0,
                        width: cols,
                        height: rows,
                    }),
                    state: match pane.pane.exit_status() {
                        Some(exit_code) => PaneRuntimeState::Exited {
                            exit_code: Some(exit_code),
                        },
                        None => PaneRuntimeState::Running,
                    },
                    screen: TerminalScreenSnapshot {
                        rows,
                        cols,
                        cursor: CursorSnapshot {
                            row: cursor_row,
                            col: cursor_col,
                            visible: !screen.hide_cursor(),
                        },
                        cells,
                        title: if screen.title().is_empty() {
                            pane.preset_name.clone()
                        } else {
                            screen.title().to_string()
                        },
                    },
                    preset_name: pane.preset_name.clone(),
                    capabilities: TerminalCapabilitiesSnapshot {
                        alternate_screen: screen.alternate_screen(),
                        bracketed_paste: screen.bracketed_paste(),
                        mouse_protocol_mode: Some(match screen.mouse_protocol_mode() {
                            MouseProtocolMode::None => {
                                humu::shared::render::MouseProtocolModeSnapshot::None
                            }
                            MouseProtocolMode::Press => {
                                humu::shared::render::MouseProtocolModeSnapshot::Press
                            }
                            MouseProtocolMode::PressRelease => {
                                humu::shared::render::MouseProtocolModeSnapshot::PressRelease
                            }
                            MouseProtocolMode::ButtonMotion => {
                                humu::shared::render::MouseProtocolModeSnapshot::ButtonMotion
                            }
                            MouseProtocolMode::AnyMotion => {
                                humu::shared::render::MouseProtocolModeSnapshot::AnyMotion
                            }
                        }),
                        mouse_protocol_encoding: Some(match screen.mouse_protocol_encoding() {
                            MouseProtocolEncoding::Default => {
                                humu::shared::render::MouseProtocolEncodingSnapshot::Default
                            }
                            MouseProtocolEncoding::Utf8 => {
                                humu::shared::render::MouseProtocolEncodingSnapshot::Utf8
                            }
                            MouseProtocolEncoding::Sgr => {
                                humu::shared::render::MouseProtocolEncodingSnapshot::Sgr
                            }
                        }),
                        scrollback_offset: screen.scrollback(),
                    },
                    agent_state: pane.session_id.as_ref().map(|_| AgentSummary {
                        status: self
                            .agent_states
                            .get(pane_id)
                            .map(|state| match state.state {
                                AgentState::Working => AgentStatus::Working,
                                AgentState::NeedsInput => AgentStatus::NeedsInput,
                                AgentState::Idle => AgentStatus::Idle,
                            })
                            .unwrap_or(AgentStatus::Idle),
                        session_id: self
                            .agent_states
                            .get(pane_id)
                            .and_then(|state| state.session_id.clone())
                            .or_else(|| pane.session_id.clone()),
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
        if let Some(session_geometry) = self.session_geometry_by_name.get(session_name).cloned() {
            base.session_geometry = Some(session_geometry);
        }
        if base.explorer_root.is_none() {
            base.explorer_root = panes.values().find_map(|pane| pane.cwd.clone());
        }
        base.session_name = session_name.to_string();
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

fn color_to_snapshot(color: TerminalColor) -> ColorSnapshot {
    match color {
        TerminalColor::Default => ColorSnapshot::Default,
        TerminalColor::Idx(idx) => ColorSnapshot::Idx(idx),
        TerminalColor::Rgb(r, g, b) => ColorSnapshot::Rgb(r, g, b),
    }
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
        config: HumuConfig,
        notifications: NotificationsConfig,
        codex_sessions_root: PathBuf,
    ) -> anyhow::Result<Self> {
        let state = Arc::new(Mutex::new(SessionRuntimeState::new(
            base_dir.join("state.yaml"),
            config,
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
                    state.process_live_panes();
                    state.cleanup_exited_panes();
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
    ) -> anyhow::Result<()> {
        let mut state = self.state.lock().expect("session runtime state lock");
        state.register_pane(
            session_name,
            pane_id,
            preset_name,
            cwd,
            agent_session_id,
            started_at,
        )
    }

    pub fn send_input(
        &self,
        session_name: &str,
        pane_id: PaneId,
        bytes: &[u8],
    ) -> anyhow::Result<()> {
        let mut state = self.state.lock().expect("session runtime state lock");
        state.send_input(session_name, pane_id, bytes)
    }

    pub fn resize_session(&self, session_name: &str, cols: u16, rows: u16) -> anyhow::Result<()> {
        let mut state = self.state.lock().expect("session runtime state lock");
        state.resize_session(session_name, cols, rows)
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
        let mut state = self.state.lock().expect("session runtime state lock");
        state.snapshot_for_session(session_name, base)
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
