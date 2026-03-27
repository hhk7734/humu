use humu::codex::{CodexTracker, CodexUpdate};
use humu::config::{HumuConfig, HumuState, NotificationsConfig, PersistedRoomLayout, SplitNode};
use humu::hook::http::{
    AgentState, HookEvent, HookServer, remove_hook_port_file, write_hook_port_file,
};
use humu::id::{PaneId, RoomId, TabId, WorkspaceId};
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
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};
use tokio::sync::oneshot;
use uuid::Uuid;

#[path = "persistence.rs"]
mod persistence;

const PRESET_CLAUDE: &str = "claude";
const PRESET_GEMINI: &str = "gemini";
const PRESET_CODEX: &str = "codex";

fn append_codex_args(args: &mut Vec<String>, session_id: Option<String>) {
    if let Some(session_id) = session_id {
        args.push("resume".to_string());
        args.push(session_id);
    }
}

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
    base_dir: PathBuf,
    state_path: PathBuf,
    hook_port: Option<u16>,
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
    pending_cold_restores: HashSet<String>,
}

impl SessionRuntimeState {
    fn pending_cold_restores(state_path: &std::path::Path) -> HashSet<String> {
        HumuState::load(state_path)
            .ok()
            .map(|state| {
                state.sessions
                    .iter()
                    .filter(|session| {
                        session
                            .active_room_id
                            .and_then(|room_id| session.tabs_by_room.get(&room_id))
                            .is_some_and(|layout| {
                                !layout.tabs.is_empty()
                                    && layout.tabs.iter().any(|tab| tab.name == "runtime")
                            })
                    })
                    .map(|session| session.name.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn persisted_session_layout(
        &self,
        session_name: &str,
    ) -> Option<(
        PersistedRoomLayout,
        Option<PathBuf>,
        Option<SessionGeometrySnapshot>,
        Option<WorkspaceId>,
        Option<RoomId>,
    )> {
        let state = HumuState::load(&self.state_path).ok()?;
        let session = state.session_by_name(session_name)?;
        let room_id = session.active_room_id?;
        let layout = session.tabs_by_room.get(&room_id)?.clone();
        let room_path = session.active_workspace_id.and_then(|workspace_id| {
            state
                .ws_by_id(workspace_id)
                .and_then(|workspace| workspace.room_by_id(room_id))
                .map(|room| room.path.clone())
        });
        let session_geometry = session.last_size.as_ref().map(|size| SessionGeometrySnapshot {
            cols: size.cols,
            rows: size.rows,
        });

        Some((
            layout,
            room_path,
            session_geometry,
            session.active_workspace_id,
            session.active_room_id,
        ))
    }

    fn apply_persisted_session_metadata(&self, session_name: &str, base: &mut FullSnapshot) {
        let Some((_, room_path, session_geometry, active_workspace_id, active_room_id)) =
            self.persisted_session_layout(session_name)
        else {
            return;
        };

        base.active_workspace_id = active_workspace_id;
        base.active_room_id = active_room_id;
        if base.explorer_root.is_none() {
            base.explorer_root = room_path;
        }
        if base.session_geometry.is_none() {
            base.session_geometry = session_geometry;
        }
    }

    fn restore_split_node(
        &mut self,
        session_name: &str,
        node: &SplitNode,
        cwd: Option<PathBuf>,
    ) -> anyhow::Result<()> {
        match node {
            SplitNode::Leaf { preset, session_id } => self.register_pane(
                session_name,
                PaneId::new(),
                preset,
                cwd,
                session_id.clone(),
                SystemTime::now(),
            ),
            SplitNode::Split { children, .. } => {
                for child in children {
                    self.restore_split_node(session_name, child, cwd.clone())?;
                }
                Ok(())
            }
        }
    }

    fn restore_session_from_state(&mut self, session_name: &str) -> anyhow::Result<()> {
        if !self.pending_cold_restores.contains(session_name) {
            return Ok(());
        }
        if self
            .runtime_panes_by_session
            .get(session_name)
            .is_some_and(|panes| !panes.is_empty())
        {
            return Ok(());
        }

        let Some((layout, cwd, session_geometry, _, _)) =
            self.persisted_session_layout(session_name)
        else {
            self.pending_cold_restores.remove(session_name);
            return Ok(());
        };
        self.pending_cold_restores.remove(session_name);

        if let Some(session_geometry) = session_geometry {
            self.session_geometry_by_name
                .insert(session_name.to_string(), session_geometry);
        }

        for tab in &layout.tabs {
            self.restore_split_node(session_name, &tab.split, cwd.clone())?;
        }

        Ok(())
    }

    fn session_runtime_layout(&self, session_name: &str) -> Option<PersistedRoomLayout> {
        let panes = self.panes_by_session.get(session_name)?;
        let mut panes = panes.iter().collect::<Vec<_>>();
        panes.sort_by_key(|(pane_id, _)| pane_id.to_string());

        let split = match panes.as_slice() {
            [] => return None,
            [(.., pane)] => SplitNode::Leaf {
                preset: pane.preset_name.clone(),
                session_id: pane.session_id.clone(),
            },
            _ => SplitNode::Split {
                direction: humu::config::SplitDirection::Horizontal,
                ratio: 0.5,
                children: panes
                    .into_iter()
                    .map(|(_, pane)| SplitNode::Leaf {
                        preset: pane.preset_name.clone(),
                        session_id: pane.session_id.clone(),
                    })
                    .collect(),
            },
        };

        Some(PersistedRoomLayout {
            active_tab: 0,
            tabs: vec![humu::config::TabLayout {
                name: "runtime".to_string(),
                split,
            }],
        })
    }

    fn persist_runtime_session_state(&self, session_name: &str) {
        let (active_workspace_id, active_room_id) = self
            .session_location(session_name)
            .map(|(workspace_id, room_id)| (Some(workspace_id), Some(room_id)))
            .unwrap_or((None, None));
        let layout = self.session_runtime_layout(session_name);
        let last_size = self.session_geometry_by_name.get(session_name).cloned();
        let _ = persistence::persist_session_runtime_state(
            &self.state_path,
            session_name,
            active_workspace_id,
            active_room_id,
            layout,
            last_size,
        );
    }

    fn persist_runtime_session_size(&self, session_name: &str) {
        let (active_workspace_id, active_room_id) = self
            .session_location(session_name)
            .map(|(workspace_id, room_id)| (Some(workspace_id), Some(room_id)))
            .unwrap_or((None, None));
        let last_size = self.session_geometry_by_name.get(session_name).cloned();
        let _ = persistence::persist_session_size(
            &self.state_path,
            session_name,
            active_workspace_id,
            active_room_id,
            last_size,
        );
    }

    fn pane_agent_summary(
        runtime_state: Option<&AgentStateEntry>,
        fallback_session_id: Option<&str>,
    ) -> Option<AgentSummary> {
        let session_id = runtime_state
            .and_then(|state| state.session_id.clone())
            .or_else(|| fallback_session_id.map(str::to_string));
        let status = runtime_state
            .map(|state| match state.state {
                AgentState::Working => AgentStatus::Working,
                AgentState::NeedsInput => AgentStatus::NeedsInput,
                AgentState::Idle => AgentStatus::Idle,
            })
            .unwrap_or(AgentStatus::Idle);
        session_id.map(|session_id| AgentSummary { status, session_id: Some(session_id) })
    }

    fn new(
        base_dir: PathBuf,
        state_path: PathBuf,
        config: HumuConfig,
        notifications: NotificationsConfig,
        codex_sessions_root: PathBuf,
    ) -> Self {
        let pending_cold_restores = Self::pending_cold_restores(&state_path);
        Self {
            base_dir,
            state_path,
            hook_port: None,
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
            pending_cold_restores,
        }
    }

    fn set_hook_port(&mut self, hook_port: u16) {
        self.hook_port = Some(hook_port);
    }

    fn session_location(&self, session_name: &str) -> Option<(WorkspaceId, RoomId)> {
        let state = HumuState::load(&self.state_path).ok()?;
        let session = state.session_by_name(session_name)?;
        Some((session.active_workspace_id?, session.active_room_id?))
    }

    fn preset_spawn_contract(
        &self,
        session_name: &str,
        pane_id: PaneId,
        preset_name: &str,
        agent_session_id: Option<&str>,
    ) -> (Vec<String>, Vec<(String, String)>) {
        let mut extra_args = Vec::new();
        let mut envs = Vec::new();

        match preset_name {
            PRESET_CLAUDE => {
                let settings_path = self.base_dir.join("hooks/claude-settings.json");
                extra_args.push("--settings".to_string());
                extra_args.push(settings_path.to_string_lossy().into_owned());
                if let Some(session_id) = agent_session_id {
                    extra_args.push("--resume".to_string());
                    extra_args.push(session_id.to_string());
                }
            }
            PRESET_GEMINI => {
                let settings_path = self.base_dir.join("hooks/gemini-settings.json");
                envs.push((
                    "GEMINI_CLI_SYSTEM_SETTINGS_PATH".to_string(),
                    settings_path.to_string_lossy().into_owned(),
                ));
                if let Some(session_id) = agent_session_id {
                    extra_args.push("--resume".to_string());
                    extra_args.push(session_id.to_string());
                }
            }
            PRESET_CODEX => append_codex_args(&mut extra_args, agent_session_id.map(str::to_string)),
            _ => {}
        }

        if matches!(preset_name, PRESET_CLAUDE | PRESET_GEMINI) {
            if let Some(hook_port) = self.hook_port {
                envs.push(("HUMU_PORT".to_string(), hook_port.to_string()));
            }
            if let Some((workspace_id, room_id)) = self.session_location(session_name) {
                envs.push(("HUMU_WORKSPACE_ID".to_string(), workspace_id.to_string()));
                envs.push(("HUMU_ROOM_ID".to_string(), room_id.to_string()));
            }
            envs.push(("HUMU_TAB_ID".to_string(), TabId::new().to_string()));
            envs.push(("HUMU_PANE_ID".to_string(), pane_id.to_string()));
        }

        (extra_args, envs)
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
        let session_size = self
            .session_geometry_by_name
            .get(session_name)
            .cloned()
            .unwrap_or(SessionGeometrySnapshot { cols: 80, rows: 24 });
        let (extra_args, envs) = self.preset_spawn_contract(
            session_name,
            pane_id,
            preset_name,
            agent_session_id.as_deref(),
        );
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
        let mut all_args = args;
        all_args.extend(extra_args);
        let pane = PtyPane::spawn_with_envs(
            &command,
            &all_args,
            cwd.as_deref(),
            session_size.cols,
            session_size.rows,
            &envs,
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
            && let Some(cwd) = cwd.as_ref()
        {
            self.codex_tracker
                .track_pane(pane_id, cwd.to_path_buf(), agent_session_id.clone(), started_at);
        }

        self.pane_sessions
            .insert(pane_id, session_name.to_string());
        self.panes_by_session
            .entry(session_name.to_string())
            .or_default()
            .insert(
                pane_id,
                RegisteredPane {
                    preset_name: preset_name.to_string(),
                    cwd,
                    started_at,
                    session_id: agent_session_id,
                },
            );

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
        self.apply_persisted_session_metadata(session_name, &mut base);
        let _ = self.restore_session_from_state(session_name);
        self.process_live_panes();
        self.cleanup_exited_panes();
        let Some(panes) = self.runtime_panes_by_session.get_mut(session_name) else {
            return base;
        };

        let mut pane_ids = panes.keys().copied().collect::<Vec<_>>();
        pane_ids.sort_by_key(|pane_id| pane_id.to_string());

        let mut pane_snapshots = HashMap::new();
        for pane_id in &pane_ids {
            let runtime_state = self.agent_states.get(pane_id).cloned();
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
                    agent_state: Self::pane_agent_summary(
                        runtime_state.as_ref(),
                        pane.session_id.as_deref(),
                    ),
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
            base_dir.clone(),
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
        state
            .lock()
            .expect("session runtime state lock")
            .set_hook_port(hook_port);

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
        let result = state.register_pane(
            session_name,
            pane_id,
            preset_name,
            cwd,
            agent_session_id,
            started_at,
        );
        if result.is_ok() {
            state.persist_runtime_session_state(session_name);
        }
        result
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
        let result = state.resize_session(session_name, cols, rows);
        if result.is_ok() {
            state.persist_runtime_session_size(session_name);
        }
        result
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn remove_pane(&self, pane_id: PaneId) {
        let mut state = self.state.lock().expect("session runtime state lock");
        let session_name = state.pane_session_name(pane_id);
        state.remove_pane(pane_id);
        if let Some(session_name) = session_name {
            state.persist_runtime_session_state(&session_name);
        }
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
