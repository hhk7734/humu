use anyhow::Result;
use crossterm::ExecutableCommand;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use humu::config::{
    HumuConfig, HumuState, PersistedRoomLayout, SplitDirection as CfgDir, SplitNode, TabLayout,
    WorkspaceEntry, humu_dir,
};
use humu::git::room::{RoomGitStatus, RoomManager};
use humu::git::workspace::{WorkspaceManager, default_clone_target_dir};
use humu::hook::http::AgentState;
use humu::id::{RoomId, TabId, WorkspaceId};
use humu::pty::input::{
    InputAction, InputRoute, PaneInputState, route_floating_mouse, route_mouse,
    route_passthrough, route_paste,
};
use humu::pty::pane::PtyPane;
use humu::shared::protocol::{ClientRequest, FrameDecoder, ServerResponse, encode_frame};
use humu::shared::render::{
    AgentStatus, FullSnapshot, PaneSnapshot, SplitDirectionSnapshot, SplitTreeSnapshot,
};
use humu::tui::completion::complete_path;
use humu::tui::input::{
    Action, Direction as NavDirection, Mode, handle_key, hint_click_action, hint_click_action_right,
};
use humu::tui::layout::{PaneId, SplitDirection, SplitTree, TabContainer};
use humu::tui::search::SearchState;
use humu::tui::widgets::dialog::{Dialog, DialogField};
use humu::tui::widgets::preset_selector::PresetSelector;
use humu::tui::widgets::status_bar::{self, StatusBar};
use humu::tui::widgets::terminal_area::TabBar;
use humu::tui::widgets::terminal_widget::TerminalWidget;
use humu::tui::widgets::workspace_panel::{TreeItemKind, WorkspacePanel, WorkspaceTreeItem};
use ratatui::Terminal;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use std::collections::HashMap;
use std::io::{Read, Write, stdout};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

// Built-in preset names for AI agent integrations.
const PRESET_CLAUDE: &str = "claude";
const PRESET_GEMINI: &str = "gemini";
const PRESET_CODEX: &str = "codex";
const DEFAULT_ROOM_NAME: &str = "local";

fn expand_workspace_path(path_str: &str) -> PathBuf {
    let expanded = if path_str.starts_with("~/") || path_str == "~" {
        if let Some(home) = dirs::home_dir() {
            format!("{}{}", home.display(), &path_str[1..])
        } else {
            path_str.to_string()
        }
    } else {
        path_str.to_string()
    };

    PathBuf::from(expanded.trim_end_matches('/'))
}

fn append_codex_args(args: &mut Vec<String>, session_id: Option<String>) {
    if let Some(sid) = session_id {
        args.push("resume".to_string());
        args.push(sid);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedPanel {
    Workspace,
    Terminal,
    Explorer,
}

/// Cached room info with git stats, refreshed periodically.
struct CachedRoomInfo {
    room_id: Option<RoomId>,
    name: String,
    path: PathBuf,
    git_status: RoomGitStatus,
}

/// Internal room item with resolved ID and agent activity flag.
struct CachedRoomItem {
    id: Option<RoomId>,
    name: String,
    path: PathBuf,
    active: bool,
    git_status: RoomGitStatus,
}

/// Tracks the last-rendered rects for each major panel so mouse clicks can be
/// hit-tested without re-running the layout computation.
#[derive(Debug, Clone, Copy, Default)]
pub struct PanelRects {
    pub workspace: Rect,
    pub terminal: Rect,
    pub explorer: Rect,
    pub tab_bar: Rect,
    pub status_bar: Rect,
}

/// Active text selection state for mouse drag in terminal panes.
#[derive(Debug, Clone)]
pub struct TextSelection {
    pub pane_id: PaneId,
    /// Start position in vt100 screen coordinates (row, col).
    pub start: (u16, u16),
    /// Current end position in vt100 screen coordinates (row, col).
    pub end: (u16, u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetAction {
    NewTab,
    SplitDown,
    SplitRight,
}

/// Holds room runtime state for non-attached local mode and layout metadata for
/// attached-session room switches.
pub struct RoomState {
    pub local_panes: HashMap<PaneId, PtyPane>,
    pub tabs: TabContainer,
    pub pane_presets: HashMap<PaneId, String>,
    pub focused_pane: Option<PaneId>,
    pub fullscreen_pane: Option<PaneId>,
}

pub struct AgentStateEntry {
    pub state: AgentState,
    pub session_id: Option<String>,
}

pub enum PopupState {
    None,
    Settings {
        selected: usize,
    },
    SplitDirection,
    LogViewer {
        lines: Vec<String>,
        scroll: usize,
        h_scroll: usize,
        file_len: u64,
    },
    PresetSelector {
        presets: Vec<String>,
        selected: usize,
        action: PresetAction,
    },
    WorkspaceCreate {
        /// field indices: 0=Mode, 1=Path, 2=URL
        fields: Vec<DialogField>,
        focused_field: usize,
        completions: Vec<String>,
        completion_selected: Option<usize>,
    },
    RoomCreate {
        /// field indices: 0=Branch name, 1=Base branch
        fields: Vec<DialogField>,
        focused_field: usize,
    },
    WorkspaceDelete {
        /// field index 0=Confirm (yes/no), 1=Checkbox (delete dir from disk)
        fields: Vec<DialogField>,
        focused_field: usize,
        workspace_id: WorkspaceId,
    },
    RoomDelete {
        /// field index 0=Confirm (yes/no)
        fields: Vec<DialogField>,
        focused_field: usize,
        workspace_id: WorkspaceId,
        branch: String,
    },
    NotificationSettings {
        selected: usize,
    },
    NotificationTokenInput {
        field: NotificationField,
        value: String,
    },
    FloatingPane {
        pane_id: PaneId,
        title: String,
    },
    ErrorDialog {
        message: String,
    },
    ExplorerNewEntry {
        is_dir: bool,
        value: String,
    },
    ExplorerDeleteConfirm {
        path: std::path::PathBuf,
        name: String,
        ok_selected: bool,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum NotificationField {
    BotToken,
    ChatId,
}

#[allow(dead_code)]
pub struct App {
    pub config: HumuConfig,
    pub state: HumuState,
    pub mode: Mode,
    pub focus: FocusedPanel,
    pub workspace_selected: Option<WorkspaceId>,
    pub room_selected: Option<RoomId>,
    pub running: bool,
    /// Local-only PTYs that are not daemon-owned session panes, such as
    /// floating editor/diff overlays.
    pub local_panes: HashMap<PaneId, PtyPane>,
    pub tabs: TabContainer,
    pub focused_pane: Option<PaneId>,
    /// Tracks which preset name was used to spawn each pane.
    pub pane_presets: HashMap<PaneId, String>,
    /// Active popup (None when no popup is showing).
    pub popup: PopupState,
    /// Per-pane agent state mirrored from persisted session metadata.
    pub agent_states: HashMap<PaneId, AgentStateEntry>,
    /// Port the daemon-owned HTTP hook server is listening on.
    pub hook_port: Option<u16>,
    /// Persistent daemon connection used for session ownership and pane registration.
    server_stream: Option<UnixStream>,
    /// Current attached-session view model mirrored from the daemon.
    attached_snapshot: Option<FullSnapshot>,
    /// Last-rendered panel rects used for mouse hit-testing.
    pub panel_rects: PanelRects,
    /// Panel widths: [workspace, explorer]. Used in the layout constraints.
    pub panel_widths: [u16; 2],
    /// True when a mouse-down was forwarded to the focused PTY (not humu UI).
    pub pty_mouse_active: bool,
    /// Whether the terminal window is focused (for focus-aware notifications).
    pub is_focused: bool,
    /// Active text selection in a terminal pane (when child has no mouse tracking).
    pub selection: Option<TextSelection>,
    /// When Some(id), only that pane is rendered filling the full terminal area.
    pub fullscreen_pane: Option<PaneId>,
    pub palette: humu::tui::theme::Palette,
    pub ui_config: humu::tui::theme::UiConfig,
    /// Counter incremented each event-loop tick for animating spinners.
    pub spin_tick: usize,
    /// Suspended room states keyed by (workspace_id, room_id).
    /// Holds live PTY panes so they survive room/workspace switches.
    pub suspended_rooms: HashMap<(WorkspaceId, RoomId), RoomState>,
    /// Active search state (None when not searching).
    pub search_state: Option<SearchState>,
    /// File explorer state for the right-side panel.
    pub explorer_state: humu::explorer::ExplorerState,
    /// Cached room list + git stats per workspace, refreshed periodically (~3s).
    room_cache: HashMap<WorkspaceId, Vec<CachedRoomInfo>>,
    /// Cached flattened workspace tree, rebuilt alongside room_cache.
    workspace_tree_cache: Vec<WorkspaceTreeItem>,
    /// Cursor position in the flat workspace tree (for keyboard navigation).
    selected_tree_index: usize,
    /// When workspace mode was entered (for auto-return to terminal after timeout).
    workspace_mode_entered: Option<std::time::Instant>,
    /// Cached path to state.yaml.
    state_path: std::path::PathBuf,
    /// Path to config.yaml for persisting changes.
    config_path: std::path::PathBuf,
}

impl App {
    pub fn new() -> Result<Self> {
        humu::log::init();

        let config_path = humu_dir().join("config.yaml");
        let config_toml_path = humu_dir().join("config.toml");
        let state_path = humu_dir().join("state.yaml");

        let config = if config_path.exists() {
            HumuConfig::load(&config_path)?
        } else if config_toml_path.exists() {
            let cfg = HumuConfig::load_toml(&config_toml_path)?;
            cfg.save(&config_path)?;
            humu::humu_log!("Migrated config.toml → config.yaml");
            cfg
        } else {
            HumuConfig::default()
        };

        let state = if state_path.exists() {
            HumuState::load(&state_path)?
        } else {
            HumuState::default()
        };

        let tabs = TabContainer::new();
        let local_panes = HashMap::new();
        let pane_presets = HashMap::new();
        let (server_stream, hook_port, attached_snapshot) =
            Self::connect_default_daemon_session()?;

        let ui_config = humu::tui::theme::UiConfig {
            simplified_ui: config.ui.simplified_ui,
            rounded_corners: config.ui.rounded_corners,
        };

        let saved_panel_widths = state.panel_widths.unwrap_or([25, 25]);

        Ok(Self {
            config,
            state,
            mode: Mode::Terminal,
            focus: FocusedPanel::Terminal,
            workspace_selected: None,
            room_selected: None,
            running: true,
            local_panes,
            tabs,
            focused_pane: None,
            pane_presets,
            popup: PopupState::None,
            agent_states: HashMap::new(),
            hook_port,
            server_stream,
            attached_snapshot,
            panel_rects: PanelRects::default(),
            panel_widths: saved_panel_widths,
            pty_mouse_active: false,
            is_focused: true,
            selection: None,
            fullscreen_pane: None,
            palette: humu::tui::theme::Palette::GITHUB_DARK,
            ui_config,
            spin_tick: 0,
            suspended_rooms: HashMap::new(),
            search_state: None,
            explorer_state: humu::explorer::ExplorerState::new(std::path::PathBuf::new()),
            room_cache: HashMap::new(),
            workspace_tree_cache: Vec::new(),
            selected_tree_index: 0,
            workspace_mode_entered: None,
            state_path: humu_dir().join("state.yaml"),
            config_path,
        })
    }

    #[cfg(not(test))]
    fn connect_default_daemon_session(
    ) -> Result<(Option<UnixStream>, Option<u16>, Option<FullSnapshot>)> {
        if let Err(err) = crate::server::daemon::run(true) {
            humu::humu_log!("failed to start daemon runtime: {err}");
            return Ok((None, None, None));
        }

        let hook_port = std::fs::read_to_string(humu_dir().join("port"))
            .ok()
            .and_then(|value| value.trim().parse::<u16>().ok());

        let socket_path = humu_dir().join("server.sock");
        let Ok(mut stream) = UnixStream::connect(&socket_path) else {
            humu::humu_log!(
                "failed to connect to daemon socket {}",
                socket_path.display()
            );
            return Ok((None, hook_port, None));
        };

        let attach = ClientRequest::AttachSession {
            name: "default".to_string(),
            cols: 80,
            rows: 24,
        };
        if stream
            .write_all(&encode_frame(&attach).expect("encode daemon attach"))
            .is_err()
        {
            return Ok((None, hook_port, None));
        }
        match Self::read_daemon_response(&mut stream) {
            Ok(ServerResponse::Attached { snapshot, .. }) => {
                Ok((Some(stream), hook_port, Some(snapshot)))
            }
            Ok(ServerResponse::AlreadyAttached {
                session_name,
                owner_pid,
                attached_at,
            }) => {
                let mut details = Vec::new();
                if let Some(pid) = owner_pid {
                    details.push(format!("pid {pid}"));
                }
                if let Some(attached_at) = attached_at {
                    details.push(format!("attached at {attached_at}"));
                }
                let suffix = if details.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", details.join(", "))
                };
                Err(anyhow::anyhow!(
                    "session \"{session_name}\" is already attached{suffix}"
                ))
            }
            Ok(other) => {
                humu::humu_log!("unexpected daemon attach response: {other:?}");
                Ok((None, hook_port, None))
            }
            Err(err) => {
                humu::humu_log!("failed to read daemon attach response: {err}");
                Ok((None, hook_port, None))
            }
        }
    }

    #[cfg(test)]
    fn connect_default_daemon_session(
    ) -> Result<(Option<UnixStream>, Option<u16>, Option<FullSnapshot>)> {
        Ok((None, None, None))
    }

    fn read_daemon_response(stream: &mut UnixStream) -> anyhow::Result<ServerResponse> {
        let mut decoder = FrameDecoder::new();
        let mut buf = [0u8; 4096];
        loop {
            let read = stream.read(&mut buf)?;
            if read == 0 {
                anyhow::bail!("daemon stream closed before response");
            }
            decoder.push(&buf[..read]);
            if let Some(response) = decoder.try_decode()? {
                return Ok(response);
            }
        }
    }

    fn send_daemon_request(&mut self, request: ClientRequest) -> Option<ServerResponse> {
        let stream = self.server_stream.as_mut()?;
        if let Err(err) = stream.write_all(&encode_frame(&request).ok()?) {
            humu::humu_log!("failed to write daemon request: {err}");
            self.server_stream = None;
            return None;
        }
        match Self::read_daemon_response(stream) {
            Ok(response) => Some(response),
            Err(err) => {
                humu::humu_log!("failed to read daemon response: {err}");
                self.server_stream = None;
                None
            }
        }
    }

    fn sync_daemon_focus(&mut self, focused: bool) {
        let _ = self.send_daemon_request(ClientRequest::FocusChanged { focused });
    }

    fn register_pane_with_daemon(
        &mut self,
        pane_id: PaneId,
        preset_name: &str,
        cwd: Option<PathBuf>,
        session_id: Option<String>,
        started_at: SystemTime,
    ) {
        let started_at_unix_secs = started_at
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = self.send_daemon_request(ClientRequest::RegisterPane {
            pane_id,
            preset_name: preset_name.to_string(),
            cwd,
            session_id,
            started_at_unix_secs,
        });
    }

    fn unregister_pane_with_daemon(&mut self, pane_id: PaneId) {
        let _ = self.send_daemon_request(ClientRequest::UnregisterPane { pane_id });
    }

    fn refresh_attached_runtime_snapshot(&mut self) {
        let Some(attached) = self.attached_snapshot.as_ref() else {
            return;
        };
        let session_name = attached.session_name.clone();
        let session_geometry = attached.session_geometry.clone();
        let cols = session_geometry.as_ref().map(|size| size.cols).unwrap_or(80);
        let rows = session_geometry.as_ref().map(|size| size.rows).unwrap_or(24);
        let Some(ServerResponse::Attached { snapshot, .. }) =
            self.send_daemon_request(ClientRequest::AttachSession {
                name: session_name,
                cols,
                rows,
            })
        else {
            return;
        };
        for (pane_id, pane) in &snapshot.panes {
            if let Some(agent_state) = pane.agent_state.as_ref() {
                let state = match agent_state.status {
                    AgentStatus::Working => AgentState::Working,
                    AgentStatus::NeedsInput => AgentState::NeedsInput,
                    AgentStatus::Idle => AgentState::Idle,
                };
                self.agent_states.insert(
                    *pane_id,
                    AgentStateEntry {
                        state,
                        session_id: agent_state.session_id.clone(),
                    },
                );
            }
        }
        self.attached_snapshot = Some(snapshot);
    }

    fn remove_pane_runtime_state(&mut self, pane_id: PaneId) {
        if let Some(mut pane) = self.local_panes.remove(&pane_id) {
            let _ = pane.kill();
        }
        self.pane_presets.remove(&pane_id);
        self.agent_states.remove(&pane_id);
        self.unregister_pane_with_daemon(pane_id);
        self.refresh_attached_runtime_snapshot();
    }

    fn clear_live_panes(&mut self) {
        let pane_ids: Vec<PaneId> = self.local_panes.keys().copied().collect();
        for pane_id in pane_ids {
            self.remove_pane_runtime_state(pane_id);
        }
        self.tabs = TabContainer::new();
        self.focused_pane = None;
        self.fullscreen_pane = None;
        self.search_state = None;
    }

    fn unregister_room_state_panes(&mut self, room_state: &RoomState) {
        let pane_ids = if room_state.local_panes.is_empty() {
            room_state.pane_presets.keys().copied().collect::<Vec<_>>()
        } else {
            room_state.local_panes.keys().copied().collect::<Vec<_>>()
        };
        for pane_id in pane_ids {
            self.agent_states.remove(&pane_id);
            self.unregister_pane_with_daemon(pane_id);
        }
    }

    fn clear_workspace_suspended_rooms(&mut self, workspace_id: WorkspaceId) {
        let room_keys: Vec<(WorkspaceId, RoomId)> = self
            .suspended_rooms
            .keys()
            .copied()
            .filter(|(wid, _)| *wid == workspace_id)
            .collect();
        for room_key in room_keys {
            if let Some(room_state) = self.suspended_rooms.remove(&room_key) {
                self.unregister_room_state_panes(&room_state);
            }
        }
    }

    fn close_floating_pane(&mut self, pane_id: PaneId) {
        self.remove_pane_runtime_state(pane_id);
        self.popup = PopupState::None;
    }

    fn cleanup_exited_floating_pane(&mut self) {
        let Some(pane_id) = (match &self.popup {
            PopupState::FloatingPane { pane_id, .. } => Some(*pane_id),
            _ => None,
        }) else {
            return;
        };

        if self
            .local_panes
            .get_mut(&pane_id)
            .and_then(|pane| pane.exit_status())
            .is_some()
        {
            self.close_floating_pane(pane_id);
        }
    }

    fn split_tree_from_snapshot(tree: &SplitTreeSnapshot) -> Option<SplitTree> {
        match tree {
            SplitTreeSnapshot::Leaf { pane_id } => Some(SplitTree::leaf(*pane_id)),
            SplitTreeSnapshot::Split {
                direction,
                ratio,
                children,
            } => {
                if children.len() < 2 {
                    return None;
                }
                let left = Self::split_tree_from_snapshot(&children[0])?;
                let right = Self::split_tree_from_snapshot(&children[1])?;
                let direction = match direction {
                    SplitDirectionSnapshot::Vertical => SplitDirection::Vertical,
                    SplitDirectionSnapshot::Horizontal => SplitDirection::Horizontal,
                };
                Some(SplitTree::Split {
                    direction,
                    ratio: *ratio,
                    children: Box::new((left, right)),
                })
            }
        }
    }

    fn split_tree_from_pane_ids(pane_ids: &[PaneId]) -> Option<SplitTree> {
        let mut pane_ids = pane_ids.iter().copied();
        let first = pane_ids.next()?;
        let mut tree = SplitTree::leaf(first);
        for (index, pane_id) in pane_ids.enumerate() {
            let direction = if index % 2 == 0 {
                SplitDirection::Horizontal
            } else {
                SplitDirection::Vertical
            };
            tree = SplitTree::Split {
                direction,
                ratio: 0.5,
                children: Box::new((tree, SplitTree::leaf(pane_id))),
            };
        }
        Some(tree)
    }

    fn restore_snapshot_layout(&mut self, snapshot: &FullSnapshot) {
        if snapshot.tabs.is_empty() {
            self.tabs = TabContainer::new();
            self.focused_pane = None;
            self.fullscreen_pane = None;
            return;
        }

        let active_index = snapshot.active_tab_index.unwrap_or(0);
        let mut tabs = TabContainer::new();
        for (index, tab) in snapshot.tabs.iter().enumerate() {
            let tree = if index == active_index {
                snapshot
                    .split_tree
                    .as_ref()
                    .and_then(Self::split_tree_from_snapshot)
            } else {
                None
            }
            .or_else(|| Self::split_tree_from_pane_ids(&tab.pane_ids));

            if let Some(tree) = tree {
                tabs.add_tab(tab.name.clone(), tree);
            }
        }

        if tabs.is_empty() {
            self.tabs = TabContainer::new();
            self.focused_pane = snapshot.focused_pane_id;
            self.fullscreen_pane = snapshot.fullscreen_pane_id;
            return;
        }

        let active_index = active_index.min(tabs.len() - 1);
        tabs.set_active(active_index);
        self.tabs = tabs;
        self.focused_pane = snapshot.focused_pane_id;
        self.fullscreen_pane = snapshot.fullscreen_pane_id;

        for (pane_id, pane) in &snapshot.panes {
            self.pane_presets
                .insert(*pane_id, pane.preset_name.clone());
        }
    }

    fn create_attached_placeholder_pane(
        &mut self,
        preset_name: &str,
        session_id: Option<String>,
    ) -> PaneId {
        let pane_id = PaneId::new();
        self.pane_presets.insert(pane_id, preset_name.to_string());
        if session_id.is_some() {
            self.agent_states.insert(
                pane_id,
                AgentStateEntry {
                    state: AgentState::Idle,
                    session_id,
                },
            );
        }
        pane_id
    }

    fn remap_split_tree(
        tree: &SplitTree,
        pane_mapping: &HashMap<PaneId, PaneId>,
    ) -> Option<SplitTree> {
        match tree {
            SplitTree::Leaf(pane_id) => Some(SplitTree::leaf(*pane_mapping.get(pane_id)?)),
            SplitTree::Split {
                direction,
                ratio,
                children,
            } => Some(SplitTree::Split {
                direction: *direction,
                ratio: *ratio,
                children: Box::new((
                    Self::remap_split_tree(&children.0, pane_mapping)?,
                    Self::remap_split_tree(&children.1, pane_mapping)?,
                )),
            }),
        }
    }

    fn remap_layout_from_attached_snapshot(
        &mut self,
        snapshot: &FullSnapshot,
    ) -> Option<HashMap<PaneId, PaneId>> {
        if self.tabs.is_empty() || self.pane_presets.is_empty() {
            return None;
        }

        let mut snapshot_by_key = HashMap::<(String, Option<String>), Vec<PaneId>>::new();
        let mut snapshot_by_preset = HashMap::<String, Vec<PaneId>>::new();
        for (pane_id, pane) in &snapshot.panes {
            let session_id = pane
                .agent_state
                .as_ref()
                .and_then(|agent_state| agent_state.session_id.clone());
            snapshot_by_key
                .entry((pane.preset_name.clone(), session_id))
                .or_default()
                .push(*pane_id);
            snapshot_by_preset
                .entry(pane.preset_name.clone())
                .or_default()
                .push(*pane_id);
        }

        let mut pane_mapping = HashMap::<PaneId, PaneId>::new();
        let mut used_snapshot_panes = std::collections::HashSet::new();

        let mut local_pane_ids = self.pane_presets.keys().copied().collect::<Vec<_>>();
        local_pane_ids.sort_by_key(|pane_id| pane_id.to_string());

        for local_pane_id in &local_pane_ids {
            let Some(preset_name) = self.pane_presets.get(local_pane_id).cloned() else {
                continue;
            };
            let session_id = self
                .agent_states
                .get(local_pane_id)
                .and_then(|entry| entry.session_id.clone());
            if let Some(candidates) = snapshot_by_key.get(&(preset_name, session_id)) {
                if let Some(snapshot_pane_id) = candidates
                    .iter()
                    .copied()
                    .find(|pane_id| !used_snapshot_panes.contains(pane_id))
                {
                    pane_mapping.insert(*local_pane_id, snapshot_pane_id);
                    used_snapshot_panes.insert(snapshot_pane_id);
                }
            }
        }

        for local_pane_id in &local_pane_ids {
            if pane_mapping.contains_key(local_pane_id) {
                continue;
            }
            let Some(preset_name) = self.pane_presets.get(local_pane_id) else {
                continue;
            };
            if let Some(candidates) = snapshot_by_preset.get(preset_name) {
                if let Some(snapshot_pane_id) = candidates
                    .iter()
                    .copied()
                    .find(|pane_id| !used_snapshot_panes.contains(pane_id))
                {
                    pane_mapping.insert(*local_pane_id, snapshot_pane_id);
                    used_snapshot_panes.insert(snapshot_pane_id);
                }
            }
        }

        let tab_names = self
            .tabs
            .tab_names()
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let active_index = self.tabs.active_index();
        let focused_pane = self.focused_pane;
        let fullscreen_pane = self.fullscreen_pane;

        let mut remapped_tabs = TabContainer::new();
        for (index, name) in tab_names.iter().enumerate() {
            let tree = self.tabs.tree_at(index)?;
            remapped_tabs.add_tab(name.clone(), Self::remap_split_tree(tree, &pane_mapping)?);
        }

        if !remapped_tabs.is_empty() {
            remapped_tabs.set_active(active_index.min(remapped_tabs.len() - 1));
        }
        self.tabs = remapped_tabs;
        self.focused_pane = focused_pane.and_then(|pane_id| pane_mapping.get(&pane_id).copied());
        self.fullscreen_pane =
            fullscreen_pane.and_then(|pane_id| pane_mapping.get(&pane_id).copied());
        if self.focused_pane.is_none() {
            self.focused_pane = snapshot.focused_pane_id;
        }
        if self.fullscreen_pane.is_none() {
            self.fullscreen_pane = snapshot.fullscreen_pane_id;
        }

        let pane_presets = snapshot
            .panes
            .iter()
            .map(|(pane_id, pane)| (*pane_id, pane.preset_name.clone()))
            .collect::<HashMap<_, _>>();
        self.pane_presets = pane_presets;

        Some(pane_mapping)
    }

    fn hydrate_attached_snapshot(&mut self) {
        let Some(snapshot) = self.attached_snapshot.clone() else {
            return;
        };

        if let Some(workspace_id) = snapshot.active_workspace_id {
            self.state.active_workspace_id = Some(workspace_id);
            self.workspace_selected = Some(workspace_id);
        }
        if let Some(room_id) = snapshot.active_room_id {
            self.state.active_room_id = Some(room_id);
            self.room_selected = Some(room_id);
        }

        let mut snapshot_states = HashMap::new();
        let mut snapshot_entries = Vec::new();
        for pane in snapshot.panes.values() {
            let Some(agent_state) = pane.agent_state.as_ref() else {
                continue;
            };
            let state = match agent_state.status {
                AgentStatus::Working => AgentState::Working,
                AgentStatus::NeedsInput => AgentState::NeedsInput,
                AgentStatus::Idle => AgentState::Idle,
            };
            snapshot_entries.push((
                pane.preset_name.clone(),
                state.clone(),
                agent_state.session_id.clone(),
            ));
            if let Some(session_id) = agent_state.session_id.clone() {
                snapshot_states.insert((pane.preset_name.clone(), session_id), state.clone());
            }
        }

        let mut unmatched_local_panes = HashMap::<String, Vec<PaneId>>::new();
        let mut matched_snapshot_keys = std::collections::HashSet::new();
        for (pane_id, preset_name) in &self.pane_presets {
            let Some(session_id) = self
                .agent_states
                .get(pane_id)
                .and_then(|entry| entry.session_id.clone())
            else {
                unmatched_local_panes
                    .entry(preset_name.clone())
                    .or_default()
                    .push(*pane_id);
                continue;
            };
            if let Some(state) = snapshot_states.get(&(preset_name.clone(), session_id.clone())) {
                matched_snapshot_keys.insert((preset_name.clone(), Some(session_id.clone())));
                self.agent_states.insert(
                    *pane_id,
                    AgentStateEntry {
                        state: state.clone(),
                        session_id: Some(session_id),
                    },
                );
            }
        }

        let mut unmatched_snapshot_by_preset =
            HashMap::<String, Vec<(AgentState, Option<String>)>>::new();
        for (preset_name, state, session_id) in snapshot_entries {
            if matched_snapshot_keys.contains(&(preset_name.clone(), session_id.clone())) {
                continue;
            }
            unmatched_snapshot_by_preset
                .entry(preset_name)
                .or_default()
                .push((state, session_id));
        }

        for (preset_name, pane_ids) in unmatched_local_panes {
            let Some(snapshot_entries) = unmatched_snapshot_by_preset.get(&preset_name) else {
                continue;
            };
            if pane_ids.len() == 1 && snapshot_entries.len() == 1 {
                let (state, session_id) = snapshot_entries[0].clone();
                self.agent_states
                    .insert(pane_ids[0], AgentStateEntry { state, session_id });
            }
        }

        let pane_mapping = self.remap_layout_from_attached_snapshot(&snapshot);
        if pane_mapping.is_none() {
            self.restore_snapshot_layout(&snapshot);
        }
        self.agent_states
            .retain(|pane_id, _| snapshot.panes.contains_key(pane_id));
        for (pane_id, pane) in &snapshot.panes {
            if let Some(agent_state) = pane.agent_state.as_ref() {
                let state = match agent_state.status {
                    AgentStatus::Working => AgentState::Working,
                    AgentStatus::NeedsInput => AgentState::NeedsInput,
                    AgentStatus::Idle => AgentState::Idle,
                };
                self.agent_states.insert(
                    *pane_id,
                    AgentStateEntry {
                        state,
                        session_id: agent_state.session_id.clone(),
                    },
                );
            }
        }
    }

    fn attached_pane_snapshot(&self, pane_id: PaneId) -> Option<&PaneSnapshot> {
        self.attached_snapshot.as_ref()?.panes.get(&pane_id)
    }

    fn pane_input_state(&self, pane_id: PaneId) -> Option<PaneInputState> {
        if let Some(pane) = self.local_panes.get(&pane_id) {
            return Some(pane.input_state());
        }
        self.attached_pane_snapshot(pane_id)
            .map(PaneSnapshot::input_state)
    }

    fn pane_exit_code(&mut self, pane_id: PaneId) -> Option<i32> {
        if let Some(pane) = self.local_panes.get_mut(&pane_id) {
            return pane.exit_status();
        }
        self.attached_pane_snapshot(pane_id)
            .and_then(PaneSnapshot::exit_code)
    }

    fn pane_has_exited(&mut self, pane_id: PaneId) -> bool {
        self.pane_exit_code(pane_id).is_some()
    }

    fn pane_search_rows(&self, pane_id: PaneId) -> Option<Vec<(String, Vec<usize>)>> {
        if let Some(pane) = self.local_panes.get(&pane_id) {
            let screen = pane.screen_snapshot();
            return Some(humu::tui::search::extract_rows(&screen));
        }
        self.attached_pane_snapshot(pane_id)
            .map(|pane| pane.screen.extract_rows())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn pane_screen_contents(&self, pane_id: PaneId) -> Option<String> {
        if let Some(pane) = self.local_panes.get(&pane_id) {
            return Some(pane.screen_snapshot().contents());
        }
        self.attached_pane_snapshot(pane_id)
            .map(|pane| pane.screen.contents())
    }

    pub fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        stdout().execute(EnterAlternateScreen)?;
        crossterm::execute!(
            stdout(),
            crossterm::event::EnableMouseCapture,
            crossterm::event::EnableBracketedPaste,
            crossterm::event::EnableFocusChange,
        )?;
        // Enable Kitty keyboard protocol for modifier-aware keys (Shift+Enter, etc.).
        // Request the full progressive enhancement set so Unicode/non-ASCII
        // modified keys arrive as CSI-u events (for example Ctrl+Hangul jamo).
        // Silently ignored on terminals that don't support it.
        let keyboard_enhanced = crossterm::execute!(
            stdout(),
            crossterm::event::PushKeyboardEnhancementFlags(
                crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | crossterm::event::KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
                    | crossterm::event::KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                    | crossterm::event::KeyboardEnhancementFlags::REPORT_EVENT_TYPES,
            ),
        )
        .is_ok();

        let backend = ratatui::backend::CrosstermBackend::new(stdout());
        let mut terminal = Terminal::new(backend)?;

        self.restore_selection();
        self.hydrate_attached_snapshot();
        self.refresh_room_cache();
        self.rebuild_workspace_tree();

        // Initialize explorer with the active room's directory.
        if let Some(path) = self.current_room_path() {
            self.explorer_state = humu::explorer::ExplorerState::new(path);
            self.explorer_state.scan();
        }

        if self.state.active_room_id.is_none() {
            self.mode = Mode::Workspace;
            self.focus = FocusedPanel::Workspace;
        }

        while self.running {
            // Process PTY output before render so cursor position is up-to-date.
            for pane in self.local_panes.values_mut() {
                let _ = pane.process_output();
            }

            // Auto-close exited panes.
            self.cleanup_exited_panes();

            // Floating pane: resize PTY to match popup area, auto-close on exit.
            if let PopupState::FloatingPane { pane_id, .. } = &self.popup {
                let pane_id = *pane_id;
                let fp_area = self.floating_pane_area();
                let inner_w = fp_area.width.saturating_sub(2);
                let inner_h = fp_area.height.saturating_sub(2);
                if let Some(pane) = self.local_panes.get_mut(&pane_id) {
                    if pane.cols() != inner_w || pane.rows() != inner_h {
                        let _ = pane.resize(inner_w, inner_h);
                    }
                }
            }
            self.cleanup_exited_floating_pane();

            // Auto-return from workspace mode to terminal after 5s of inactivity.
            if let Some(entered) = self.workspace_mode_entered {
                if self.mode == Mode::Workspace && entered.elapsed().as_secs() >= 5 {
                    self.mode = Mode::Terminal;
                    self.focus = FocusedPanel::Terminal;
                    self.workspace_mode_entered = None;
                }
            }

            // Periodic rescan (~3s) to pick up git status changes.
            if self.spin_tick % 60 == 0 {
                if !self.explorer_state.root.as_os_str().is_empty() {
                    self.explorer_state.scan();
                }
                self.refresh_room_cache();
                self.rebuild_workspace_tree();
            }

            terminal.draw(|frame| self.render(frame))?;
            self.spin_tick = self.spin_tick.wrapping_add(1);

            // Drain all pending events before rendering to avoid
            // per-event renders when mouse moves queue up.
            while event::poll(Duration::from_millis(0))? {
                match event::read()? {
                    Event::Key(key) if should_handle_key_event(&key) => {
                        let key = normalize_key_event(key);
                        // Reset workspace auto-return timer on any keypress.
                        if self.mode == Mode::Workspace {
                            self.workspace_mode_entered = Some(std::time::Instant::now());
                        }
                        // Ctrl+Q is a global quit — bypass popups.
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && key.code == KeyCode::Char('q')
                            && !matches!(self.popup, PopupState::FloatingPane { .. })
                        {
                            self.handle_action(Action::Quit);
                        } else if self.handle_popup_key(key) {
                        } else {
                            self.handle_action(handle_key(self.mode, key));
                        }
                    }
                    Event::Mouse(mouse) => self.handle_mouse(mouse),
                    Event::Paste(text) => self.handle_paste_event(&text),
                    Event::FocusGained => {
                        self.is_focused = true;
                        self.sync_daemon_focus(true);
                    }
                    Event::FocusLost => {
                        self.is_focused = false;
                        self.sync_daemon_focus(false);
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }
            // If no events were pending, wait up to 50ms for the next one.
            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(key) if should_handle_key_event(&key) => {
                        let key = normalize_key_event(key);
                        // Reset workspace auto-return timer on any keypress.
                        if self.mode == Mode::Workspace {
                            self.workspace_mode_entered = Some(std::time::Instant::now());
                        }
                        // Ctrl+Q is a global quit — bypass popups.
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && key.code == KeyCode::Char('q')
                            && !matches!(self.popup, PopupState::FloatingPane { .. })
                        {
                            self.handle_action(Action::Quit);
                        } else if self.handle_popup_key(key) {
                        } else {
                            self.handle_action(handle_key(self.mode, key));
                        }
                    }
                    Event::Mouse(mouse) => self.handle_mouse(mouse),
                    Event::Paste(text) => self.handle_paste_event(&text),
                    Event::FocusGained => {
                        self.is_focused = true;
                        self.sync_daemon_focus(true);
                    }
                    Event::FocusLost => {
                        self.is_focused = false;
                        self.sync_daemon_focus(false);
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }

            // Refresh log viewer if open.
            self.refresh_log_viewer();
        }

        if keyboard_enhanced {
            let _ = crossterm::execute!(stdout(), crossterm::event::PopKeyboardEnhancementFlags);
        }
        crossterm::execute!(
            stdout(),
            crossterm::event::DisableBracketedPaste,
            crossterm::event::DisableMouseCapture,
            crossterm::event::DisableFocusChange,
        )?;
        stdout().execute(LeaveAlternateScreen)?;
        disable_raw_mode()?;

        // Graceful shutdown: sync layout for current room and all suspended rooms,
        // then write once to disk.
        self.sync_layout();
        self.local_panes.clear();

        // Sync suspended rooms into state.
        let suspended: Vec<_> = self.suspended_rooms.drain().collect();
        for ((ws_id, room_id), room_state) in suspended {
            // Temporarily swap in the suspended state to reuse persist helpers.
            self.tabs = room_state.tabs;
            self.pane_presets = room_state.pane_presets;
            let layout = self.save_layout();
            self.persist_room_layout(ws_id, room_id, layout);
        }

        self.save_state();

        Ok(())
    }

    /// Handle a key event when a popup is active.
    /// Returns `true` if the key was consumed (popup was active), `false` otherwise.
    fn handle_popup_key(&mut self, key: KeyEvent) -> bool {
        match &self.popup {
            PopupState::None => false,

            PopupState::Settings { .. } => {
                self.handle_settings_key(key);
                true
            }
            PopupState::SplitDirection => {
                self.handle_split_direction_key(key);
                true
            }
            PopupState::LogViewer { .. } => {
                self.handle_log_viewer_key(key);
                true
            }
            PopupState::PresetSelector { .. } => {
                self.handle_preset_selector_key(key);
                true
            }
            PopupState::WorkspaceCreate { .. }
            | PopupState::RoomCreate { .. }
            | PopupState::WorkspaceDelete { .. }
            | PopupState::RoomDelete { .. } => {
                self.handle_dialog_key(key);
                true
            }
            PopupState::NotificationSettings { .. } => {
                self.handle_notification_settings_key(key);
                true
            }
            PopupState::NotificationTokenInput { .. } => {
                self.handle_notification_token_input_key(key);
                true
            }
            PopupState::FloatingPane { pane_id, .. } => {
                let pane_id = *pane_id;
                self.handle_floating_pane_key(pane_id, key);
                true
            }
            PopupState::ErrorDialog { .. } => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char(' ')) {
                    self.popup = PopupState::None;
                }
                true
            }
            PopupState::ExplorerNewEntry { is_dir, value } => {
                let is_dir = *is_dir;
                let mut value = value.clone();
                match key.code {
                    KeyCode::Enter => {
                        if !value.is_empty() {
                            self.explorer_create_entry(is_dir, &value);
                            // Don't clear popup if show_error set an ErrorDialog
                            if matches!(self.popup, PopupState::ExplorerNewEntry { .. }) {
                                self.popup = PopupState::None;
                            }
                        } else {
                            self.popup = PopupState::None;
                        }
                    }
                    KeyCode::Esc => {
                        self.popup = PopupState::None;
                    }
                    KeyCode::Backspace => {
                        value.pop();
                        self.popup = PopupState::ExplorerNewEntry { is_dir, value };
                    }
                    KeyCode::Char(c) => {
                        value.push(c);
                        self.popup = PopupState::ExplorerNewEntry { is_dir, value };
                    }
                    _ => {}
                }
                true
            }
            PopupState::ExplorerDeleteConfirm {
                path,
                name,
                ok_selected,
            } => {
                let path = path.clone();
                let name = name.clone();
                let mut ok_selected = *ok_selected;
                match key.code {
                    KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
                        ok_selected = !ok_selected;
                        self.popup = PopupState::ExplorerDeleteConfirm {
                            path,
                            name,
                            ok_selected,
                        };
                    }
                    KeyCode::Enter => {
                        if ok_selected {
                            self.explorer_delete_entry(&path);
                        }
                        self.popup = PopupState::None;
                    }
                    KeyCode::Esc => {
                        self.popup = PopupState::None;
                    }
                    _ => {}
                }
                true
            }
        }
    }

    fn handle_preset_selector_key(&mut self, key: KeyEvent) {
        let PopupState::PresetSelector {
            presets,
            selected,
            action,
        } = &self.popup
        else {
            return;
        };
        let presets = presets.clone();
        let mut selected = *selected;
        let action = *action;

        match key.code {
            KeyCode::Down => {
                if selected + 1 < presets.len() {
                    selected += 1;
                }
                self.popup = PopupState::PresetSelector {
                    presets,
                    selected,
                    action,
                };
            }
            KeyCode::Up => {
                selected = selected.saturating_sub(1);
                self.popup = PopupState::PresetSelector {
                    presets,
                    selected,
                    action,
                };
            }
            KeyCode::Enter => {
                let chosen = presets[selected].clone();
                self.popup = PopupState::None;
                match action {
                    PresetAction::NewTab => self.new_tab_with_preset(&chosen),
                    PresetAction::SplitDown => self.split_pane_with_preset(&chosen, false),
                    PresetAction::SplitRight => self.split_pane_with_preset(&chosen, true),
                }
                self.mode = Mode::Terminal;
            }
            KeyCode::Esc => {
                self.popup = PopupState::None;
            }
            _ => {}
        }
    }

    const SETTINGS_ITEMS: &'static [&'static str] = &["Notifications", "View Logs"];

    fn handle_settings_key(&mut self, key: KeyEvent) {
        let PopupState::Settings { selected } = &self.popup else {
            return;
        };
        let mut selected = *selected;

        match key.code {
            KeyCode::Down => {
                if selected + 1 < Self::SETTINGS_ITEMS.len() {
                    selected += 1;
                }
                self.popup = PopupState::Settings { selected };
            }
            KeyCode::Up => {
                selected = selected.saturating_sub(1);
                self.popup = PopupState::Settings { selected };
            }
            KeyCode::Enter => match selected {
                0 => {
                    self.popup = PopupState::NotificationSettings { selected: 0 };
                }
                1 => {
                    self.popup = PopupState::None;
                    self.open_log_viewer();
                }
                _ => {}
            },
            KeyCode::Esc => {
                self.popup = PopupState::None;
            }
            _ => {}
        }
    }

    fn notification_settings_items(&self) -> Vec<String> {
        let cfg = &self.config.notifications;
        let on_off = |b: bool| if b { "ON" } else { "OFF" };
        vec![
            format!("OS Notifications: {}", on_off(cfg.os.enabled)),
            format!("OS Only Unfocused: {}", on_off(cfg.os.only_unfocused)),
            format!("Sound: {}", on_off(cfg.sound.enabled)),
            format!("Sound Only Unfocused: {}", on_off(cfg.sound.only_unfocused)),
            format!("Telegram: {}", on_off(cfg.telegram.enabled)),
            format!(
                "Telegram Only Unfocused: {}",
                on_off(cfg.telegram.only_unfocused)
            ),
            format!(
                "Telegram Bot Token: {}",
                if cfg.telegram.bot_token_encrypted.is_empty() {
                    "(not set)"
                } else {
                    "****"
                }
            ),
            format!(
                "Telegram Chat ID: {}",
                if cfg.telegram.chat_id_encrypted.is_empty() {
                    "(not set)"
                } else {
                    "****"
                }
            ),
        ]
    }

    fn handle_notification_settings_key(&mut self, key: KeyEvent) {
        let PopupState::NotificationSettings { selected } = &self.popup else {
            return;
        };
        let mut selected = *selected;
        let item_count = 8;

        match key.code {
            KeyCode::Down => {
                if selected + 1 < item_count {
                    selected += 1;
                }
                self.popup = PopupState::NotificationSettings { selected };
            }
            KeyCode::Up => {
                selected = selected.saturating_sub(1);
                self.popup = PopupState::NotificationSettings { selected };
            }
            KeyCode::Enter | KeyCode::Char(' ') => match selected {
                0 => {
                    self.config.notifications.os.enabled = !self.config.notifications.os.enabled;
                    self.rebuild_notification_manager();
                }
                1 => {
                    self.config.notifications.os.only_unfocused =
                        !self.config.notifications.os.only_unfocused;
                    self.rebuild_notification_manager();
                }
                2 => {
                    self.config.notifications.sound.enabled =
                        !self.config.notifications.sound.enabled;
                    self.rebuild_notification_manager();
                }
                3 => {
                    self.config.notifications.sound.only_unfocused =
                        !self.config.notifications.sound.only_unfocused;
                    self.rebuild_notification_manager();
                }
                4 => {
                    self.config.notifications.telegram.enabled =
                        !self.config.notifications.telegram.enabled;
                    self.rebuild_notification_manager();
                }
                5 => {
                    self.config.notifications.telegram.only_unfocused =
                        !self.config.notifications.telegram.only_unfocused;
                    self.rebuild_notification_manager();
                }
                6 => {
                    self.popup = PopupState::NotificationTokenInput {
                        field: NotificationField::BotToken,
                        value: String::new(),
                    };
                    return;
                }
                7 => {
                    self.popup = PopupState::NotificationTokenInput {
                        field: NotificationField::ChatId,
                        value: String::new(),
                    };
                    return;
                }
                _ => {}
            },
            KeyCode::Esc => {
                self.popup = PopupState::Settings { selected: 0 };
            }
            _ => {}
        }
    }

    fn handle_notification_token_input_key(&mut self, key: KeyEvent) {
        let PopupState::NotificationTokenInput { field, value } = &self.popup else {
            return;
        };
        let field = *field;
        let mut value = value.clone();

        match key.code {
            KeyCode::Enter => {
                let encrypted = humu::notification::crypto::encrypt(&value).unwrap_or_default();
                match field {
                    NotificationField::BotToken => {
                        self.config.notifications.telegram.bot_token_encrypted = encrypted;
                    }
                    NotificationField::ChatId => {
                        self.config.notifications.telegram.chat_id_encrypted = encrypted;
                    }
                }
                self.rebuild_notification_manager();
                self.popup = PopupState::NotificationSettings { selected: 0 };
                return;
            }
            KeyCode::Esc => {
                self.popup = PopupState::NotificationSettings { selected: 0 };
                return;
            }
            KeyCode::Backspace => {
                value.pop();
            }
            KeyCode::Char(c) => {
                value.push(c);
            }
            _ => {}
        }
        self.popup = PopupState::NotificationTokenInput { field, value };
    }

    fn handle_floating_pane_key(&mut self, pane_id: PaneId, key: KeyEvent) {
        // Ctrl+Q or Ctrl+G closes the floating pane.
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('q') | KeyCode::Char('g'))
        {
            self.close_floating_pane(pane_id);
            return;
        }
        // Forward all keys to the PTY
        if let Some(pane) = self.local_panes.get_mut(&pane_id) {
            let bytes = key_event_to_bytes(&key);
            if !bytes.is_empty() {
                let _ = pane.write_input(&bytes);
            }
        }
    }

    fn apply_input_route(
        &mut self,
        pane_id: PaneId,
        pane_rect: Option<Rect>,
        route: InputRoute,
    ) -> bool {
        let InputRoute::Handled(actions) = route else {
            return false;
        };

        let mut finish_selection = false;

        for action in actions {
            match action {
                InputAction::Write(bytes) => {
                    if let Some(pane) = self.local_panes.get_mut(&pane_id) {
                        let _ = pane.write_input(&bytes);
                    } else {
                        let _ = self.send_daemon_request(ClientRequest::SendInput { pane_id, bytes });
                    }
                }
                InputAction::AdjustScrollback { lines, up } => {
                    if let Some(pane) = self.local_panes.get(&pane_id) {
                        if up {
                            pane.scrollback_up(lines);
                        } else {
                            pane.scrollback_down(lines);
                        }
                    }
                }
                InputAction::ResetScrollback => {
                    if let Some(pane) = self.local_panes.get(&pane_id) {
                        pane.reset_scrollback();
                    }
                }
                InputAction::StartSelection { row, col } => {
                    if pane_rect.is_some() {
                        self.selection = Some(TextSelection {
                            pane_id,
                            start: (row, col),
                            end: (row, col),
                        });
                    }
                }
                InputAction::UpdateSelection { row, col } => {
                    if let Some(ref mut sel) = self.selection
                        && sel.pane_id == pane_id
                    {
                        sel.end = (row, col);
                    }
                }
                InputAction::FinishSelection => {
                    finish_selection = true;
                }
            }
        }

        if finish_selection {
            if let Some(ref sel) = self.selection
                && sel.pane_id == pane_id
                && sel.start != sel.end
            {
                self.copy_selection_to_clipboard();
            }
            if self
                .selection
                .as_ref()
                .is_some_and(|sel| sel.pane_id == pane_id)
            {
                self.selection = None;
            }
        }

        true
    }

    /// Forward a mouse event to the floating pane's PTY. Returns true if handled.
    fn forward_mouse_to_floating_pane(
        &mut self,
        pane_id: PaneId,
        mouse: &crossterm::event::MouseEvent,
    ) -> bool {
        let popup_area = self.floating_pane_area();

        let pos = Position::new(mouse.column, mouse.row);
        if !popup_area.contains(pos) {
            return false;
        }

        let pane = match self.local_panes.get_mut(&pane_id) {
            Some(p) => p,
            None => return false,
        };
        let state = pane.input_state();
        let _ = pane;
        self.apply_input_route(
            pane_id,
            None,
            route_floating_mouse(*mouse, popup_area, &state),
        )
    }

    fn show_error(&mut self, message: impl Into<String>) {
        self.popup = PopupState::ErrorDialog {
            message: message.into(),
        };
    }

    fn rebuild_notification_manager(&mut self) {
        if let Err(e) = self.config.save(&self.config_path) {
            humu::humu_log!("failed to save config: {e}");
        }
    }

    fn handle_split_direction_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Down => {
                self.popup = PopupState::None;
                self.show_preset_selector(PresetAction::SplitDown);
            }
            KeyCode::Right => {
                self.popup = PopupState::None;
                self.show_preset_selector(PresetAction::SplitRight);
            }
            KeyCode::Esc => {
                self.popup = PopupState::None;
            }
            _ => {}
        }
    }

    fn read_log_lines() -> (Vec<String>, u64) {
        let path = humu::log::log_path();
        let file_len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let lines = match std::fs::read_to_string(&path) {
            Ok(content) => content.lines().map(String::from).collect(),
            Err(_) => vec![],
        };
        (lines, file_len)
    }

    fn refresh_log_viewer(&mut self) {
        if let PopupState::LogViewer {
            lines,
            scroll,
            file_len,
            ..
        } = &mut self.popup
        {
            let current_len = std::fs::metadata(humu::log::log_path())
                .map(|m| m.len())
                .unwrap_or(0);
            if current_len == *file_len {
                return;
            }
            let was_at_end = *scroll + 1 >= lines.len();
            let (new_lines, new_len) = Self::read_log_lines();
            *lines = new_lines;
            *file_len = new_len;
            if was_at_end {
                *scroll = lines.len().saturating_sub(1);
            }
        }
    }

    fn open_log_viewer(&mut self) {
        let (lines, file_len) = Self::read_log_lines();
        let scroll = lines.len().saturating_sub(1);
        self.popup = PopupState::LogViewer {
            lines,
            scroll,
            h_scroll: 0,
            file_len,
        };
    }

    fn handle_log_viewer_key(&mut self, key: KeyEvent) {
        let PopupState::LogViewer {
            lines,
            scroll,
            h_scroll,
            ..
        } = &self.popup
        else {
            return;
        };
        let total = lines.len();
        let mut scroll = *scroll;
        let mut h_scroll = *h_scroll;

        match key.code {
            KeyCode::Esc => {
                self.popup = PopupState::None;
                return;
            }
            KeyCode::Up => scroll = scroll.saturating_sub(1),
            KeyCode::Down => {
                if scroll + 1 < total {
                    scroll += 1;
                }
            }
            KeyCode::Left => h_scroll = h_scroll.saturating_sub(10),
            KeyCode::Right => h_scroll += 10,
            KeyCode::PageUp => scroll = scroll.saturating_sub(20),
            KeyCode::PageDown => scroll = (scroll + 20).min(total.saturating_sub(1)),
            KeyCode::Home => {
                scroll = 0;
                h_scroll = 0;
            }
            KeyCode::End => scroll = total.saturating_sub(1),
            _ => {}
        }

        if let PopupState::LogViewer {
            scroll: ref mut s,
            h_scroll: ref mut hs,
            ..
        } = self.popup
        {
            *s = scroll;
            *hs = h_scroll;
        }
    }

    fn handle_dialog_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                if !self.try_dismiss_completions() {
                    self.popup = PopupState::None;
                }
            }
            KeyCode::Enter => {
                if !self.try_confirm_completion() {
                    self.confirm_dialog();
                }
            }
            KeyCode::Tab | KeyCode::BackTab => {
                if !self.try_accept_completion() {
                    self.dialog_move_focus(key.code == KeyCode::Tab);
                }
            }
            KeyCode::Up => {
                if !self.try_navigate_completion(false) {
                    self.dialog_move_focus(false);
                }
            }
            KeyCode::Down => {
                if !self.try_navigate_completion(true) {
                    self.dialog_move_focus(true);
                }
            }
            KeyCode::Left => {
                self.dialog_field_left();
            }
            KeyCode::Right => {
                self.dialog_field_right();
            }
            KeyCode::Backspace => {
                self.dialog_field_backspace();
                self.refresh_completions();
            }
            KeyCode::Char(c) => {
                self.dialog_field_insert(c);
                self.refresh_completions();
            }
            _ => {}
        }
    }

    /// If the workspace-create Path field is focused and has completions,
    /// accept the selected (or first) completion.  Returns `true` if consumed.
    fn try_accept_completion(&mut self) -> bool {
        let PopupState::WorkspaceCreate {
            fields,
            focused_field,
            completions,
            completion_selected,
        } = &mut self.popup
        else {
            return false;
        };

        // Only act when focused on the Path field (index 1) and completions exist.
        if *focused_field != 1 || completions.is_empty() {
            return false;
        }

        // Accept the current selection, or pick the first item if none highlighted.
        let next = completion_selected.unwrap_or(0);

        // Write the selected completion into the Path field value.
        if let Some(DialogField::TextInput { value, .. }) = fields.get_mut(1) {
            *value = completions[next].clone();
        }
        // Recompute completions for the new value (important for directories).
        // Always clear selection so Enter submits the dialog next time
        // unless the user explicitly navigates with Up/Down.
        if let Some(DialogField::TextInput { value, .. }) = fields.get(1) {
            *completions = complete_path(value);
        }
        *completion_selected = None;

        true
    }

    /// Clear completions if they are visible.  Returns `true` if consumed
    /// (so Esc dismisses suggestions first, closes dialog on second Esc).
    fn try_dismiss_completions(&mut self) -> bool {
        let PopupState::WorkspaceCreate {
            focused_field,
            completions,
            completion_selected,
            ..
        } = &mut self.popup
        else {
            return false;
        };

        if *focused_field != 1 || completions.is_empty() {
            return false;
        }

        completions.clear();
        *completion_selected = None;
        true
    }

    /// Accept the highlighted completion into the Path field on Enter.
    /// Returns `true` if a completion was selected (dialog NOT submitted).
    fn try_confirm_completion(&mut self) -> bool {
        let PopupState::WorkspaceCreate {
            fields,
            focused_field,
            completions,
            completion_selected,
        } = &mut self.popup
        else {
            return false;
        };

        if *focused_field != 1 {
            return false;
        }

        let Some(sel) = *completion_selected else {
            return false;
        };

        if sel >= completions.len() {
            return false;
        }

        if let Some(DialogField::TextInput { value, .. }) = fields.get_mut(1) {
            *value = completions[sel].clone();
        }
        *completions = if let Some(DialogField::TextInput { value, .. }) = fields.get(1) {
            complete_path(value)
        } else {
            vec![]
        };
        *completion_selected = None;
        true
    }

    /// Move the completion highlight up/down.  Returns `true` if consumed.
    fn try_navigate_completion(&mut self, down: bool) -> bool {
        let PopupState::WorkspaceCreate {
            focused_field,
            completions,
            completion_selected,
            ..
        } = &mut self.popup
        else {
            return false;
        };

        if *focused_field != 1 || completions.is_empty() {
            return false;
        }

        let next = match *completion_selected {
            Some(idx) => {
                if down {
                    if idx + 1 < completions.len() {
                        idx + 1
                    } else {
                        0
                    }
                } else {
                    if idx == 0 {
                        completions.len() - 1
                    } else {
                        idx - 1
                    }
                }
            }
            None => {
                if down {
                    0
                } else {
                    completions.len() - 1
                }
            }
        };
        *completion_selected = Some(next);
        true
    }

    /// Recompute path completions when the Path text field changes.
    fn refresh_completions(&mut self) {
        let PopupState::WorkspaceCreate {
            fields,
            focused_field,
            completions,
            completion_selected,
        } = &mut self.popup
        else {
            return;
        };

        if *focused_field != 1 {
            return;
        }

        if let Some(DialogField::TextInput { value, .. }) = fields.get(1) {
            *completions = complete_path(value);
            *completion_selected = None;
        }
    }

    fn dialog_field_count(&self) -> usize {
        match &self.popup {
            PopupState::WorkspaceCreate { fields, .. }
            | PopupState::RoomCreate { fields, .. }
            | PopupState::WorkspaceDelete { fields, .. }
            | PopupState::RoomDelete { fields, .. } => fields.len(),
            _ => 0,
        }
    }

    fn dialog_move_focus(&mut self, forward: bool) {
        let count = self.dialog_field_count();
        if count == 0 {
            return;
        }
        match &mut self.popup {
            PopupState::WorkspaceCreate { focused_field, .. }
            | PopupState::RoomCreate { focused_field, .. }
            | PopupState::WorkspaceDelete { focused_field, .. }
            | PopupState::RoomDelete { focused_field, .. } => {
                if forward {
                    *focused_field = (*focused_field + 1) % count;
                } else {
                    *focused_field = focused_field.checked_sub(1).unwrap_or(count - 1);
                }
            }
            _ => {}
        }
    }

    /// Left arrow: for Select fields cycle backwards; for Confirm toggle yes/no.
    fn dialog_field_left(&mut self) {
        match &mut self.popup {
            PopupState::WorkspaceCreate {
                fields,
                focused_field,
                ..
            }
            | PopupState::RoomCreate {
                fields,
                focused_field,
                ..
            }
            | PopupState::WorkspaceDelete {
                fields,
                focused_field,
                ..
            }
            | PopupState::RoomDelete {
                fields,
                focused_field,
                ..
            } => {
                let idx = *focused_field;
                if idx < fields.len() {
                    match &mut fields[idx] {
                        DialogField::Select {
                            options, selected, ..
                        } => {
                            if *selected > 0 {
                                *selected -= 1;
                            } else {
                                *selected = options.len().saturating_sub(1);
                            }
                        }
                        DialogField::Confirm { yes, .. } => {
                            *yes = !*yes;
                        }
                        DialogField::Checkbox { checked, .. } => {
                            *checked = !*checked;
                        }
                        DialogField::TextInput { .. } => {}
                    }
                }
            }
            _ => {}
        }
    }

    /// Right arrow: for Select fields cycle forwards; for Confirm toggle yes/no.
    fn dialog_field_right(&mut self) {
        match &mut self.popup {
            PopupState::WorkspaceCreate {
                fields,
                focused_field,
                ..
            }
            | PopupState::RoomCreate {
                fields,
                focused_field,
                ..
            }
            | PopupState::WorkspaceDelete {
                fields,
                focused_field,
                ..
            }
            | PopupState::RoomDelete {
                fields,
                focused_field,
                ..
            } => {
                let idx = *focused_field;
                if idx < fields.len() {
                    match &mut fields[idx] {
                        DialogField::Select {
                            options, selected, ..
                        } => {
                            *selected = (*selected + 1) % options.len().max(1);
                        }
                        DialogField::Confirm { yes, .. } => {
                            *yes = !*yes;
                        }
                        DialogField::Checkbox { checked, .. } => {
                            *checked = !*checked;
                        }
                        DialogField::TextInput { .. } => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn dialog_field_backspace(&mut self) {
        match &mut self.popup {
            PopupState::WorkspaceCreate {
                fields,
                focused_field,
                ..
            }
            | PopupState::RoomCreate {
                fields,
                focused_field,
                ..
            }
            | PopupState::WorkspaceDelete {
                fields,
                focused_field,
                ..
            }
            | PopupState::RoomDelete {
                fields,
                focused_field,
                ..
            } => {
                let idx = *focused_field;
                if idx < fields.len()
                    && let DialogField::TextInput { value, .. } = &mut fields[idx]
                {
                    value.pop();
                }
            }
            _ => {}
        }
    }

    fn dialog_field_insert(&mut self, c: char) {
        match &mut self.popup {
            PopupState::WorkspaceCreate {
                fields,
                focused_field,
                ..
            }
            | PopupState::RoomCreate {
                fields,
                focused_field,
                ..
            }
            | PopupState::WorkspaceDelete {
                fields,
                focused_field,
                ..
            }
            | PopupState::RoomDelete {
                fields,
                focused_field,
                ..
            } => {
                let idx = *focused_field;
                if idx < fields.len()
                    && let DialogField::TextInput { value, .. } = &mut fields[idx]
                {
                    value.push(c);
                }
            }
            _ => {}
        }
    }

    fn confirm_dialog(&mut self) {
        // Swap popup to None so we can consume the data.
        let popup = std::mem::replace(&mut self.popup, PopupState::None);
        match popup {
            PopupState::WorkspaceCreate { fields, .. } => {
                self.execute_workspace_create(fields);
            }
            PopupState::RoomCreate { fields, .. } => {
                self.execute_room_create(fields);
            }
            PopupState::WorkspaceDelete {
                fields,
                workspace_id,
                ..
            } => {
                self.execute_workspace_delete(fields, workspace_id);
            }
            PopupState::RoomDelete {
                fields,
                workspace_id,
                branch,
                ..
            } => {
                self.execute_room_delete(fields, workspace_id, branch);
            }
            other => {
                // Restore if we didn't handle it.
                self.popup = other;
            }
        }
    }

    fn execute_workspace_create(&mut self, fields: Vec<DialogField>) {
        // Field 0: Mode (Clone/Existing/New)
        // Field 1: Path
        // Field 2: URL (only used for Clone)
        let mode_idx = match &fields[0] {
            DialogField::Select { selected, .. } => *selected,
            _ => 0,
        };
        let path_str = match &fields[1] {
            DialogField::TextInput { value, .. } => value.clone(),
            _ => String::new(),
        };
        let mgr = WorkspaceManager::new();
        let result = match mode_idx {
            0 => {
                // Clone
                let url = match &fields[2] {
                    DialogField::TextInput { value, .. } => value.clone(),
                    _ => String::new(),
                };
                if url.is_empty() {
                    self.show_error("URL is required for Clone");
                    return;
                }
                let path_buf = if path_str.is_empty() {
                    let Some(home) = dirs::home_dir() else {
                        self.show_error(
                            "Could not determine home directory for default clone path",
                        );
                        return;
                    };
                    match default_clone_target_dir(&home, &url) {
                        Ok(path) => path,
                        Err(e) => {
                            self.show_error(e.to_string());
                            return;
                        }
                    }
                } else {
                    expand_workspace_path(&path_str)
                };
                let path = path_buf.as_path();
                mgr.clone_remote(&mut self.state, &url, path)
            }
            1 => {
                // Existing
                if path_str.is_empty() {
                    self.show_error("Path is required");
                    return;
                }
                let path_buf = expand_workspace_path(&path_str);
                let path = path_buf.as_path();
                mgr.register(&mut self.state, path)
            }
            _ => {
                // New
                if path_str.is_empty() {
                    self.show_error("Path is required");
                    return;
                }
                let path_buf = expand_workspace_path(&path_str);
                let path = path_buf.as_path();
                mgr.init(&mut self.state, path)
            }
        };
        match result {
            Ok(ws_id) => {
                // Auto-select the new workspace and its default room.
                self.workspace_selected = Some(ws_id);
                self.room_selected = None;
                self.switch_to_selected_room();
            }
            Err(e) => {
                self.show_error(e.to_string());
            }
        }
    }

    fn execute_room_create(&mut self, fields: Vec<DialogField>) {
        // Field 0: Branch name
        // Field 1: Base branch
        let branch = match &fields[0] {
            DialogField::TextInput { value, .. } => value.clone(),
            _ => String::new(),
        };
        let base_branch = match &fields[1] {
            DialogField::TextInput { value, .. } => value.clone(),
            _ => String::new(),
        };
        if branch.is_empty() {
            self.show_error("Branch name is required");
            return;
        }

        let ws_id = match self.state.active_workspace_id {
            Some(id) => id,
            None => {
                self.show_error("No active workspace");
                return;
            }
        };
        let ws = match self.state.ws_by_id(ws_id) {
            Some(w) => w,
            None => {
                self.show_error("Workspace not found");
                return;
            }
        };
        let ws_path = ws.path.clone();
        let ws_id_str = ws_id.to_string();
        let base = if base_branch.is_empty() {
            "HEAD"
        } else {
            &base_branch
        };
        let worktree_path = humu_dir().join("worktrees").join(&ws_id_str).join(&branch);
        let mgr = RoomManager::new();
        if let Err(e) = mgr.create(&ws_path, &branch, base, &worktree_path) {
            self.show_error(e.to_string());
            return;
        }

        let room_id = self
            .state
            .ws_by_id(ws_id)
            .and_then(|ws| ws.room_by_path(&worktree_path))
            .map(|room| room.id)
            .or_else(|| {
                humu::config::create_room_for_workspace(
                    &mut self.state,
                    ws_id,
                    &branch,
                    worktree_path.clone(),
                )
            });

        let Some(room_id) = room_id else {
            self.show_error("Failed to register new room");
            return;
        };

        self.workspace_selected = Some(ws_id);
        self.room_selected = Some(room_id);
        self.refresh_room_cache();
        self.rebuild_workspace_tree();
        self.switch_to_selected_room();
        self.mode = Mode::Terminal;
        self.focus = FocusedPanel::Terminal;
        self.workspace_mode_entered = None;
        if let Some(path) = self.current_room_path() {
            self.explorer_state = humu::explorer::ExplorerState::new(path);
            self.explorer_state.scan();
        }
    }

    fn execute_workspace_delete(&mut self, fields: Vec<DialogField>, workspace_id: WorkspaceId) {
        // Field 0: Confirm (yes/no)
        let confirmed = match &fields[0] {
            DialogField::Confirm { yes, .. } => *yes,
            _ => false,
        };
        if !confirmed {
            return;
        }
        // Field 1: Checkbox — delete directory from disk
        let remove_from_disk = match fields.get(1) {
            Some(DialogField::Checkbox { checked, .. }) => *checked,
            _ => false,
        };

        let was_active = Some(workspace_id) == self.state.active_workspace_id;

        // If the active workspace is being deleted and its panes are live,
        // clear them first (they'll be invalid after deletion).
        if was_active {
            self.clear_live_panes();
        }

        let mgr = WorkspaceManager::new();
        match mgr.delete(&mut self.state, workspace_id, remove_from_disk) {
            Ok(()) => {
                // Remove all suspended rooms belonging to the deleted workspace.
                self.clear_workspace_suspended_rooms(workspace_id);

                // Adjust selection if needed.
                if self.state.workspaces.is_empty() {
                    self.workspace_selected = None;
                    self.room_selected = None;
                    self.save_state();
                } else if was_active {
                    // Select first available workspace.
                    if let Some(first) = self.state.workspaces.first() {
                        self.workspace_selected = Some(first.id);
                    }
                    self.room_selected = None;
                    self.switch_to_selected_room();
                } else {
                    self.save_state();
                }
            }
            Err(e) => {
                self.show_error(e.to_string());
            }
        }
    }

    fn execute_room_delete(
        &mut self,
        fields: Vec<DialogField>,
        ws_id: WorkspaceId,
        branch: String,
    ) {
        let confirmed = match &fields[0] {
            DialogField::Confirm { yes, .. } => *yes,
            _ => false,
        };
        if !confirmed {
            return;
        }
        let ws = match self.state.ws_by_id(ws_id) {
            Some(w) => w,
            None => {
                self.show_error("Workspace not found");
                return;
            }
        };
        let Some(room_entry) = ws.rooms.iter().find(|room| room.name == branch).cloned() else {
            self.show_error("Room not found");
            return;
        };
        if room_entry.name == DEFAULT_ROOM_NAME || room_entry.path == ws.path {
            self.show_error("The local room cannot be deleted");
            return;
        }
        let ws_path = ws.path.clone();
        let worktree_path = room_entry.path.clone();
        let mgr = RoomManager::new();
        if let Err(e) = mgr.delete(&ws_path, &branch, &worktree_path) {
            self.show_error(e.to_string());
            return;
        }

        self.drop_room_runtime_state(ws_id, room_entry.id);
        self.remove_room_from_state(ws_id, room_entry.id);

        self.workspace_selected = Some(ws_id);
        self.room_selected = self.ensure_local_room(ws_id);
        self.refresh_room_cache();
        self.rebuild_workspace_tree();
        self.switch_to_selected_room();
        self.mode = Mode::Terminal;
        self.focus = FocusedPanel::Terminal;
        self.workspace_mode_entered = None;
        if let Some(path) = self.current_room_path() {
            self.explorer_state = humu::explorer::ExplorerState::new(path);
            self.explorer_state.scan();
        }
    }

    fn render(&mut self, frame: &mut ratatui::Frame) {
        let size = frame.area();

        // Main layout: [workspace | terminal | explorer] + status bar
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(size);

        let panel_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(self.panel_widths[0]),
                Constraint::Min(1),
                Constraint::Length(self.panel_widths[1]),
            ])
            .split(main_chunks[0]);

        // Store rects for mouse hit-testing.
        let tab_bar_rect = Rect::new(
            panel_chunks[1].x,
            panel_chunks[1].y,
            panel_chunks[1].width,
            1,
        );
        self.panel_rects = PanelRects {
            workspace: panel_chunks[0],
            terminal: panel_chunks[1],
            explorer: panel_chunks[2],
            tab_bar: tab_bar_rect,
            status_bar: main_chunks[1],
        };

        // Compute animated spinner frame (~100ms per frame at 50ms tick).
        let spinner_frame = SPINNER_FRAMES[self.spin_tick / 2 % SPINNER_FRAMES.len()];

        // Workspace tree panel (workspaces + rooms)
        let tree_items = self.workspace_tree_cache.clone();
        let ws_widget = WorkspacePanel::new(&tree_items, &self.palette, &self.ui_config)
            .selected(Some(self.selected_tree_index))
            .focus(self.focus == FocusedPanel::Workspace)
            .spinner(spinner_frame)
            .active(self.state.active_workspace_id, self.state.active_room_id);
        frame.render_widget(ws_widget, panel_chunks[0]);

        // Terminal area: tab bar (1 line) + pane area
        self.render_terminal_area(frame, panel_chunks[1]);

        // Explorer panel
        let explorer_widget = humu::tui::widgets::explorer_panel::ExplorerPanel::new(
            &self.explorer_state,
            &self.palette,
            &self.ui_config,
        )
        .focus(self.focus == FocusedPanel::Explorer);
        frame.render_widget(explorer_widget, panel_chunks[2]);

        // Status bar
        let mut status = StatusBar::new(self.mode, &self.palette, &self.ui_config);
        if let Some(ref state) = self.search_state {
            status = status
                .search_query(Some(&state.query))
                .search_valid(state.is_valid_regex());
            if self.mode == Mode::Search {
                let active = state.active_index.map(|i| i + 1).unwrap_or(0);
                let total = state.matches.len();
                status =
                    status.search_info(Some((active, total, state.case_sensitive, state.wrap)));
            }
        }
        frame.render_widget(status, main_chunks[1]);

        // Render popup on top of everything when active.
        self.render_popup(frame, size);
    }

    fn render_log_viewer(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
        lines: &[String],
        scroll: usize,
        h_scroll: usize,
    ) {
        use ratatui::style::{Modifier, Style};
        use ratatui::widgets::{Block, BorderType, Borders, Clear, Widget};

        let width = (area.width - 4).min(100);
        let height = (area.height - 2).min(40);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup = Rect::new(x, y, width, height);

        Clear.render(popup, frame.buffer_mut());

        let border_type = if self.ui_config.rounded_corners {
            BorderType::Rounded
        } else {
            BorderType::Plain
        };
        let title = format!(" Logs ({}/{}) ", scroll + 1, lines.len().max(1));
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.palette.accent_blue))
            .border_type(border_type);
        let inner = block.inner(popup);
        block.render(popup, frame.buffer_mut());

        let visible_height = inner.height as usize;
        let max_width = inner.width as usize;
        let start = scroll.saturating_sub(visible_height.saturating_sub(1));
        let text_style = Style::default().fg(self.palette.fg_primary);
        let muted_style = Style::default().fg(self.palette.fg_muted);
        let ellipsis_style = Style::default().fg(self.palette.fg_muted);

        for (i, line_idx) in (start..lines.len()).enumerate() {
            if i >= visible_height {
                break;
            }
            let line = &lines[line_idx];
            let style = if line_idx == scroll {
                text_style.add_modifier(Modifier::BOLD)
            } else {
                muted_style
            };

            let chars: Vec<char> = line.chars().collect();
            let total_chars = chars.len();
            let visible: String = chars.iter().skip(h_scroll).take(max_width).collect();
            let truncated_left = h_scroll > 0;
            let truncated_right = h_scroll + max_width < total_chars;

            if truncated_left && max_width > 3 {
                frame
                    .buffer_mut()
                    .set_string(inner.x, inner.y + i as u16, "...", ellipsis_style);
                let rest: String = chars
                    .iter()
                    .skip(h_scroll + 3)
                    .take(max_width - 3)
                    .collect();
                frame
                    .buffer_mut()
                    .set_string(inner.x + 3, inner.y + i as u16, &rest, style);
            } else {
                frame
                    .buffer_mut()
                    .set_string(inner.x, inner.y + i as u16, &visible, style);
            }
            if truncated_right && max_width > 3 {
                let col = inner.x + max_width as u16 - 3;
                frame
                    .buffer_mut()
                    .set_string(col, inner.y + i as u16, "...", ellipsis_style);
            }
        }
    }

    fn render_popup(&self, frame: &mut ratatui::Frame, area: Rect) {
        match &self.popup {
            PopupState::None => {}
            PopupState::Settings { selected } => {
                let items: Vec<String> =
                    Self::SETTINGS_ITEMS.iter().map(|s| s.to_string()).collect();
                frame.render_widget(
                    PresetSelector::new(&items, *selected, &self.palette, &self.ui_config)
                        .title(" Settings "),
                    area,
                );
            }
            PopupState::SplitDirection => {
                let items = vec!["\u{2193} Down".to_string(), "\u{2192} Right".to_string()];
                frame.render_widget(
                    PresetSelector::new(&items, usize::MAX, &self.palette, &self.ui_config)
                        .title(" Press \u{2193} or \u{2192} "),
                    area,
                );
            }
            PopupState::LogViewer {
                lines,
                scroll,
                h_scroll,
                ..
            } => {
                self.render_log_viewer(frame, area, lines, *scroll, *h_scroll);
            }
            PopupState::PresetSelector {
                presets, selected, ..
            } => {
                frame.render_widget(
                    PresetSelector::new(presets, *selected, &self.palette, &self.ui_config),
                    area,
                );
            }
            PopupState::WorkspaceCreate {
                fields,
                focused_field,
                completions,
                completion_selected,
            } => {
                let mut dialog = Dialog::new(
                    "Create Workspace",
                    fields,
                    *focused_field,
                    &self.palette,
                    &self.ui_config,
                );
                dialog.completions = completions;
                dialog.completion_selected = *completion_selected;
                dialog.completion_field = Some(1); // Path field
                frame.render_widget(dialog, area);
            }
            PopupState::RoomCreate {
                fields,
                focused_field,
            } => {
                frame.render_widget(
                    Dialog::new(
                        "Create Room",
                        fields,
                        *focused_field,
                        &self.palette,
                        &self.ui_config,
                    ),
                    area,
                );
            }
            PopupState::WorkspaceDelete {
                fields,
                focused_field,
                ..
            } => {
                frame.render_widget(
                    Dialog::new(
                        "Delete Workspace",
                        fields,
                        *focused_field,
                        &self.palette,
                        &self.ui_config,
                    ),
                    area,
                );
            }
            PopupState::RoomDelete {
                fields,
                focused_field,
                ..
            } => {
                frame.render_widget(
                    Dialog::new(
                        "Delete Room",
                        fields,
                        *focused_field,
                        &self.palette,
                        &self.ui_config,
                    ),
                    area,
                );
            }
            PopupState::NotificationSettings { selected } => {
                let items = self.notification_settings_items();
                let selector =
                    PresetSelector::new(&items, *selected, &self.palette, &self.ui_config)
                        .title(" Notifications ");
                frame.render_widget(selector, area);
            }
            PopupState::NotificationTokenInput { field, value } => {
                let title = match field {
                    NotificationField::BotToken => " Bot Token ",
                    NotificationField::ChatId => " Chat ID ",
                };
                let fields = vec![DialogField::TextInput {
                    label: title.trim().to_string(),
                    value: value.clone(),
                }];
                let dialog = Dialog::new(title.trim(), &fields, 0, &self.palette, &self.ui_config);
                frame.render_widget(dialog, area);
            }
            PopupState::FloatingPane { pane_id, title } => {
                use humu::tui::widgets::terminal_widget::TerminalWidget;
                use ratatui::widgets::Clear;

                let popup_area = self.floating_pane_area();

                frame.render_widget(Clear, popup_area);

                if let Some(pane) = self.local_panes.get(pane_id) {
                    let screen = pane.screen_snapshot();
                    let tw = TerminalWidget::new(&screen, title, &self.palette, &self.ui_config)
                        .focus(true)
                        .pane_count(1);
                    frame.render_widget(tw, popup_area);

                    // Show cursor inside the floating pane.
                    if !screen.hide_cursor() {
                        let (crow, ccol) = screen.cursor_position();
                        let cx = popup_area.x + 1 + ccol;
                        let cy = popup_area.y + 1 + crow;
                        if cx < popup_area.x + popup_area.width
                            && cy < popup_area.y + popup_area.height
                        {
                            frame.set_cursor_position(Position::new(cx, cy));
                        }
                    }
                }
            }
            PopupState::ErrorDialog { message } => {
                use ratatui::style::{Modifier, Style};
                use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

                let lines: Vec<&str> = message.lines().collect();
                let max_line_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(10);
                let width = (max_line_len as u16 + 6).min(area.width - 4).max(20);
                let height = (lines.len() as u16 + 4).min(area.height - 2).max(5);
                let x = area.x + (area.width.saturating_sub(width)) / 2;
                let y = area.y + (area.height.saturating_sub(height)) / 2;
                let popup_area = Rect::new(x, y, width, height);

                frame.render_widget(Clear, popup_area);

                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(self.palette.accent_red))
                    .title(" Error ")
                    .title_style(
                        Style::default()
                            .fg(self.palette.accent_red)
                            .add_modifier(Modifier::BOLD),
                    );

                let paragraph = Paragraph::new(message.as_str())
                    .style(Style::default().fg(self.palette.fg_primary))
                    .block(block)
                    .wrap(Wrap { trim: false });

                frame.render_widget(paragraph, popup_area);
            }
            PopupState::ExplorerNewEntry { is_dir, value } => {
                use ratatui::style::{Modifier, Style};
                use ratatui::widgets::{Block, Borders, Clear, Paragraph};

                let title = if *is_dir {
                    " New Directory "
                } else {
                    " New File "
                };
                let width = 40u16.min(area.width - 4);
                let height = 3u16;
                let x = area.x + (area.width.saturating_sub(width)) / 2;
                let y = area.y + (area.height.saturating_sub(height)) / 2;
                let popup_area = Rect::new(x, y, width, height);

                frame.render_widget(Clear, popup_area);

                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(self.palette.accent_blue))
                    .title(title)
                    .title_style(
                        Style::default()
                            .fg(self.palette.accent_blue)
                            .add_modifier(Modifier::BOLD),
                    );

                let display = format!("{}\u{2588}", value); // value + cursor block
                let paragraph = Paragraph::new(display)
                    .style(Style::default().fg(self.palette.fg_primary))
                    .block(block);

                frame.render_widget(paragraph, popup_area);
            }
            PopupState::ExplorerDeleteConfirm {
                name, ok_selected, ..
            } => {
                use ratatui::style::{Modifier, Style};
                use ratatui::widgets::{Block, Borders, Clear};

                let msg = format!("Delete \"{}\"?", name);
                let width = (msg.chars().count() as u16 + 6).min(area.width - 4).max(24);
                let height = 5u16;
                let x = area.x + (area.width.saturating_sub(width)) / 2;
                let y = area.y + (area.height.saturating_sub(height)) / 2;
                let popup_area = Rect::new(x, y, width, height);

                frame.render_widget(Clear, popup_area);

                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(self.palette.accent_red))
                    .title(" Confirm Delete ")
                    .title_style(
                        Style::default()
                            .fg(self.palette.accent_red)
                            .add_modifier(Modifier::BOLD),
                    );
                let inner = block.inner(popup_area);
                frame.render_widget(block, popup_area);

                // Message line
                let msg_style = Style::default().fg(self.palette.fg_primary);
                frame
                    .buffer_mut()
                    .set_string(inner.x + 1, inner.y, &msg, msg_style);

                // Buttons line
                let btn_y = inner.y + 2;
                let cancel_label = " Cancel ";
                let ok_label = "  OK  ";
                let total_btn_width = cancel_label.len() + 2 + ok_label.len();
                let btn_x = inner.x + (inner.width.saturating_sub(total_btn_width as u16)) / 2;

                let cancel_style = if !ok_selected {
                    Style::default()
                        .fg(self.palette.bg_primary)
                        .bg(self.palette.accent_blue)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(self.palette.fg_secondary)
                        .bg(self.palette.bg_tertiary)
                };
                let ok_style = if *ok_selected {
                    Style::default()
                        .fg(self.palette.bg_primary)
                        .bg(self.palette.accent_red)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(self.palette.fg_secondary)
                        .bg(self.palette.bg_tertiary)
                };

                frame
                    .buffer_mut()
                    .set_string(btn_x, btn_y, cancel_label, cancel_style);
                frame.buffer_mut().set_string(
                    btn_x + cancel_label.len() as u16 + 2,
                    btn_y,
                    ok_label,
                    ok_style,
                );
            }
        }
    }

    fn render_terminal_area(&mut self, frame: &mut ratatui::Frame, area: Rect) {
        if area.height == 0 {
            return;
        }

        // Split into tab bar (1 row) and pane content area
        let tab_bar_area = Rect::new(area.x, area.y, area.width, 1);
        let pane_area = self.terminal_pane_area();

        // Pre-compute search highlight data for the focused pane.
        // Search matches use viewport-relative rows (0 = top of visible area).
        let search_base_row = 0;
        let (search_matches, search_active) = match &self.search_state {
            Some(state) if matches!(self.mode, Mode::EnterSearch | Mode::Search) => {
                (state.matches.clone(), state.active_index)
            }
            _ => (Vec::new(), None),
        };

        // Render tab bar — a tab is active if any of its panes has a non-Idle
        // agent state in agent_states.
        let tab_names: Vec<&str> = self.tabs.tab_names();
        let active_indicators: Vec<bool> = (0..tab_names.len())
            .map(|i| {
                let tree = match self.tabs.tree_at(i) {
                    Some(t) => t,
                    None => return false,
                };
                tree.pane_ids().iter().any(|pid| {
                    self.agent_states
                        .get(pid)
                        .map(|e| matches!(e.state, AgentState::Working | AgentState::NeedsInput))
                        .unwrap_or(false)
                })
            })
            .collect();
        let spinner_frame = SPINNER_FRAMES[self.spin_tick / 2 % SPINNER_FRAMES.len()];
        let tab_bar = TabBar::new(
            &tab_names,
            self.tabs.active_index(),
            &active_indicators,
            &self.palette,
            &self.ui_config,
        )
        .spinner(spinner_frame);
        frame.render_widget(tab_bar, tab_bar_area);

        // Render panes from active tab's split tree
        if pane_area.height == 0 {
            return;
        }

        // Fullscreen mode: render only the fullscreen pane filling the whole area.
        if let Some(fs_id) = self.fullscreen_pane {
            if let Some(pane) = self.local_panes.get_mut(&fs_id) {
                let inner_w = pane_area.width.saturating_sub(2);
                let inner_h = pane_area.height.saturating_sub(2);
                if pane.cols() != inner_w || pane.rows() != inner_h {
                    let _ = pane.resize(inner_w, inner_h);
                }
            }
            let fs_exit_code = self.pane_exit_code(fs_id);
            let fs_pane_count = self
                .tabs
                .active_tree()
                .map(|t| t.pane_ids().len())
                .unwrap_or(1);
            if let Some(pane) = self.local_panes.get(&fs_id) {
                let screen = pane.screen_snapshot();
                let preset_name = self
                    .pane_presets
                    .get(&fs_id)
                    .map(|s| s.as_str())
                    .unwrap_or("shell");
                let sel = self.selection_for_pane(fs_id);
                let widget =
                    TerminalWidget::new(&screen, preset_name, &self.palette, &self.ui_config)
                        .focus(true)
                        .exited(fs_exit_code)
                        .pane_count(fs_pane_count)
                        .search(&search_matches, search_active, search_base_row)
                        .selection(sel);
                frame.render_widget(widget, pane_area);
                if fs_exit_code.is_none() && screen.scrollback() == 0 {
                    let (crow, ccol) = screen.cursor_position();
                    let cx = pane_area.x + 1 + ccol;
                    let cy = pane_area.y + 1 + crow;
                    if cx < pane_area.right() - 1 && cy < pane_area.bottom() - 1 {
                        frame.set_cursor_position(Position::new(cx, cy));
                    }
                }
            } else if let Some(pane) = self.attached_pane_snapshot(fs_id) {
                let preset_name = self
                    .pane_presets
                    .get(&fs_id)
                    .map(|s| s.as_str())
                    .unwrap_or(pane.preset_name.as_str());
                let sel = self.selection_for_pane(fs_id);
                let widget = TerminalWidget::from_snapshot(
                    &pane.screen,
                    pane.capabilities.scrollback_offset,
                    preset_name,
                    &self.palette,
                    &self.ui_config,
                )
                .focus(true)
                .exited(fs_exit_code)
                .pane_count(fs_pane_count)
                .search(&search_matches, search_active, search_base_row)
                .selection(sel);
                frame.render_widget(widget, pane_area);
                if fs_exit_code.is_none()
                    && pane.capabilities.scrollback_offset == 0
                    && pane.screen.cursor.visible
                {
                    let cx = pane_area.x + 1 + pane.screen.cursor.col;
                    let cy = pane_area.y + 1 + pane.screen.cursor.row;
                    if cx < pane_area.right() - 1 && cy < pane_area.bottom() - 1 {
                        frame.set_cursor_position(Position::new(cx, cy));
                    }
                }
            }
            return;
        }

        if let Some(tree) = self.tabs.active_tree() {
            let rects = tree.compute_rects(pane_area);
            let pane_count = rects.len();
            for (pane_id, rect) in &rects {
                // Resize pane if its dimensions have changed since last render.
                let inner_w = rect.width.saturating_sub(2);
                let inner_h = rect.height.saturating_sub(2);
                if let Some(pane) = self.local_panes.get_mut(pane_id)
                    && (pane.cols() != inner_w || pane.rows() != inner_h)
                {
                    let _ = pane.resize(inner_w, inner_h);
                }
            }
            // Collect exit codes while we still have mutable access.
            let exit_codes: HashMap<PaneId, Option<i32>> = rects
                .iter()
                .map(|(pid, _)| (*pid, self.pane_exit_code(*pid)))
                .collect();
            for (pane_id, rect) in rects {
                if let Some(pane) = self.local_panes.get(&pane_id) {
                    let screen = pane.screen_snapshot();
                    let is_focused =
                        self.focused_pane == Some(pane_id) && self.focus == FocusedPanel::Terminal;
                    let preset_name = self
                        .pane_presets
                        .get(&pane_id)
                        .map(|s| s.as_str())
                        .unwrap_or("shell");
                    let exit_code = exit_codes.get(&pane_id).copied().flatten();
                    let sel = self.selection_for_pane(pane_id);
                    let widget =
                        TerminalWidget::new(&screen, preset_name, &self.palette, &self.ui_config)
                            .focus(is_focused)
                            .exited(exit_code)
                            .pane_count(pane_count)
                            .search(
                                if is_focused { &search_matches } else { &[] },
                                if is_focused { search_active } else { None },
                                search_base_row,
                            )
                            .selection(sel);
                    frame.render_widget(widget, rect);
                    if is_focused
                        && exit_code.is_none()
                        && !screen.hide_cursor()
                        && screen.scrollback() == 0
                    {
                        let (crow, ccol) = screen.cursor_position();
                        let cx = rect.x + 1 + ccol;
                        let cy = rect.y + 1 + crow;
                        if cx < rect.right() - 1 && cy < rect.bottom() - 1 {
                            frame.set_cursor_position(Position::new(cx, cy));
                        }
                    }
                } else if let Some(pane) = self.attached_pane_snapshot(pane_id) {
                    let is_focused =
                        self.focused_pane == Some(pane_id) && self.focus == FocusedPanel::Terminal;
                    let preset_name = self
                        .pane_presets
                        .get(&pane_id)
                        .map(|s| s.as_str())
                        .unwrap_or(pane.preset_name.as_str());
                    let exit_code = exit_codes.get(&pane_id).copied().flatten();
                    let sel = self.selection_for_pane(pane_id);
                    let widget = TerminalWidget::from_snapshot(
                        &pane.screen,
                        pane.capabilities.scrollback_offset,
                        preset_name,
                        &self.palette,
                        &self.ui_config,
                    )
                    .focus(is_focused)
                    .exited(exit_code)
                    .pane_count(pane_count)
                    .search(
                        if is_focused { &search_matches } else { &[] },
                        if is_focused { search_active } else { None },
                        search_base_row,
                    )
                    .selection(sel);
                    frame.render_widget(widget, rect);
                    if is_focused
                        && exit_code.is_none()
                        && pane.screen.cursor.visible
                        && pane.capabilities.scrollback_offset == 0
                    {
                        let cx = rect.x + 1 + pane.screen.cursor.col;
                        let cy = rect.y + 1 + pane.screen.cursor.row;
                        if cx < rect.right() - 1 && cy < rect.bottom() - 1 {
                            frame.set_cursor_position(Position::new(cx, cy));
                        }
                    }
                }
            }
        }
    }

    fn handle_action(&mut self, action: Action) {
        match action {
            Action::EnterMode(mode) => {
                // Clear search state when switching away from search modes.
                if matches!(self.mode, Mode::EnterSearch | Mode::Search)
                    && !matches!(mode, Mode::EnterSearch | Mode::Search)
                {
                    self.search_state = None;
                }
                // Initialize search state when entering EnterSearch.
                if mode == Mode::EnterSearch && self.search_state.is_none() {
                    self.search_state = Some(SearchState::new());
                }
                self.mode = mode;
                // Track workspace mode entry for auto-timeout.
                if mode == Mode::Workspace {
                    self.workspace_mode_entered = Some(std::time::Instant::now());
                } else {
                    self.workspace_mode_entered = None;
                }
                match mode {
                    Mode::Workspace => self.focus = FocusedPanel::Workspace,
                    Mode::Explorer => {
                        self.focus = FocusedPanel::Explorer;
                        if let Some(path) = self.current_room_path() {
                            if self.explorer_state.root != path {
                                self.explorer_state = humu::explorer::ExplorerState::new(path);
                            }
                            self.explorer_state.scan();
                        }
                    }
                    _ => self.focus = FocusedPanel::Terminal,
                }
            }
            Action::Quit => self.running = false,

            Action::FocusWorkspacePanel => self.focus = FocusedPanel::Workspace,

            Action::NavigateUp => self.navigate(-1),
            Action::NavigateDown => self.navigate(1),
            Action::Select => self.select_current(),

            Action::PassThrough(key) => self.handle_passthrough(key),

            // Tab actions
            Action::NewTab => self.show_preset_selector(PresetAction::NewTab),
            Action::CloseTab => self.close_tab(),
            Action::PrevTab => {
                let active = self.tabs.active_index();
                if active > 0 {
                    self.tabs.set_active(active - 1);
                    self.sync_focused_pane();
                }
            }
            Action::NextTab => {
                let active = self.tabs.active_index();
                if active + 1 < self.tabs.len() {
                    self.tabs.set_active(active + 1);
                    self.sync_focused_pane();
                }
            }
            Action::GoToTab(n) => {
                if n < self.tabs.len() {
                    self.tabs.set_active(n);
                    self.sync_focused_pane();
                }
            }

            // Pane actions
            Action::NewPane => {
                self.popup = PopupState::SplitDirection;
            }
            Action::SplitDown => self.show_preset_selector(PresetAction::SplitDown),
            Action::SplitRight => self.show_preset_selector(PresetAction::SplitRight),
            Action::ClosePane => self.close_pane(),
            Action::MoveFocus(dir) => self.move_focus(dir),
            Action::ToggleFullscreen => self.toggle_fullscreen(),

            // Workspace/room actions
            Action::Create => self.show_create_dialog(),
            Action::CreateWorkspace => self.show_create_workspace_dialog(),
            Action::Delete => self.show_delete_dialog(),

            // Settings
            Action::OpenSettings => {
                self.popup = PopupState::Settings { selected: 0 };
            }

            // Resize actions
            Action::Resize(dir) => self.handle_resize_action(dir),

            // Search actions
            Action::SearchInput(key) => {
                if let Some(ref mut state) = self.search_state {
                    match key.code {
                        KeyCode::Char(c) => {
                            state.query.push(c);
                            self.run_search();
                        }
                        KeyCode::Backspace => {
                            state.query.pop();
                            self.run_search();
                        }
                        _ => {}
                    }
                }
            }
            Action::SearchConfirm => {
                if let Some(ref state) = self.search_state {
                    if state.query.is_empty() {
                        self.search_state = None;
                        self.mode = Mode::Terminal;
                    } else {
                        self.mode = Mode::Search;
                    }
                }
            }
            Action::SearchCancel => {
                self.search_state = None;
                self.mode = Mode::Terminal;
            }
            Action::SearchNext => {
                if let Some(ref mut state) = self.search_state {
                    if state.next() {
                        self.scroll_to_active_match();
                    }
                }
            }
            Action::SearchPrev => {
                if let Some(ref mut state) = self.search_state {
                    if state.prev() {
                        self.scroll_to_active_match();
                    }
                }
            }
            Action::SearchToggleCase => {
                if let Some(ref mut state) = self.search_state {
                    state.case_sensitive = !state.case_sensitive;
                    self.run_search();
                }
            }
            Action::SearchToggleWrap => {
                if let Some(ref mut state) = self.search_state {
                    state.wrap = !state.wrap;
                }
            }
            Action::ScrollUp => {
                if let Some(pane_id) = self.focused_pane {
                    if let Some(pane) = self.local_panes.get(&pane_id) {
                        pane.scrollback_up(1);
                    }
                }
                if self.search_state.is_some() {
                    self.run_search();
                }
            }
            Action::ScrollDown => {
                if let Some(pane_id) = self.focused_pane {
                    if let Some(pane) = self.local_panes.get(&pane_id) {
                        pane.scrollback_down(1);
                    }
                }
                if self.search_state.is_some() {
                    self.run_search();
                }
            }
            Action::ScrollPageUp => {
                if let Some(pane_id) = self.focused_pane {
                    if let Some(pane) = self.local_panes.get(&pane_id) {
                        let page = pane.rows() as usize;
                        pane.scrollback_up(page);
                    }
                }
                if self.search_state.is_some() {
                    self.run_search();
                }
            }
            Action::ScrollPageDown => {
                if let Some(pane_id) = self.focused_pane {
                    if let Some(pane) = self.local_panes.get(&pane_id) {
                        let page = pane.rows() as usize;
                        pane.scrollback_down(page);
                    }
                }
                if self.search_state.is_some() {
                    self.run_search();
                }
            }

            Action::DiffFile => {
                self.explorer_diff_file();
            }
            Action::ToggleIgnored => {
                self.explorer_state.show_ignored = !self.explorer_state.show_ignored;
                self.explorer_state.scan();
            }

            Action::CopyPath => {
                if let Some(entry) = self.explorer_state.selected_entry() {
                    let abs_path = entry.path.to_string_lossy();
                    use std::io::Write;
                    let encoded = base64_encode(abs_path.as_bytes());
                    let osc = format!("\x1b]52;c;{}\x07", encoded);
                    let _ = stdout().write_all(osc.as_bytes());
                    let _ = stdout().flush();
                }
            }

            Action::NewFile => {
                self.popup = PopupState::ExplorerNewEntry {
                    is_dir: false,
                    value: String::new(),
                };
            }

            Action::NewDir => {
                self.popup = PopupState::ExplorerNewEntry {
                    is_dir: true,
                    value: String::new(),
                };
            }

            Action::DeleteEntry => {
                if let Some(entry) = self.explorer_state.selected_entry() {
                    let path = entry.path.clone();
                    let name = entry.name.clone();
                    self.popup = PopupState::ExplorerDeleteConfirm {
                        path,
                        name,
                        ok_selected: false,
                    };
                }
            }

            _ => {}
        }
    }

    fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent) {
        // Forward mouse events to floating pane if active and within its area.
        if let PopupState::FloatingPane { pane_id, .. } = &self.popup {
            let pane_id = *pane_id;
            if self.forward_mouse_to_floating_pane(pane_id, &mouse) {
                return;
            }
        }

        if !matches!(
            self.popup,
            PopupState::None | PopupState::FloatingPane { .. }
        ) {
            if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                self.pty_mouse_active = false;
                self.selection = None;
                let _ = self.handle_dialog_mouse(mouse.column, mouse.row);
            }
            return;
        }

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.pty_mouse_active = false;
                self.selection = None;
                let pos = Position::new(mouse.column, mouse.row);
                self.handle_click(mouse.column, mouse.row);
                if self.pane_at(pos).is_some() && self.try_forward_mouse(&mouse) {
                    self.pty_mouse_active = self
                        .focused_pane
                        .and_then(|pane_id| self.pane_input_state(pane_id))
                        .is_some_and(|state| {
                            state.mouse_mode != humu::pty::terminal::MouseProtocolMode::None
                        });
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.pty_mouse_active || self.selection.is_some() {
                    let _ = self.try_forward_mouse(&mouse);
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if self.pty_mouse_active || self.selection.is_some() {
                    let _ = self.try_forward_mouse(&mouse);
                }
                self.pty_mouse_active = false;
            }
            MouseEventKind::Down(_) | MouseEventKind::Up(_) => {
                let pos = Position::new(mouse.column, mouse.row);
                if self.pane_at(pos).is_some() || self.pty_mouse_active || self.selection.is_some()
                {
                    let _ = self.try_forward_mouse(&mouse);
                }
            }
            MouseEventKind::Drag(_) => {
                if self.pty_mouse_active {
                    let _ = self.try_forward_mouse(&mouse);
                }
            }
            MouseEventKind::ScrollUp => {
                if self
                    .pane_at(Position::new(mouse.column, mouse.row))
                    .is_none()
                    || !self.try_forward_mouse(&mouse)
                {
                    self.handle_scroll(mouse.column, mouse.row, true);
                }
            }
            MouseEventKind::ScrollDown => {
                if self
                    .pane_at(Position::new(mouse.column, mouse.row))
                    .is_none()
                    || !self.try_forward_mouse(&mouse)
                {
                    self.handle_scroll(mouse.column, mouse.row, false);
                }
            }
            _ => {}
        }
    }

    fn focused_pane_context(&self, mouse: &crossterm::event::MouseEvent) -> Option<(PaneId, Rect)> {
        let pane_id = self.focused_pane?;
        let pane_rect = self
            .pane_at(Position::new(mouse.column, mouse.row))
            .map(|(_, rect)| rect)
            .or_else(|| {
                let pane_area = self.terminal_pane_area();
                self.tabs.active_tree().and_then(|t| {
                    t.compute_rects(pane_area)
                        .into_iter()
                        .find(|(id, _)| *id == pane_id)
                        .map(|(_, rect)| rect)
                })
            })
            .unwrap_or_else(|| self.terminal_pane_area());
        Some((pane_id, pane_rect))
    }

    fn selection_for_pane(&self, pane_id: PaneId) -> Option<(u16, u16, u16, u16)> {
        let sel = self.selection.as_ref()?;
        if sel.pane_id != pane_id {
            return None;
        }
        let (sr, sc, er, ec) = if sel.start <= sel.end {
            (sel.start.0, sel.start.1, sel.end.0, sel.end.1)
        } else {
            (sel.end.0, sel.end.1, sel.start.0, sel.start.1)
        };
        Some((sr, sc, er, ec))
    }

    fn copy_selection_to_clipboard(&self) {
        let sel = match &self.selection {
            Some(s) => s,
            None => return,
        };
        let (start_row, start_col, end_row, end_col) = if sel.start <= sel.end {
            (sel.start.0, sel.start.1, sel.end.0, sel.end.1)
        } else {
            (sel.end.0, sel.end.1, sel.start.0, sel.start.1)
        };

        let mut text = String::new();
        if let Some(pane) = self.local_panes.get(&sel.pane_id) {
            let screen = pane.screen_snapshot();
            let cols = screen.size().1;
            for row in start_row..=end_row {
                let from = if row == start_row { start_col } else { 0 };
                let to = if row == end_row {
                    end_col
                } else {
                    cols.saturating_sub(1)
                };
                for col in from..=to {
                    if let Some(cell) = screen.cell(row, col) {
                        if cell.is_wide_continuation() {
                            continue;
                        }
                        let contents = cell.contents();
                        if contents.is_empty() {
                            text.push(' ');
                        } else {
                            text.push_str(&contents);
                        }
                    }
                }
                if row < end_row {
                    let trimmed = text.trim_end_matches(' ');
                    text.truncate(trimmed.len());
                    text.push('\n');
                }
            }
        } else if let Some(pane) = self.attached_pane_snapshot(sel.pane_id) {
            let cols = pane.screen.cols;
            for row in start_row..=end_row {
                let from = if row == start_row { start_col } else { 0 };
                let to = if row == end_row {
                    end_col
                } else {
                    cols.saturating_sub(1)
                };
                for col in from..=to {
                    if let Some(cell) = pane
                        .screen
                        .cells
                        .get(row as usize)
                        .and_then(|cells| cells.get(col as usize))
                    {
                        if cell.wide_continuation {
                            continue;
                        }
                        let contents = cell.contents();
                        if contents.is_empty() {
                            text.push(' ');
                        } else {
                            text.push_str(contents);
                        }
                    }
                }
                if row < end_row {
                    let trimmed = text.trim_end_matches(' ');
                    text.truncate(trimmed.len());
                    text.push('\n');
                }
            }
        } else {
            return;
        }
        let trimmed = text.trim_end();
        if trimmed.is_empty() {
            return;
        }

        // OSC 52 clipboard: \x1b]52;c;{base64}\x07
        use std::io::Write;
        let encoded = base64_encode(trimmed.as_bytes());
        let osc = format!("\x1b]52;c;{}\x07", encoded);
        let _ = stdout().write_all(osc.as_bytes());
        let _ = stdout().flush();
    }

    /// Handle a left-button mouse click at terminal coordinates (x, y).
    fn handle_click(&mut self, x: u16, y: u16) {
        let pos = Position::new(x, y);
        if self.panel_rects.workspace.contains(pos) {
            let was_workspace_focused =
                self.mode == Mode::Workspace && self.focus == FocusedPanel::Workspace;
            self.handle_action(Action::EnterMode(Mode::Workspace));
            let visual_row = y.saturating_sub(self.panel_rects.workspace.y + 1) as usize;
            let tree = self.workspace_tree_cache.clone();
            if let Some(idx) = Self::visual_row_to_tree_index(&tree, visual_row) {
                let activate = was_workspace_focused && self.selected_tree_index == idx;
                self.selected_tree_index = idx;
                let item = &tree[idx];
                self.select_workspace_tree_item(item);
                if activate {
                    self.activate_selected_workspace_tree_item();
                }
            }
        } else if self.panel_rects.tab_bar.contains(pos) {
            // Determine which tab or "+" was clicked.
            self.handle_tab_bar_click(x);
        } else if self.panel_rects.explorer.contains(pos) {
            let was_focused = self.focus == FocusedPanel::Explorer;
            self.mode = Mode::Explorer;
            self.focus = FocusedPanel::Explorer;
            // Map click y to tree entry (accounting for border + scroll offset)
            let inner_y = y.saturating_sub(self.panel_rects.explorer.y + 1);
            let clicked_index = self.explorer_state.scroll_offset + inner_y as usize;
            if clicked_index < self.explorer_state.entries.len() {
                if was_focused && clicked_index == self.explorer_state.selected {
                    // Focused + same item → open (same as Enter)
                    self.explorer_select();
                } else {
                    // Not focused or different item → just select
                    self.explorer_state.selected = clicked_index;
                }
            }
        } else if self.panel_rects.terminal.contains(pos) {
            self.mode = Mode::Terminal;
            self.focus = FocusedPanel::Terminal;
            if let Some((pane_id, _)) = self.pane_at(pos) {
                self.focused_pane = Some(pane_id);
            }
        } else if self.panel_rects.status_bar.contains(pos) {
            self.handle_status_bar_click(x);
        }
    }

    /// Handle a click on a status bar hint segment.
    fn handle_status_bar_click(&mut self, click_x: u16) {
        // Modes without clickable hints
        if matches!(self.mode, Mode::EnterSearch | Mode::Locked) {
            return;
        }

        let area = self.panel_rects.status_bar;
        let mut x = area.x;

        // Mode badge: " MODE " + separator
        let label = status_bar::mode_label(self.mode);
        x += label.chars().count() as u16 + 2 + 1;

        // Terminal mode has a "Ctrl+" prefix block
        if self.mode == Mode::Terminal {
            x += 1 + 8 + 1; // sep + " Ctrl + " + sep
        }

        // Search mode: skip over query prefix, then test search-specific hints
        if self.mode == Mode::Search {
            if let Some(ref state) = self.search_state {
                // " / {query} " rendered before hints
                x += 3 + state.query.chars().count() as u16 + 1;

                let hints: [(&str, &str); 4] = [
                    ("n", "NEXT"),
                    ("N", "PREV"),
                    ("c", "CASE"), // "CASE"/"case" both 4 chars
                    ("w", "WRAP"), // "WRAP"/"wrap" both 4 chars
                ];
                for (i, (key, lbl)) in hints.iter().enumerate() {
                    let seg = status_bar::hint_segment_width(key, lbl);
                    if click_x >= x && click_x < x + seg {
                        if let Some(action) = hint_click_action(self.mode, i) {
                            self.handle_action(action);
                        }
                        return;
                    }
                    x += seg;
                }
            }
            return;
        }

        // Normal modes: iterate mode_hints
        let hints = status_bar::mode_hints(self.mode);
        for (i, (key, lbl)) in hints.iter().enumerate() {
            let seg = status_bar::hint_segment_width(key, lbl);
            if click_x >= x && click_x < x + seg {
                if let Some(action) = hint_click_action(self.mode, i) {
                    self.handle_action(action);
                }
                return;
            }
            x += seg;
        }

        // Right-aligned hints (e.g. Alt+n in Terminal mode)
        let right_hints = status_bar::mode_hints_right(self.mode);
        if !right_hints.is_empty() {
            let alt_prefix_width: u16 = 1 + 7 + 1; // sep + " Alt + " + sep
            let hints_width: u16 = right_hints
                .iter()
                .map(|(k, l)| status_bar::hint_segment_width(k, l))
                .sum();
            let total_right = hints_width + alt_prefix_width;
            let mut rx = area.x + area.width.saturating_sub(total_right);
            for (i, (key, lbl)) in right_hints.iter().enumerate() {
                let seg = status_bar::hint_segment_width(key, lbl);
                if click_x >= rx && click_x < rx + seg {
                    if let Some(action) = hint_click_action_right(self.mode, i) {
                        self.handle_action(action);
                    }
                    return;
                }
                rx += seg;
            }
        }
    }

    /// Returns the pane content area (terminal area minus the tab bar row).
    fn terminal_pane_area(&self) -> Rect {
        let r = self.panel_rects.terminal;
        if r.height > 1 {
            Rect::new(r.x, r.y + 1, r.width, r.height - 1)
        } else {
            Rect::new(r.x, r.y, r.width, 0)
        }
    }

    /// Find the pane whose rendered rect contains the given position.
    fn pane_at(&self, pos: Position) -> Option<(PaneId, Rect)> {
        let pane_area = self.terminal_pane_area();
        self.tabs
            .active_tree()?
            .compute_rects(pane_area)
            .into_iter()
            .find(|(_, rect)| rect.contains(pos))
    }

    /// Try to forward a mouse event to the focused pane's PTY.
    /// Returns true if the event was forwarded (child has mouse reporting enabled).
    fn try_forward_mouse(&mut self, mouse: &crossterm::event::MouseEvent) -> bool {
        let (pane_id, pane_rect) = match self.focused_pane_context(mouse) {
            Some(ctx) => ctx,
            None => return false,
        };
        let state = match self.pane_input_state(pane_id) {
            Some(state) => state,
            None => return false,
        };
        let handled = self.apply_input_route(
            pane_id,
            Some(pane_rect),
            route_mouse(
                *mouse,
                pane_rect,
                &state,
                self.pty_mouse_active,
                self.selection
                    .as_ref()
                    .is_some_and(|sel| sel.pane_id == pane_id),
            ),
        );
        handled
    }

    /// Handle mouse scroll within the terminal area.
    ///
    /// Workspace and explorer panels treat scroll as list navigation. Terminal
    /// panes preserve the existing scrollback / mouse-reporting behavior.
    fn handle_scroll(&mut self, x: u16, y: u16, up: bool) {
        let pos = Position::new(x, y);
        if self.panel_rects.workspace.contains(pos) {
            self.mode = Mode::Workspace;
            self.focus = FocusedPanel::Workspace;
            self.workspace_mode_entered = Some(std::time::Instant::now());
            self.navigate(if up { -1 } else { 1 });
            return;
        }

        if self.panel_rects.explorer.contains(pos) {
            self.mode = Mode::Explorer;
            self.focus = FocusedPanel::Explorer;
            if up {
                self.explorer_state.move_up();
            } else {
                self.explorer_state.move_down();
            }
            let viewport_height = self.panel_rects.explorer.height.saturating_sub(2) as usize;
            self.explorer_state.scroll_to_visible(viewport_height);
            return;
        }

        if !self.panel_rects.terminal.contains(pos) {
            return;
        }

        let (pane_id, pane_rect) = match self.pane_at(pos) {
            Some(v) => v,
            None => return,
        };

        let state = match self.pane_input_state(pane_id) {
            Some(state) => state,
            None => return,
        };
        let route = route_mouse(
            crossterm::event::MouseEvent {
                kind: if up {
                    MouseEventKind::ScrollUp
                } else {
                    MouseEventKind::ScrollDown
                },
                column: x,
                row: y,
                modifiers: KeyModifiers::NONE,
            },
            pane_rect,
            &state,
            self.pty_mouse_active,
            false,
        );
        self.apply_input_route(pane_id, Some(pane_rect), route);
        // Re-run search so highlights track the new viewport.
        if self.search_state.is_some() {
            self.run_search();
        }
    }

    fn select_workspace_tree_item(&mut self, item: &WorkspaceTreeItem) {
        match item.kind {
            TreeItemKind::Workspace => {
                self.workspace_selected = Some(item.workspace_id);
                self.room_selected = None;
            }
            TreeItemKind::Room => {
                self.ensure_tree_room_selected(item);
            }
        }
    }

    fn activate_selected_workspace_tree_item(&mut self) {
        let tree = self.workspace_tree_cache.clone();
        if self.selected_tree_index >= tree.len() {
            return;
        }

        let item = tree[self.selected_tree_index].clone();
        self.select_workspace_tree_item(&item);
        self.switch_to_selected_room();
        self.mode = Mode::Terminal;
        self.focus = FocusedPanel::Terminal;
        self.workspace_mode_entered = None;
    }

    /// Handle keyboard resize actions.
    fn handle_resize_action(&mut self, dir: NavDirection) {
        match self.focus {
            FocusedPanel::Terminal => {
                if let Some(pane_id) = self.focused_pane
                    && let Some(tree) = self.tabs.active_tree_mut()
                {
                    let sign: f64 = match dir {
                        NavDirection::Left | NavDirection::Up => -0.05,
                        NavDirection::Right | NavDirection::Down => 0.05,
                    };
                    tree.resize(pane_id, sign);
                }
            }
            FocusedPanel::Workspace => {
                let delta: i16 = match dir {
                    NavDirection::Right => 1,
                    NavDirection::Left => -1,
                    _ => 0,
                };
                self.panel_widths[0] = (self.panel_widths[0] as i16 + delta).clamp(5, 60) as u16;
            }
            FocusedPanel::Explorer => {
                let delta: i16 = match dir {
                    NavDirection::Right => 1,
                    NavDirection::Left => -1,
                    _ => 0,
                };
                self.panel_widths[1] = (self.panel_widths[1] as i16 + delta).clamp(5, 60) as u16;
            }
        }
    }

    /// Handle tab bar clicks: switch tabs or open new tab via "+".
    fn handle_tab_bar_click(&mut self, x: u16) {
        let tab_names = self
            .tabs
            .tab_names()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        let mut cursor = self.panel_rects.tab_bar.x;
        for (i, name) in tab_names.iter().enumerate() {
            // Tab text is " {name} " — 2 extra chars (leading + trailing space).
            let tab_width = (name.len() as u16) + 2;
            if x >= cursor && x < cursor + tab_width {
                self.tabs.set_active(i);
                self.sync_focused_pane();
                self.focus = FocusedPanel::Terminal;
                return;
            }
            cursor += tab_width;
        }
        // Clicked on "+" — open new tab.
        self.show_preset_selector(PresetAction::NewTab);
    }

    /// Build and display the preset selector popup.
    fn show_preset_selector(&mut self, action: PresetAction) {
        if self.state.active_room_id.is_none() {
            self.show_error("Select a room first");
            return;
        }
        let mut presets: Vec<String> = self.config.presets.keys().cloned().collect();
        presets.sort();
        if presets.is_empty() {
            // No presets configured — fall back to direct action with "shell".
            match action {
                PresetAction::NewTab => self.new_tab_with_preset("shell"),
                PresetAction::SplitDown => self.split_pane_with_preset("shell", false),
                PresetAction::SplitRight => self.split_pane_with_preset("shell", true),
            }
            self.mode = Mode::Terminal;
            return;
        }
        self.popup = PopupState::PresetSelector {
            presets,
            selected: 0,
            action,
        };
    }

    /// Show create room dialog (n in workspace mode).
    fn show_create_dialog(&mut self) {
        match self.focus {
            FocusedPanel::Workspace => {
                if self.state.active_workspace_id.is_none() {
                    self.show_error("No active workspace — create a workspace first (Shift+N)");
                    return;
                }
                let fields = vec![
                    DialogField::TextInput {
                        label: "Branch name".to_string(),
                        value: String::new(),
                    },
                    DialogField::TextInput {
                        label: "Base branch".to_string(),
                        value: String::new(),
                    },
                ];
                self.popup = PopupState::RoomCreate {
                    fields,
                    focused_field: 0,
                };
            }
            FocusedPanel::Terminal => {}
            FocusedPanel::Explorer => {}
        }
    }

    /// Show create workspace dialog (Shift+N in workspace mode).
    fn show_create_workspace_dialog(&mut self) {
        let fields = vec![
            DialogField::Select {
                label: "Mode".to_string(),
                options: vec![
                    "Clone".to_string(),
                    "Existing".to_string(),
                    "New".to_string(),
                ],
                selected: 0,
            },
            DialogField::TextInput {
                label: "Path".to_string(),
                value: String::new(),
            },
            DialogField::TextInput {
                label: "URL (Clone only)".to_string(),
                value: String::new(),
            },
        ];
        self.popup = PopupState::WorkspaceCreate {
            fields,
            focused_field: 0,
            completions: vec![],
            completion_selected: None,
        };
    }

    /// Show the appropriate delete dialog based on focused panel and tree selection.
    fn show_delete_dialog(&mut self) {
        match self.focus {
            FocusedPanel::Workspace => {
                let tree = self.workspace_tree_cache.clone();
                if self.selected_tree_index >= tree.len() {
                    return;
                }
                let item = &tree[self.selected_tree_index];
                match &item.kind {
                    TreeItemKind::Workspace => {
                        let fields = vec![
                            DialogField::Confirm {
                                message: format!("Delete workspace '{}'?", item.name),
                                yes: false,
                            },
                            DialogField::Checkbox {
                                label: "Also delete directory from disk".to_string(),
                                checked: false,
                            },
                        ];
                        self.popup = PopupState::WorkspaceDelete {
                            fields,
                            focused_field: 0,
                            workspace_id: item.workspace_id,
                        };
                    }
                    TreeItemKind::Room => {
                        let branch = match self
                            .state
                            .ws_by_id(item.workspace_id)
                            .and_then(|ws| item.room_id.and_then(|rid| ws.room_by_id(rid)))
                            .map(|r| r.name.clone())
                        {
                            Some(b) => b,
                            None => return,
                        };
                        let fields = vec![DialogField::Confirm {
                            message: "Delete room? This removes worktree and branch.".to_string(),
                            yes: false,
                        }];
                        self.popup = PopupState::RoomDelete {
                            fields,
                            focused_field: 0,
                            workspace_id: item.workspace_id,
                            branch,
                        };
                    }
                }
            }
            FocusedPanel::Terminal => {}
            FocusedPanel::Explorer => {}
        }
    }

    /// Compute the working directory for the currently active workspace/room.
    ///
    /// Returns `None` when no workspace or room is active.  The default room
    /// (the workspace repo itself) maps to the workspace path; worktree rooms
    /// map to their actual git worktree path (looked up from cache).
    fn current_room_path(&self) -> Option<PathBuf> {
        let ws_id = self.state.active_workspace_id?;
        let room_id = self.state.active_room_id?;
        let ws = self.state.ws_by_id(ws_id)?;
        // Look up the actual worktree path from the cache.
        if let Some(cached) = self.room_cache.get(&ws_id) {
            if let Some(entry) = cached.iter().find(|r| r.room_id == Some(room_id)) {
                return Some(entry.path.clone());
            }
        }

        // Fallback: try humu-managed worktree path, then workspace root.
        let room = ws.room_by_id(room_id)?;
        let worktree_path = humu_dir()
            .join("worktrees")
            .join(ws_id.to_string())
            .join(&room.name);

        if worktree_path.exists() {
            Some(worktree_path)
        } else {
            Some(ws.path.clone())
        }
    }

    /// Spawn a new pane from the named preset and register it.
    /// Returns the new `PaneId` on success.
    fn spawn_pane(&mut self, preset_name: &str, session_id: Option<String>) -> Option<PaneId> {
        // Preserve session_id for agent_states before it's consumed by args.
        let restored_session_id = session_id.clone();

        if self.attached_snapshot.is_some() {
            let id = self.create_attached_placeholder_pane(preset_name, restored_session_id.clone());
            let cwd = self.current_room_path();
            self.register_pane_with_daemon(
                id,
                preset_name,
                cwd,
                restored_session_id,
                SystemTime::now(),
            );
            self.refresh_attached_runtime_snapshot();
            return Some(id);
        }

        let shell_cmd = self
            .config
            .presets
            .get(preset_name)
            .map(|p| p.command.as_str())
            .unwrap_or("sh")
            .to_string();
        let shell_args: Vec<String> = self
            .config
            .presets
            .get(preset_name)
            .map(|p| p.args.clone())
            .unwrap_or_default();
        let arg_refs: Vec<&str> = shell_args.iter().map(String::as_str).collect();
        let (cmd, args) = humu::preset::resolve_preset(&shell_cmd, &arg_refs);

        // Set HUMU_* env vars when spawning the "claude" or "gemini" preset.
        let mut extra_args: Vec<String> = vec![];
        let id = PaneId::new();
        let mut envs: Vec<(String, String)> = vec![];

        if preset_name == PRESET_CLAUDE {
            let settings_path = humu_dir().join("hooks/claude-settings.json");
            extra_args.push("--settings".to_string());
            extra_args.push(settings_path.to_string_lossy().into_owned());

            if let Some(sid) = session_id {
                extra_args.push("--resume".to_string());
                extra_args.push(sid);
            }
        } else if preset_name == PRESET_GEMINI {
            let settings_path = humu_dir().join("hooks/gemini-settings.json");
            // Gemini CLI uses env var for custom settings path
            envs.push((
                "GEMINI_CLI_SYSTEM_SETTINGS_PATH".to_string(),
                settings_path.to_string_lossy().into_owned(),
            ));

            if let Some(sid) = session_id {
                extra_args.push("--resume".to_string());
                extra_args.push(sid);
            }
        } else if preset_name == PRESET_CODEX {
            append_codex_args(&mut extra_args, session_id);
        }

        if preset_name == PRESET_CLAUDE || preset_name == PRESET_GEMINI {
            if let Some(port) = self.hook_port {
                envs.push(("HUMU_PORT".to_string(), port.to_string()));
            }
            if let Some(ws_id) = self.state.active_workspace_id {
                envs.push(("HUMU_WORKSPACE_ID".to_string(), ws_id.to_string()));
            }
            if let Some(room_id) = self.state.active_room_id {
                envs.push(("HUMU_ROOM_ID".to_string(), room_id.to_string()));
            }
            envs.push(("HUMU_TAB_ID".to_string(), TabId::new().to_string()));
            envs.push(("HUMU_PANE_ID".to_string(), id.to_string()));
        }

        let cwd = self.current_room_path();
        let mut all_args = args;
        all_args.extend(extra_args);
        let pane = PtyPane::spawn_with_envs(&cmd, &all_args, cwd.as_deref(), 80, 24, &envs).ok()?;
        self.local_panes.insert(id, pane);
        self.pane_presets.insert(id, preset_name.to_string());
        // Seed agent_states so session_id survives restart even if no hook
        // event arrives before the next shutdown.
        if restored_session_id.is_some() {
            self.agent_states.insert(
                id,
                AgentStateEntry {
                    state: AgentState::Idle,
                    session_id: restored_session_id.clone(),
                },
            );
        }
        self.register_pane_with_daemon(
            id,
            preset_name,
            cwd,
            restored_session_id,
            SystemTime::now(),
        );
        Some(id)
    }

    fn new_tab_with_preset(&mut self, preset_name: &str) {
        if let Some(new_id) = self.spawn_pane(preset_name, None) {
            self.tabs
                .add_tab(preset_name.to_string(), SplitTree::leaf(new_id));
            let last = self.tabs.len() - 1;
            self.tabs.set_active(last);
            self.focused_pane = Some(new_id);
            self.persist_layout();
        }
    }

    fn cleanup_exited_panes(&mut self) {
        // Don't clean up floating pane — it has its own auto-close logic.
        let floating_id = if let PopupState::FloatingPane { pane_id, .. } = &self.popup {
            Some(*pane_id)
        } else {
            None
        };
        let exited: Vec<PaneId> = self
            .local_panes
            .iter_mut()
            .filter_map(|(id, p)| p.exit_status().map(|_| *id))
            .filter(|id| Some(*id) != floating_id)
            .collect();
        if !exited.is_empty() {
            self.remove_panes(&exited);
        }
    }

    fn close_tab(&mut self) {
        let ids: Vec<PaneId> = self
            .tabs
            .active_tree()
            .map(|t| t.pane_ids())
            .unwrap_or_default();
        if !ids.is_empty() {
            self.remove_panes(&ids);
        }
    }

    /// After changing the active tab, set `focused_pane` to the first pane in that tab.
    fn sync_focused_pane(&mut self) {
        self.focused_pane = self
            .tabs
            .active_tree()
            .and_then(|t| t.pane_ids().into_iter().next());
    }

    fn split_pane_with_preset(&mut self, preset_name: &str, horizontal: bool) {
        let focused = match self.focused_pane {
            Some(id) => id,
            None => return,
        };
        let new_id = match self.spawn_pane(preset_name, None) {
            Some(id) => id,
            None => return,
        };
        if let Some(tree) = self.tabs.active_tree_mut() {
            if horizontal {
                tree.split_horizontal(focused, new_id);
            } else {
                tree.split_vertical(focused, new_id);
            }
            self.focused_pane = Some(new_id);
            self.persist_layout();
        } else {
            // No active tree — clean up the pane we just spawned.
            self.remove_pane_runtime_state(new_id);
        }
    }

    fn toggle_fullscreen(&mut self) {
        if self.fullscreen_pane.is_some() {
            // Turn off fullscreen — restore normal split rendering.
            self.fullscreen_pane = None;
        } else {
            // Enter fullscreen with the currently focused pane.
            self.fullscreen_pane = self.focused_pane;
        }
    }

    fn close_pane(&mut self) {
        if let Some(id) = self.focused_pane {
            self.remove_panes(&[id]);
        }
    }

    /// Remove panes by ID: clean up state, update split trees, remove empty tabs.
    fn remove_panes(&mut self, ids: &[PaneId]) {
        for id in ids {
            self.remove_pane_runtime_state(*id);
        }
        // Remove dead panes from trees and remove empty tabs.
        let mut i = self.tabs.len();
        while i > 0 {
            i -= 1;
            if let Some(tree) = self.tabs.tree_at_mut(i) {
                for id in ids {
                    tree.remove_pane(*id);
                }
                let alive = tree
                    .pane_ids()
                    .iter()
                    .any(|id| self.pane_presets.contains_key(id));
                if !alive {
                    self.tabs.remove_tab(i);
                }
            }
        }
        self.fullscreen_pane = None;
        self.sync_focused_pane();
        self.persist_layout();
    }

    fn move_focus(&mut self, dir: NavDirection) {
        let focused = match self.focused_pane {
            Some(id) => id,
            None => return,
        };

        // We need a temporary area to compute rects; use a fixed reference area.
        // The actual area is not known here, so we use a large fixed rect so
        // relative adjacency is still computed correctly.
        let area = Rect::new(0, 0, 1000, 1000);

        let rects = match self.tabs.active_tree() {
            Some(tree) => tree.compute_rects(area),
            None => return,
        };

        let focused_rect = match rects.iter().find(|(id, _)| *id == focused) {
            Some((_, r)) => *r,
            None => return,
        };

        let candidate = match dir {
            NavDirection::Left => rects
                .iter()
                .filter(|(id, r)| {
                    *id != focused
                        && r.x + r.width <= focused_rect.x
                        && ranges_overlap(
                            r.y,
                            r.y + r.height,
                            focused_rect.y,
                            focused_rect.y + focused_rect.height,
                        )
                })
                .max_by_key(|(_, r)| r.x + r.width),

            NavDirection::Right => rects
                .iter()
                .filter(|(id, r)| {
                    *id != focused
                        && r.x >= focused_rect.x + focused_rect.width
                        && ranges_overlap(
                            r.y,
                            r.y + r.height,
                            focused_rect.y,
                            focused_rect.y + focused_rect.height,
                        )
                })
                .min_by_key(|(_, r)| r.x),

            NavDirection::Up => rects
                .iter()
                .filter(|(id, r)| {
                    *id != focused
                        && r.y + r.height <= focused_rect.y
                        && ranges_overlap(
                            r.x,
                            r.x + r.width,
                            focused_rect.x,
                            focused_rect.x + focused_rect.width,
                        )
                })
                .max_by_key(|(_, r)| r.y + r.height),

            NavDirection::Down => rects
                .iter()
                .filter(|(id, r)| {
                    *id != focused
                        && r.y >= focused_rect.y + focused_rect.height
                        && ranges_overlap(
                            r.x,
                            r.x + r.width,
                            focused_rect.x,
                            focused_rect.x + focused_rect.width,
                        )
                })
                .min_by_key(|(_, r)| r.y),
        };

        if let Some((new_id, _)) = candidate {
            self.focused_pane = Some(*new_id);
            self.focus = FocusedPanel::Terminal;
        }
    }

    fn handle_passthrough(&mut self, key: KeyEvent) {
        if self.focus != FocusedPanel::Terminal {
            return;
        }
        let Some(pane_id) = self.focused_pane else {
            return;
        };

        // If focused pane has exited, intercept p/t to close pane/tab.
        let exited = self.pane_has_exited(pane_id);
        if exited {
            return;
        }

        let Some(state) = self.pane_input_state(pane_id) else {
            return;
        };
        let route = route_passthrough(key, &state);
        self.apply_input_route(pane_id, None, route);
    }

    /// Route paste events: popups get priority, otherwise forward to PTY.
    fn handle_paste_event(&mut self, text: &str) {
        if self.paste_into_focused_dialog_field(text) {
            return;
        }
        if let PopupState::NotificationTokenInput { field, value } = &self.popup {
            let field = *field;
            let mut value = value.clone();
            value.push_str(text);
            self.popup = PopupState::NotificationTokenInput { field, value };
            return;
        }
        if let PopupState::ExplorerNewEntry { is_dir, value } = &self.popup {
            let is_dir = *is_dir;
            let mut value = value.clone();
            value.push_str(text);
            self.popup = PopupState::ExplorerNewEntry { is_dir, value };
            return;
        }
        if let PopupState::FloatingPane { pane_id, .. } = &self.popup {
            let pane_id = *pane_id;
            if let Some(pane) = self.local_panes.get_mut(&pane_id) {
                let _ = pane.write_input(text.as_bytes());
            }
            return;
        }
        self.handle_paste(text);
    }

    /// Forward pasted text to the focused PTY pane.
    /// Wraps in bracketed paste sequences if the child process has requested it.
    /// In EnterSearch mode, appends the text to the search query instead.
    fn handle_paste(&mut self, text: &str) {
        if self.mode == Mode::EnterSearch {
            if let Some(ref mut state) = self.search_state {
                state.query.push_str(text);
                self.run_search();
            }
            return;
        }
        if self.focus != FocusedPanel::Terminal {
            return;
        }
        let Some(pane_id) = self.focused_pane else {
            return;
        };
        let Some(state) = self.pane_input_state(pane_id) else {
            return;
        };
        if self.pane_has_exited(pane_id) {
            return;
        }
        let route = route_paste(text, &state);
        self.apply_input_route(pane_id, None, route);
    }

    fn navigate(&mut self, delta: i32) {
        match self.focus {
            FocusedPanel::Workspace => {
                let tree = self.workspace_tree_cache.clone();
                if tree.is_empty() {
                    return;
                }
                let current = self.selected_tree_index as i32;
                let next = (current + delta).clamp(0, tree.len() as i32 - 1) as usize;
                self.selected_tree_index = next;
            }
            FocusedPanel::Terminal => {}
            FocusedPanel::Explorer => {
                if delta < 0 {
                    self.explorer_state.move_up();
                } else {
                    self.explorer_state.move_down();
                }
            }
        }
    }

    fn select_current(&mut self) {
        match self.focus {
            FocusedPanel::Workspace => {
                self.activate_selected_workspace_tree_item();
            }
            FocusedPanel::Terminal => {}
            FocusedPanel::Explorer => {
                self.explorer_select();
            }
        }
    }

    fn restore_selection(&mut self) {
        let ws_info: Vec<(WorkspaceId, PathBuf)> = self
            .state
            .workspaces
            .iter()
            .map(|w| (w.id, w.path.clone()))
            .collect();
        for (ws_id, ws_path) in ws_info {
            self.sync_workspace_rooms(ws_id, &ws_path);
        }

        self.workspace_selected = self.state.active_workspace_id;
        self.room_selected = self.state.active_room_id;

        // Restore layout if saved
        if let (Some(_ws_id), Some(room_id)) =
            (self.state.active_workspace_id, self.state.active_room_id)
        {
            if let Some(layout) = self.room_layout(room_id) {
                self.restore_layout(layout.active_tab, layout.tabs);
            }
        }
    }

    /// Collect pane IDs belonging to a workspace (live + suspended).
    fn pane_ids_for_workspace(&self, ws_id: WorkspaceId) -> Vec<PaneId> {
        let mut ids = Vec::new();
        // Current room's panes if this is the active workspace.
        if self.state.active_workspace_id == Some(ws_id) {
            ids.extend(self.pane_presets.keys());
        }
        // Suspended rooms for this workspace.
        for ((wid, _), room_state) in &self.suspended_rooms {
            if *wid == ws_id {
                ids.extend(room_state.pane_presets.keys());
            }
        }
        ids
    }

    /// Check if any pane in the given set has an active agent state.
    fn has_active_agent(&self, pane_ids: &[PaneId]) -> bool {
        pane_ids.iter().any(|id| {
            self.agent_states
                .get(id)
                .is_some_and(|e| matches!(e.state, AgentState::Working | AgentState::NeedsInput))
        })
    }

    /// Build a flattened workspace tree for rendering. Workspaces are sorted
    /// alphabetically. Expanded workspaces show their rooms as children. The
    /// active workspace is always expanded.
    /// Rebuild the cached workspace tree from current state.
    fn rebuild_workspace_tree(&mut self) {
        let ws_list: Vec<_> = self.state.workspaces.iter().collect();

        // Compute display names: show at least 2 trailing path components,
        // expanding further when duplicates remain.
        let display_names = Self::compute_display_names(&ws_list);

        // Zip and sort by display name.
        let mut entries: Vec<_> = display_names.into_iter().zip(ws_list).collect();
        entries.sort_by(|(a, _), (b, _)| a.cmp(b));

        let mut items = Vec::new();

        for (display_name, ws) in &entries {
            let ws_active = self.has_active_agent(&self.pane_ids_for_workspace(ws.id));

            items.push(WorkspaceTreeItem {
                kind: TreeItemKind::Workspace,
                name: display_name.clone(),
                active: ws_active,
                workspace_id: ws.id,
                room_id: None,
                room_path: None,
                git_status: RoomGitStatus::default(),
            });

            let room_items = self.room_items_for_workspace(ws.id);
            for r in room_items {
                items.push(WorkspaceTreeItem {
                    kind: TreeItemKind::Room,
                    name: r.name.clone(),
                    active: r.active,
                    workspace_id: ws.id,
                    room_id: r.id,
                    room_path: Some(r.path.clone()),
                    git_status: r.git_status,
                });
            }
        }

        self.workspace_tree_cache = items;
    }

    /// Build display names for workspaces using at least 2 trailing path
    /// components (e.g. `hhk7734/humu`). When duplicates remain, expand with
    /// more components until each name is unique (or the full path is used).
    fn compute_display_names(ws_list: &[&WorkspaceEntry]) -> Vec<String> {
        let min_depth = 2usize;

        // Helper: take the last `n` components of a path and join with '/'.
        let trailing = |path: &std::path::Path, n: usize| -> String {
            let total = path.iter().count();
            path.iter()
                .skip(total.saturating_sub(n))
                .map(|c| c.to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        };

        // Start with min_depth components for every workspace.
        let mut names: Vec<String> = ws_list
            .iter()
            .map(|ws| trailing(&ws.path, min_depth))
            .collect();

        // Iteratively expand duplicates until unique or path exhausted.
        let max_components = ws_list
            .iter()
            .map(|ws| ws.path.iter().count())
            .max()
            .unwrap_or(0);

        for depth in (min_depth + 1)..=max_components {
            let mut count: HashMap<&str, usize> = HashMap::new();
            for name in &names {
                *count.entry(name.as_str()).or_insert(0) += 1;
            }
            let dups: Vec<usize> = (0..names.len())
                .filter(|i| count[names[*i].as_str()] > 1)
                .collect();
            if dups.is_empty() {
                break;
            }
            for i in dups {
                names[i] = trailing(&ws_list[i].path, depth);
            }
        }

        names
    }

    /// Map a visual row in the workspace panel to a tree item index.
    /// Rooms always take 2 rows (name + git stats line).
    fn visual_row_to_tree_index(tree: &[WorkspaceTreeItem], visual_row: usize) -> Option<usize> {
        let mut y = 0usize;
        for (i, item) in tree.iter().enumerate() {
            if y == visual_row {
                return Some(i);
            }
            y += 1;
            // Rooms always have a git stats line (2nd row)
            if matches!(item.kind, TreeItemKind::Room) {
                if y == visual_row {
                    return Some(i); // clicked on stats line → same item
                }
                y += 1;
            }
        }
        None
    }

    /// Collect pane IDs belonging to a specific room (live + suspended).
    fn pane_ids_for_room(&self, ws_id: WorkspaceId, room_id: RoomId) -> Vec<PaneId> {
        let mut ids = Vec::new();
        // Current room's panes if this is the active workspace+room.
        if self.state.active_workspace_id == Some(ws_id)
            && self.state.active_room_id == Some(room_id)
        {
            ids.extend(self.pane_presets.keys());
        }
        // Suspended room.
        if let Some(room_state) = self.suspended_rooms.get(&(ws_id, room_id)) {
            ids.extend(room_state.pane_presets.keys());
        }
        ids
    }

    /// Refresh cached room list + git stats for all workspaces.
    fn refresh_room_cache(&mut self) {
        let mgr = RoomManager::new();
        let ws_ids: Vec<WorkspaceId> = self.state.workspaces.iter().map(|w| w.id).collect();
        for ws_id in ws_ids {
            let ws_path = match self.state.ws_by_id(ws_id) {
                Some(ws) => ws.path.clone(),
                None => continue,
            };

            self.sync_workspace_rooms(ws_id, &ws_path);

            let ws = match self.state.ws_by_id(ws_id) {
                Some(ws) => ws,
                None => continue,
            };
            if let Ok(rooms) = mgr.list(&ws_path) {
                let cached: Vec<CachedRoomInfo> = rooms
                    .into_iter()
                    .map(|r| {
                        let existing = ws.room_by_path(&r.path);
                        let room_id = existing.map(|e| e.id);
                        let name = existing.map(|e| e.name.clone()).unwrap_or_else(|| {
                            if r.is_default {
                                DEFAULT_ROOM_NAME.to_string()
                            } else {
                                r.branch
                            }
                        });
                        let git_status = mgr.status(&r.path);
                        CachedRoomInfo {
                            room_id,
                            name,
                            path: r.path,
                            git_status,
                        }
                    })
                    .collect();
                self.room_cache.insert(ws_id, cached);
            }
        }
    }

    fn sync_workspace_rooms(&mut self, ws_id: WorkspaceId, ws_path: &std::path::Path) {
        let mgr = RoomManager::new();
        let Ok(rooms) = mgr.list(ws_path) else {
            return;
        };

        let discovered: std::collections::HashSet<PathBuf> =
            rooms.iter().map(|r| r.path.clone()).collect();
        humu::config::prune_stale_rooms_for_workspace(&mut self.state, ws_id, &discovered);

        for room in rooms {
            let exists = self
                .state
                .ws_by_id(ws_id)
                .and_then(|ws| ws.room_by_path(&room.path))
                .is_some();
            if !exists {
                let room_name = if room.is_default {
                    DEFAULT_ROOM_NAME
                } else {
                    room.branch.as_str()
                };
                let _ = humu::config::create_room_for_workspace(
                    &mut self.state,
                    ws_id,
                    room_name,
                    room.path,
                );
            }
        }
    }

    fn ensure_tree_room_selected(&mut self, item: &WorkspaceTreeItem) {
        self.workspace_selected = Some(item.workspace_id);
        self.room_selected = item.room_id;

        if self.room_selected.is_none() {
            if let Some(path) = &item.room_path {
                let room_name = if item.name.is_empty() {
                    DEFAULT_ROOM_NAME
                } else {
                    item.name.as_str()
                };
                self.room_selected = humu::config::create_room_for_workspace(
                    &mut self.state,
                    item.workspace_id,
                    room_name,
                    path.clone(),
                );
            }
        }
    }

    fn ensure_local_room(&mut self, ws_id: WorkspaceId) -> Option<RoomId> {
        let ws_path = self.state.ws_by_id(ws_id)?.path.clone();
        if let Some(existing) = self
            .state
            .ws_by_id(ws_id)
            .and_then(|ws| ws.room_by_path(&ws_path).map(|room| room.id))
        {
            return Some(existing);
        }

        humu::config::create_room_for_workspace(&mut self.state, ws_id, DEFAULT_ROOM_NAME, ws_path)
    }

    fn drop_room_runtime_state(&mut self, ws_id: WorkspaceId, room_id: RoomId) {
        if self.state.active_workspace_id == Some(ws_id)
            && self.state.active_room_id == Some(room_id)
        {
            let pane_ids: Vec<PaneId> = self.pane_presets.keys().copied().collect();
            for pane_id in pane_ids {
                self.agent_states.remove(&pane_id);
                self.unregister_pane_with_daemon(pane_id);
            }
            self.pane_presets.clear();
            self.tabs = TabContainer::new();
            self.focused_pane = None;
            self.fullscreen_pane = None;
            self.search_state = None;
        }

        if let Some(room_state) = self.suspended_rooms.remove(&(ws_id, room_id)) {
            self.unregister_room_state_panes(&room_state);
        }
    }

    fn remove_room_from_state(&mut self, ws_id: WorkspaceId, room_id: RoomId) {
        if let Some(ws) = self.state.ws_by_id_mut(ws_id) {
            ws.rooms.retain(|room| room.id != room_id);
            if ws.last_room_id == Some(room_id) {
                ws.last_room_id = None;
            }
        }

        self.state.remove_room_session_state(ws_id, room_id);
    }

    /// List rooms for a specific workspace by ID, with agent activity flags.
    /// Uses the cached room list -- no git subprocesses.
    fn room_items_for_workspace(&self, ws_id: WorkspaceId) -> Vec<CachedRoomItem> {
        let empty = Vec::new();
        let cache = self.room_cache.get(&ws_id).unwrap_or(&empty);
        cache
            .iter()
            .map(|r| {
                let active = r
                    .room_id
                    .map(|rid| self.has_active_agent(&self.pane_ids_for_room(ws_id, rid)))
                    .unwrap_or(false);
                CachedRoomItem {
                    id: r.room_id,
                    name: r.name.clone(),
                    path: r.path.clone(),
                    active,
                    git_status: r.git_status,
                }
            })
            .collect()
    }

    /// Convert the current runtime TabContainer into layout data for persistence.
    /// Returns `None` if there are no tabs.
    fn save_layout(&self) -> Option<(usize, Vec<TabLayout>)> {
        if self.tabs.is_empty() {
            return None;
        }
        let tabs: Vec<TabLayout> = self
            .tabs
            .tab_names()
            .into_iter()
            .enumerate()
            .filter_map(|(i, name)| {
                let tree = self.tabs.tree_at(i)?;
                let split = self.split_tree_to_node(tree)?;
                Some(TabLayout {
                    name: name.to_string(),
                    split,
                })
            })
            .collect();

        if tabs.is_empty() {
            return None;
        }

        Some((self.tabs.active_index(), tabs))
    }

    fn room_layout(&self, room_id: RoomId) -> Option<PersistedRoomLayout> {
        self.state
            .session_by_name(HumuState::DEFAULT_SESSION_NAME)
            .and_then(|session| session.tabs_by_room.get(&room_id))
            .cloned()
    }

    fn persist_room_layout(
        &mut self,
        ws_id: WorkspaceId,
        room_id: RoomId,
        layout: Option<(usize, Vec<TabLayout>)>,
    ) {
        {
            let session = self.state.ensure_session(HumuState::DEFAULT_SESSION_NAME);
            match layout {
                Some((active_tab, tabs)) => {
                    session
                        .tabs_by_room
                        .insert(room_id, PersistedRoomLayout { active_tab, tabs });
                }
                None => {
                    session.tabs_by_room.remove(&room_id);
                }
            }
        }

        if let Some(ws_entry) = self.state.ws_by_id_mut(ws_id)
            && let Some(room_entry) = ws_entry.room_by_id_mut(room_id)
        {
            room_entry.active_tab = None;
            room_entry.tabs.clear();
        }
    }

    /// Recursively convert a runtime `SplitTree` to the serializable `SplitNode`.
    fn split_tree_to_node(&self, tree: &SplitTree) -> Option<SplitNode> {
        match tree {
            SplitTree::Leaf(pane_id) => {
                let preset = self.pane_presets.get(pane_id)?.clone();
                let session_id = self
                    .agent_states
                    .get(pane_id)
                    .and_then(|e| e.session_id.clone());
                Some(SplitNode::Leaf { preset, session_id })
            }
            SplitTree::Split {
                direction,
                ratio,
                children,
            } => {
                let left = self.split_tree_to_node(&children.0)?;
                let right = self.split_tree_to_node(&children.1)?;
                let dir = match direction {
                    SplitDirection::Vertical => CfgDir::Vertical,
                    SplitDirection::Horizontal => CfgDir::Horizontal,
                };
                Some(SplitNode::Split {
                    direction: dir,
                    ratio: *ratio,
                    children: vec![left, right],
                })
            }
        }
    }

    /// Sync the current layout for the active workspace/room into `self.state`
    /// (memory only, no disk write).
    fn sync_layout(&mut self) {
        let ws_id = match self.state.active_workspace_id {
            Some(id) => id,
            None => return,
        };
        let room_id = match self.state.active_room_id {
            Some(id) => id,
            None => return,
        };
        let layout = self.save_layout();
        self.persist_room_layout(ws_id, room_id, layout);
    }

    /// Sync layout to state and flush to disk.
    fn persist_layout(&mut self) {
        self.sync_layout();
        self.save_state();
    }

    /// Flush `self.state` to disk (`~/.humu/state.yaml`).
    /// Also syncs `panel_widths` so it's never stale on disk.
    fn save_state(&mut self) {
        self.state.panel_widths = Some(self.panel_widths);
        if let Err(e) = self.state.save(&self.state_path) {
            humu::humu_log!("failed to save state: {e}");
        }
    }

    /// Close all existing panes and rebuild the TabContainer from a saved room's layout.
    fn restore_layout(&mut self, active_tab: usize, tabs: Vec<TabLayout>) {
        // Drop all existing panes.
        self.local_panes.clear();
        self.pane_presets.clear();
        self.tabs = TabContainer::new();
        self.focused_pane = None;
        for tab_layout in &tabs {
            match self.node_to_split_tree(&tab_layout.split) {
                Some(tree) => {
                    self.tabs.add_tab(tab_layout.name.clone(), tree);
                }
                None => {
                    // Fallback: spawn a plain shell tab if restore fails for this tab.
                    if let Some(id) = self.spawn_pane("shell", None) {
                        self.tabs
                            .add_tab(tab_layout.name.clone(), SplitTree::leaf(id));
                    }
                }
            }
        }

        // If nothing was restored, create a default shell tab.
        if self.tabs.is_empty()
            && let Some(id) = self.spawn_pane("shell", None)
        {
            self.tabs.add_tab("shell".into(), SplitTree::leaf(id));
        }

        // Restore active tab.
        let last = self.tabs.len().saturating_sub(1);
        self.tabs.set_active(active_tab.min(last));
        self.sync_focused_pane();
    }

    /// Recursively convert a `SplitNode` (config) into a runtime `SplitTree`,
    /// spawning PTY panes as needed.
    fn node_to_split_tree(&mut self, node: &SplitNode) -> Option<SplitTree> {
        match node {
            SplitNode::Leaf { preset, session_id } => {
                let id = if self.attached_snapshot.is_some() {
                    self.create_attached_placeholder_pane(preset, session_id.clone())
                } else {
                    self.spawn_pane(preset, session_id.clone())?
                };
                Some(SplitTree::Leaf(id))
            }
            SplitNode::Split {
                direction,
                ratio,
                children,
            } => {
                if children.len() < 2 {
                    return None;
                }
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

    // ── Search helpers ─────────────────────────────────────────────────────────

    fn run_search(&mut self) {
        let pane_id = match self.focused_pane {
            Some(id) => id,
            None => return,
        };
        let rows = match self.pane_search_rows(pane_id) {
            Some(rows) => rows,
            None => return,
        };
        if let Some(ref mut state) = self.search_state {
            state.execute(&rows);
            self.scroll_to_active_match();
        }
    }

    fn scroll_to_active_match(&mut self) {
        // Search covers the current viewport, so matches are already visible.
        // No scrollback adjustment needed.
    }

    /// Suspend the current room's live state (panes, tabs, etc.) into the
    /// `suspended_rooms` map without killing PTY processes.
    fn suspend_current_room(&mut self) {
        let ws_id = match self.state.active_workspace_id {
            Some(id) => id,
            None => return,
        };
        let room_id = match self.state.active_room_id {
            Some(id) => id,
            None => return,
        };

        // Clear search state — suspended panes may receive new output.
        self.search_state = None;

        // Sync layout into state so the room can be cold-restored.
        self.sync_layout();

        // Move live state out of self into suspended storage.
        // PaneId uses UUID, so uniqueness is guaranteed across rooms.
        let room_state = RoomState {
            local_panes: if self.attached_snapshot.is_some() {
                HashMap::new()
            } else {
                std::mem::take(&mut self.local_panes)
            },
            tabs: std::mem::replace(&mut self.tabs, TabContainer::new()),
            pane_presets: std::mem::take(&mut self.pane_presets),
            focused_pane: self.focused_pane.take(),
            fullscreen_pane: self.fullscreen_pane.take(),
        };

        self.suspended_rooms.insert((ws_id, room_id), room_state);
    }

    /// Restore a suspended room's live state, or fall back to cold restore from
    /// the persisted layout, or create a default shell tab.
    fn restore_room(&mut self, ws_id: WorkspaceId, room_id: RoomId) {
        if let Some(room_state) = self.suspended_rooms.remove(&(ws_id, room_id)) {
            if self.attached_snapshot.is_some() {
                // Hot restore for attached rooms only needs the layout metadata;
                // the daemon continues to own the underlying session panes.
            } else {
                self.local_panes = room_state.local_panes;
                for pane in self.local_panes.values_mut() {
                    let _ = pane.process_output();
                }
            }
            self.tabs = room_state.tabs;
            self.pane_presets = room_state.pane_presets;
            self.focused_pane = room_state.focused_pane;
            self.fullscreen_pane = room_state.fullscreen_pane;
        } else {
            // Cold restore from persisted layout, or create default.
            if let Some(layout) = self.room_layout(room_id) {
                self.restore_layout(layout.active_tab, layout.tabs);
            } else {
                // No saved layout — create a default shell tab.
                self.pane_presets.clear();
                self.tabs = TabContainer::new();
                self.focused_pane = None;
                self.fullscreen_pane = None;
                if let Some(id) = self.spawn_pane("shell", None) {
                    self.tabs.add_tab("shell".into(), SplitTree::leaf(id));
                    self.focused_pane = Some(id);
                }
            }
        }
    }

    /// Compute the floating pane overlay rect (centered on terminal panel, 90% size).
    fn floating_pane_area(&self) -> Rect {
        let base = self.panel_rects.terminal;
        let width = (base.width * 9 / 10).max(20);
        let height = (base.height * 9 / 10).max(10);
        let x = base.x + (base.width.saturating_sub(width)) / 2;
        let y = base.y + (base.height.saturating_sub(height)) / 2;
        Rect::new(x, y, width, height)
    }

    fn app_area(&self) -> Rect {
        let right = self.panel_rects.explorer.x + self.panel_rects.explorer.width;
        let bottom = self.panel_rects.status_bar.y + self.panel_rects.status_bar.height;
        Rect::new(0, 0, right, bottom)
    }

    #[cfg(test)]
    fn dialog_popup_area(&self, field_count: usize) -> Rect {
        Self::dialog_popup_area_for(self.app_area(), field_count)
    }

    fn dialog_popup_area_for(area: Rect, field_count: usize) -> Rect {
        let field_rows = field_count as u16 * 2;
        let height = (field_rows + 3).min(area.height);
        let width = 60u16.min(area.width);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect::new(x, y, width, height)
    }

    fn dialog_field_at(popup: Rect, fields: &[DialogField], x: u16, y: u16) -> Option<usize> {
        if !popup.contains(Position::new(x, y)) {
            return None;
        }

        let inner = Rect::new(
            popup.x.saturating_add(1),
            popup.y.saturating_add(1),
            popup.width.saturating_sub(2),
            popup.height.saturating_sub(2),
        );
        if !inner.contains(Position::new(x, y)) {
            return None;
        }

        let mut row = inner.y;
        for (idx, field) in fields.iter().enumerate() {
            let field_height = match field {
                DialogField::Checkbox { .. } => 1,
                _ => 2,
            };
            let end = row.saturating_add(field_height);
            if y >= row && y < end {
                return Some(idx);
            }
            row = end;
            if row >= inner.y + inner.height {
                break;
            }
        }

        None
    }

    fn handle_dialog_mouse(&mut self, x: u16, y: u16) -> bool {
        let area = self.app_area();
        match &mut self.popup {
            PopupState::WorkspaceCreate {
                fields,
                focused_field,
                ..
            }
            | PopupState::RoomCreate {
                fields,
                focused_field,
                ..
            }
            | PopupState::WorkspaceDelete {
                fields,
                focused_field,
                ..
            }
            | PopupState::RoomDelete {
                fields,
                focused_field,
                ..
            } => {
                let popup = Self::dialog_popup_area_for(area, fields.len());
                if let Some(idx) = Self::dialog_field_at(popup, fields, x, y) {
                    *focused_field = idx;
                }
                true
            }
            _ => false,
        }
    }

    fn paste_into_focused_dialog_field(&mut self, text: &str) -> bool {
        let mut refresh_workspace_completions = false;

        let pasted = match &mut self.popup {
            PopupState::WorkspaceCreate {
                fields,
                focused_field,
                ..
            }
            | PopupState::RoomCreate {
                fields,
                focused_field,
                ..
            }
            | PopupState::WorkspaceDelete {
                fields,
                focused_field,
                ..
            }
            | PopupState::RoomDelete {
                fields,
                focused_field,
                ..
            } => {
                let idx = *focused_field;
                if let Some(DialogField::TextInput { value, .. }) = fields.get_mut(idx) {
                    value.push_str(text);
                    refresh_workspace_completions =
                        matches!(self.popup, PopupState::WorkspaceCreate { .. }) && idx == 1;
                    true
                } else {
                    true
                }
            }
            _ => false,
        };

        if refresh_workspace_completions {
            self.refresh_completions();
        }

        pasted
    }

    /// Spawn an arbitrary command in a new PTY pane without going through presets.
    fn spawn_command(
        &mut self,
        cmd: &str,
        args: &[String],
        cwd: &std::path::Path,
        preset_name: &str,
        cols: u16,
        rows: u16,
    ) -> Option<PaneId> {
        let id = PaneId::new();
        let pane = PtyPane::spawn_with_envs(cmd, args, Some(cwd), cols, rows, &[]).ok()?;
        self.local_panes.insert(id, pane);
        self.pane_presets.insert(id, preset_name.to_string());
        Some(id)
    }

    /// Open the selected explorer entry: toggle directories, open files in $EDITOR.
    fn explorer_select(&mut self) {
        let entry = match self.explorer_state.selected_entry() {
            Some(e) => e.clone(),
            None => return,
        };
        if entry.kind == humu::explorer::FileKind::Directory {
            self.explorer_state.toggle_dir();
            return;
        }
        // Open file in $EDITOR as floating pane
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
        let cwd = self.explorer_state.root.clone();
        let filepath = entry.path.clone();
        let args = vec![filepath.to_string_lossy().into_owned()];
        let title = entry.name.clone();
        let fp = self.floating_pane_area();
        let (cols, rows) = (fp.width.saturating_sub(2), fp.height.saturating_sub(2));
        if let Some(id) = self.spawn_command(&editor, &args, &cwd, "_editor", cols, rows) {
            self.popup = PopupState::FloatingPane { pane_id: id, title };
        }
    }

    /// Open a git diff view for the selected modified file using delta.
    fn explorer_diff_file(&mut self) {
        let entry = match self.explorer_state.selected_entry() {
            Some(e)
                if e.kind == humu::explorer::FileKind::File
                    && e.git_status == Some(humu::explorer::GitStatus::Modified) =>
            {
                e.clone()
            }
            _ => return,
        };
        if !self.explorer_state.check_delta() {
            self.show_error(
                "delta not installed — install from https://github.com/dandavison/delta",
            );
            return;
        }
        let cwd = self.explorer_state.root.clone();
        let rel_path = entry.path.strip_prefix(&cwd).unwrap_or(&entry.path);
        let escaped_path = rel_path.display().to_string().replace('\'', "'\\''");
        let diff_cmd = format!(
            "git diff '{}' | delta --side-by-side --paging=always",
            escaped_path
        );
        let args = vec!["-c".to_string(), diff_cmd];
        let title = format!("diff: {}", entry.name);
        let fp = self.floating_pane_area();
        let (cols, rows) = (fp.width.saturating_sub(2), fp.height.saturating_sub(2));
        if let Some(id) = self.spawn_command("sh", &args, &cwd, "_diff", cols, rows) {
            self.popup = PopupState::FloatingPane { pane_id: id, title };
        }
    }

    /// Create a new file or directory in the explorer's current directory context.
    fn explorer_create_entry(&mut self, is_dir: bool, name: &str) {
        // Validate name
        let name = name.trim();
        if name.is_empty() {
            self.show_error("Name cannot be empty");
            return;
        }
        if name.contains('/') || name.contains('\\') {
            self.show_error("Name cannot contain path separators");
            return;
        }
        if name == "." || name == ".." {
            self.show_error("Invalid name");
            return;
        }

        let parent = if let Some(entry) = self.explorer_state.selected_entry() {
            if entry.kind == humu::explorer::FileKind::Directory {
                entry.path.clone()
            } else {
                entry
                    .path
                    .parent()
                    .unwrap_or(&self.explorer_state.root)
                    .to_path_buf()
            }
        } else {
            self.explorer_state.root.clone()
        };
        let target = parent.join(name);

        // Check if already exists
        if target.exists() {
            self.show_error(format!("\"{}\" already exists", name));
            return;
        }

        let result = if is_dir {
            std::fs::create_dir_all(&target)
        } else {
            std::fs::File::create(&target).map(|_| ())
        };
        if let Err(e) = result {
            self.show_error(format!("Failed to create: {e}"));
        } else {
            // Expand the parent directory so the new entry is visible.
            self.explorer_state.expanded_dirs.insert(parent);
            self.explorer_state.scan();
            // Select the newly created entry.
            if let Some(idx) = self
                .explorer_state
                .entries
                .iter()
                .position(|e| e.path == target)
            {
                self.explorer_state.selected = idx;
                let viewport = self.panel_rects.explorer.height.saturating_sub(2) as usize;
                self.explorer_state.scroll_to_visible(viewport);
            }
        }
    }

    /// Delete a file or directory from the explorer.
    fn explorer_delete_entry(&mut self, path: &std::path::Path) {
        let result = if path.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };
        if let Err(e) = result {
            self.show_error(format!("Failed to delete: {e}"));
        } else {
            self.explorer_state.scan();
        }
    }

    /// Switch to the room identified by the current workspace/room selection,
    /// suspending the current room and restoring the target room.
    fn switch_to_selected_room(&mut self) {
        let target_ws_id = match self.workspace_selected {
            Some(id) => id,
            None => return,
        };

        // Preserve an explicit room selection when it already belongs to the
        // target workspace (for example clicking room2 in another workspace).
        // Otherwise clear stale selection so the usual last-room fallback applies.
        let selected_room_in_target = self.room_selected.filter(|rid| {
            self.state
                .ws_by_id(target_ws_id)
                .and_then(|ws| ws.room_by_id(*rid))
                .is_some()
        });
        if selected_room_in_target.is_none() {
            self.room_selected = None;
        }

        // Resolve room ID:
        // 1. If room_selected is set (same workspace navigation), use it.
        // 2. Otherwise, restore the last-used room for this workspace.
        // 3. Otherwise, use the first discovered room (default/main).
        // 4. Otherwise, ensure the "main" room entry exists.
        let target_room_id = if let Some(rid) = self.room_selected {
            rid
        } else if let Some(last) = self.state.ws_by_id(target_ws_id).and_then(|w| {
            // Only use last_room_id if the room entry still exists.
            w.last_room_id.filter(|id| w.room_by_id(*id).is_some())
        }) {
            self.room_selected = Some(last);
            last
        } else {
            let items = self.room_items_for_workspace(target_ws_id);
            match items.first() {
                Some(r) if r.id.is_some() => {
                    let id = r.id.unwrap();
                    self.room_selected = Some(id);
                    id
                }
                Some(r) => {
                    // Room exists in git but not in state — create the entry.
                    match humu::config::create_room_for_workspace(
                        &mut self.state,
                        target_ws_id,
                        &r.name,
                        r.path.clone(),
                    ) {
                        Some(id) => {
                            self.room_selected = Some(id);
                            id
                        }
                        None => return,
                    }
                }
                None => {
                    // No rooms discovered — create default room at workspace root.
                    let ws_path = match self.state.ws_by_id(target_ws_id) {
                        Some(w) => w.path.clone(),
                        None => return,
                    };
                    match humu::config::create_room_for_workspace(
                        &mut self.state,
                        target_ws_id,
                        DEFAULT_ROOM_NAME,
                        ws_path,
                    ) {
                        Some(id) => {
                            self.room_selected = Some(id);
                            id
                        }
                        None => return,
                    }
                }
            }
        };

        // Save last room on the current workspace before suspending.
        if let (Some(ws_id), Some(room_id)) =
            (self.state.active_workspace_id, self.state.active_room_id)
        {
            if let Some(ws) = self.state.ws_by_id_mut(ws_id) {
                ws.last_room_id = Some(room_id);
            }
        }

        // Suspend current room (preserves live PTY panes).
        self.suspend_current_room();

        // Update active IDs directly.
        self.state.active_workspace_id = Some(target_ws_id);
        self.state.active_room_id = Some(target_room_id);

        // Restore target room (hot if suspended, cold otherwise).
        self.restore_room(target_ws_id, target_room_id);

        // Reset explorer to the new room's path.
        if let Some(path) = self.current_room_path() {
            self.explorer_state = humu::explorer::ExplorerState::new(path);
            self.explorer_state.scan();
        }

        // Refresh room git stats immediately so the panel doesn't show stale/empty data.
        self.refresh_room_cache();
        self.rebuild_workspace_tree();

        self.save_state();
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.clear_live_panes();
        let _ = self.send_daemon_request(ClientRequest::Detach);
    }
}

/// Returns true if the ranges [a_start, a_end) and [b_start, b_end) overlap.
fn ranges_overlap(a_start: u16, a_end: u16, b_start: u16, b_end: u16) -> bool {
    a_start < b_end && b_start < a_end
}

/// Compute the CSI u modifier parameter: 1 + bitmask(shift=1, alt=2, ctrl=4).
fn csi_u_modifier(modifiers: KeyModifiers) -> u8 {
    1 + if modifiers.contains(KeyModifiers::SHIFT) {
        1
    } else {
        0
    } + if modifiers.contains(KeyModifiers::ALT) {
        2
    } else {
        0
    } + if modifiers.contains(KeyModifiers::CONTROL) {
        4
    } else {
        0
    }
}

fn normalize_key_event(mut key: KeyEvent) -> KeyEvent {
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && let KeyCode::Char(c) = key.code
        && let Some(mapped) = hangul_2set_jamo_to_qwerty(c)
    {
        key.code = KeyCode::Char(mapped);
    }

    key
}

fn hangul_2set_jamo_to_qwerty(c: char) -> Option<char> {
    Some(match c {
        'ㅂ' => 'q',
        'ㅈ' => 'w',
        'ㄷ' => 'e',
        'ㄱ' => 'r',
        'ㅅ' => 't',
        'ㅛ' => 'y',
        'ㅕ' => 'u',
        'ㅑ' => 'i',
        'ㅐ' => 'o',
        'ㅔ' => 'p',
        'ㅁ' => 'a',
        'ㄴ' => 's',
        'ㅇ' => 'd',
        'ㄹ' => 'f',
        'ㅎ' => 'g',
        'ㅗ' => 'h',
        'ㅓ' => 'j',
        'ㅏ' => 'k',
        'ㅣ' => 'l',
        'ㅋ' => 'z',
        'ㅌ' => 'x',
        'ㅊ' => 'c',
        'ㅍ' => 'v',
        'ㅠ' => 'b',
        'ㅜ' => 'n',
        'ㅡ' => 'm',
        'ㅃ' => 'q',
        'ㅉ' => 'w',
        'ㄸ' => 'e',
        'ㄲ' => 'r',
        'ㅆ' => 't',
        'ㅒ' => 'o',
        'ㅖ' => 'p',
        _ => return None,
    })
}

fn key_event_to_bytes(key: &KeyEvent) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let has_modifier = key.modifiers != KeyModifiers::NONE;
    match key.code {
        KeyCode::Char(c) if ctrl => {
            let base = vec![(c as u8) & 0x1f];
            if alt {
                [b"\x1b".as_slice(), &base].concat()
            } else {
                base
            }
        }
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            let base = s.as_bytes().to_vec();
            if alt {
                [b"\x1b".as_slice(), &base].concat()
            } else {
                base
            }
        }
        KeyCode::Enter if has_modifier => {
            // CSI u format: \x1b[13;{modifier}u
            format!("\x1b[13;{}u", csi_u_modifier(key.modifiers)).into_bytes()
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab if has_modifier => {
            format!("\x1b[9;{}u", csi_u_modifier(key.modifiers)).into_bytes()
        }
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::F(n) => match n {
            1 => b"\x1bOP".to_vec(),
            2 => b"\x1bOQ".to_vec(),
            3 => b"\x1bOR".to_vec(),
            4 => b"\x1bOS".to_vec(),
            5..=12 => format!("\x1b[{n}~").into_bytes(),
            _ => vec![],
        },
        _ => vec![],
    }
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[(n >> 18 & 63) as usize] as char);
        result.push(CHARS[(n >> 12 & 63) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[(n >> 6 & 63) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(n & 63) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn should_handle_key_event(key: &KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

#[cfg(test)]
impl App {
    pub fn test_with_state(state: HumuState, state_path: PathBuf) -> Self {
        let config = HumuConfig::default();

        Self {
            config,
            state,
            mode: Mode::Terminal,
            focus: FocusedPanel::Terminal,
            workspace_selected: None,
            room_selected: None,
            running: true,
            local_panes: HashMap::new(),
            tabs: TabContainer::new(),
            focused_pane: None,
            pane_presets: HashMap::new(),
            popup: PopupState::None,
            agent_states: HashMap::new(),
            hook_port: None,
            server_stream: None,
            attached_snapshot: None,
            panel_rects: PanelRects {
                workspace: Rect::new(0, 0, 24, 8),
                terminal: Rect::new(24, 0, 56, 20),
                explorer: Rect::new(80, 0, 20, 20),
                tab_bar: Rect::new(24, 0, 56, 1),
                status_bar: Rect::new(0, 20, 100, 1),
            },
            panel_widths: [24, 20],
            pty_mouse_active: false,
            is_focused: true,
            selection: None,
            fullscreen_pane: None,
            palette: humu::tui::theme::Palette::GITHUB_DARK,
            ui_config: humu::tui::theme::UiConfig {
                simplified_ui: false,
                rounded_corners: true,
            },
            spin_tick: 0,
            suspended_rooms: HashMap::new(),
            search_state: None,
            explorer_state: humu::explorer::ExplorerState::new(PathBuf::new()),
            room_cache: HashMap::new(),
            workspace_tree_cache: Vec::new(),
            selected_tree_index: 0,
            workspace_mode_entered: None,
            state_path,
            config_path: PathBuf::from("/tmp/humu-test-config.yaml"),
        }
    }

    pub fn test_persist_layout(&mut self) {
        self.persist_layout();
    }

    pub fn test_hydrate_attached_snapshot(&mut self, snapshot: FullSnapshot) {
        self.attached_snapshot = Some(snapshot);
        self.hydrate_attached_snapshot();
    }

    pub fn test_attached_screen_contents(&self, pane_id: PaneId) -> Option<String> {
        self.pane_screen_contents(pane_id)
    }

    pub fn test_pane_input_state(&self, pane_id: PaneId) -> Option<PaneInputState> {
        self.pane_input_state(pane_id)
    }

    pub fn test_search_matches_for_query(
        &mut self,
        pane_id: PaneId,
        query: &str,
    ) -> Vec<(usize, usize, usize)> {
        self.focused_pane = Some(pane_id);
        self.search_state = Some(SearchState {
            query: query.to_string(),
            matches: Vec::new(),
            active_index: None,
            case_sensitive: true,
            wrap: false,
        });
        self.run_search();
        self.search_state
            .as_ref()
            .map(|state| {
                state
                    .matches
                    .iter()
                    .map(|sm| (sm.row, sm.col_start, sm.col_end))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn test_state_path(&self) -> &std::path::Path {
        &self.state_path
    }

    pub fn test_remove_room_state(&mut self, ws_id: WorkspaceId, room_id: RoomId) {
        self.remove_room_from_state(ws_id, room_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    fn ctrl_char(c: char) -> KeyEvent {
        KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn normalize_key_event_maps_ctrl_hangul_jamo_to_qwerty() {
        let normalized = normalize_key_event(ctrl_char('ㅊ'));
        assert_eq!(normalized.code, KeyCode::Char('c'));
    }

    #[test]
    fn normalize_key_event_leaves_plain_hangul_input_unchanged() {
        let key = KeyEvent {
            code: KeyCode::Char('ㅊ'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };

        assert_eq!(normalize_key_event(key).code, KeyCode::Char('ㅊ'));
    }

    #[test]
    fn ctrl_hangul_jamo_emits_matching_ascii_control_byte() {
        let normalized = normalize_key_event(ctrl_char('ㅊ'));
        assert_eq!(key_event_to_bytes(&normalized), vec![0x03]);
    }

    #[test]
    fn should_handle_key_event_accepts_press_and_repeat() {
        let press = KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        let repeat = KeyEvent {
            kind: KeyEventKind::Repeat,
            ..press
        };

        assert!(should_handle_key_event(&press));
        assert!(should_handle_key_event(&repeat));
    }

    #[test]
    fn should_handle_key_event_ignores_release() {
        let release = KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: crossterm::event::KeyEventState::NONE,
        };

        assert!(!should_handle_key_event(&release));
    }

    fn test_app_with_workspace_tree(
        state: HumuState,
        workspace_tree_cache: Vec<WorkspaceTreeItem>,
        room_cache: HashMap<WorkspaceId, Vec<CachedRoomInfo>>,
    ) -> App {
        let config = HumuConfig::default();

        App {
            config,
            state,
            mode: Mode::Terminal,
            focus: FocusedPanel::Terminal,
            workspace_selected: None,
            room_selected: None,
            running: true,
            local_panes: HashMap::new(),
            tabs: TabContainer::new(),
            focused_pane: None,
            pane_presets: HashMap::new(),
            popup: PopupState::None,
            agent_states: HashMap::new(),
            hook_port: None,
            server_stream: None,
            attached_snapshot: None,
            panel_rects: PanelRects {
                workspace: Rect::new(0, 0, 24, 8),
                terminal: Rect::new(24, 0, 56, 20),
                explorer: Rect::new(80, 0, 20, 20),
                tab_bar: Rect::new(24, 0, 56, 1),
                status_bar: Rect::new(0, 20, 100, 1),
            },
            panel_widths: [24, 20],
            pty_mouse_active: false,
            is_focused: true,
            selection: None,
            fullscreen_pane: None,
            palette: humu::tui::theme::Palette::GITHUB_DARK,
            ui_config: humu::tui::theme::UiConfig {
                simplified_ui: false,
                rounded_corners: true,
            },
            spin_tick: 0,
            suspended_rooms: HashMap::new(),
            search_state: None,
            explorer_state: humu::explorer::ExplorerState::new(PathBuf::new()),
            room_cache,
            workspace_tree_cache,
            selected_tree_index: 0,
            workspace_mode_entered: None,
            state_path: PathBuf::from("/tmp/humu-test-state.yaml"),
            config_path: PathBuf::from("/tmp/humu-test-config.yaml"),
        }
    }

    fn workspace_room_fixture() -> (App, WorkspaceId, RoomId, RoomId) {
        let ws_id = WorkspaceId::new();
        let local_room_id = RoomId::new();
        let feature_room_id = RoomId::new();
        let workspace_path = PathBuf::from("/tmp/humu-workspace");
        let feature_path = workspace_path.join("feature");

        let state = HumuState {
            active_workspace_id: Some(ws_id),
            active_room_id: Some(local_room_id),
            workspaces: vec![WorkspaceEntry {
                name: "humu".to_string(),
                id: ws_id,
                path: workspace_path.clone(),
                last_room_id: Some(local_room_id),
                rooms: vec![
                    humu::config::RoomEntry {
                        name: "local".to_string(),
                        id: local_room_id,
                        path: workspace_path.clone(),
                        active_tab: None,
                        tabs: vec![],
                    },
                    humu::config::RoomEntry {
                        name: "feature".to_string(),
                        id: feature_room_id,
                        path: feature_path.clone(),
                        active_tab: None,
                        tabs: vec![],
                    },
                ],
            }],
            sessions: vec![],
            panel_widths: Some([24, 20]),
        };

        let workspace_tree_cache = vec![
            WorkspaceTreeItem {
                kind: TreeItemKind::Workspace,
                name: "hhk7734/humu".to_string(),
                active: false,
                workspace_id: ws_id,
                room_id: None,
                room_path: None,
                git_status: RoomGitStatus::default(),
            },
            WorkspaceTreeItem {
                kind: TreeItemKind::Room,
                name: "local".to_string(),
                active: false,
                workspace_id: ws_id,
                room_id: Some(local_room_id),
                room_path: Some(workspace_path.clone()),
                git_status: RoomGitStatus::default(),
            },
            WorkspaceTreeItem {
                kind: TreeItemKind::Room,
                name: "feature".to_string(),
                active: false,
                workspace_id: ws_id,
                room_id: Some(feature_room_id),
                room_path: Some(feature_path.clone()),
                git_status: RoomGitStatus::default(),
            },
        ];

        let room_cache = HashMap::from([(
            ws_id,
            vec![
                CachedRoomInfo {
                    room_id: Some(local_room_id),
                    name: "local".to_string(),
                    path: workspace_path,
                    git_status: RoomGitStatus::default(),
                },
                CachedRoomInfo {
                    room_id: Some(feature_room_id),
                    name: "feature".to_string(),
                    path: feature_path,
                    git_status: RoomGitStatus::default(),
                },
            ],
        )]);

        let mut app = test_app_with_workspace_tree(state, workspace_tree_cache, room_cache);
        app.suspended_rooms.insert(
            (ws_id, feature_room_id),
            RoomState {
                local_panes: HashMap::new(),
                tabs: TabContainer::new(),
                pane_presets: HashMap::new(),
                focused_pane: None,
                fullscreen_pane: None,
            },
        );

        (app, ws_id, local_room_id, feature_room_id)
    }

    fn record_daemon_requests(
        app: &mut App,
        action: impl FnOnce(&mut App),
    ) -> Vec<ClientRequest> {
        let (client_stream, mut daemon_stream) = UnixStream::pair().expect("create unix stream pair");
        daemon_stream
            .set_read_timeout(Some(Duration::from_millis(250)))
            .expect("set read timeout");
        app.server_stream = Some(client_stream);

        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let mut decoder = FrameDecoder::new();
            let mut buf = [0u8; 4096];
            loop {
                match daemon_stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(read) => {
                        decoder.push(&buf[..read]);
                        while let Some(request) =
                            decoder.try_decode::<ClientRequest>().expect("decode client request")
                        {
                            tx.send(request).expect("send request to test");
                            daemon_stream
                                .write_all(&encode_frame(&ServerResponse::Ack).expect("encode ack"))
                                .expect("write daemon ack");
                        }
                    }
                    Err(err)
                        if matches!(
                            err.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        break;
                    }
                    Err(err) => panic!("read daemon request: {err}"),
                }
            }
        });

        action(app);
        app.server_stream.take();
        handle.join().expect("join daemon recorder");
        rx.try_iter().collect()
    }

    fn left_click(column: u16, row: u16) -> crossterm::event::MouseEvent {
        crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn scroll_down(column: u16, row: u16) -> crossterm::event::MouseEvent {
        crossterm::event::MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn first_workspace_room_click_only_selects_and_enters_workspace_mode() {
        let (mut app, ws_id, local_room_id, feature_room_id) = workspace_room_fixture();

        app.handle_mouse(left_click(2, 4));

        assert_eq!(app.selected_tree_index, 2);
        assert_eq!(app.workspace_selected, Some(ws_id));
        assert_eq!(app.room_selected, Some(feature_room_id));
        assert_eq!(app.state.active_room_id, Some(local_room_id));
        assert_eq!(app.mode, Mode::Workspace);
        assert_eq!(app.focus, FocusedPanel::Workspace);
    }

    #[test]
    fn second_workspace_room_click_activates_selected_room() {
        let (mut app, ws_id, _local_room_id, feature_room_id) = workspace_room_fixture();

        app.handle_mouse(left_click(2, 4));
        app.handle_mouse(left_click(2, 4));

        assert_eq!(app.state.active_workspace_id, Some(ws_id));
        assert_eq!(app.state.active_room_id, Some(feature_room_id));
        assert_eq!(app.mode, Mode::Terminal);
        assert_eq!(app.focus, FocusedPanel::Terminal);
    }

    #[test]
    fn workspace_panel_scroll_moves_selection() {
        let (mut app, _ws_id, _local_room_id, _feature_room_id) = workspace_room_fixture();

        app.handle_mouse(scroll_down(2, 2));

        assert_eq!(app.selected_tree_index, 1);
        assert_eq!(app.mode, Mode::Workspace);
        assert_eq!(app.focus, FocusedPanel::Workspace);
    }

    #[test]
    fn workspace_create_existing_mode_still_requires_path() {
        let mut app = test_app_with_workspace_tree(HumuState::default(), vec![], HashMap::new());

        app.execute_workspace_create(vec![
            DialogField::Select {
                label: "Mode".to_string(),
                options: vec![
                    "Clone".to_string(),
                    "Existing".to_string(),
                    "New".to_string(),
                ],
                selected: 1,
            },
            DialogField::TextInput {
                label: "Path".to_string(),
                value: String::new(),
            },
            DialogField::TextInput {
                label: "URL (Clone only)".to_string(),
                value: String::new(),
            },
        ]);

        assert!(matches!(
            app.popup,
            PopupState::ErrorDialog { ref message } if message == "Path is required"
        ));
    }

    #[test]
    fn workspace_create_dialog_click_focuses_text_field_without_selecting_terminal() {
        let mut app = test_app_with_workspace_tree(HumuState::default(), vec![], HashMap::new());
        app.mode = Mode::Workspace;
        app.focus = FocusedPanel::Workspace;
        app.show_create_workspace_dialog();

        let area = app.dialog_popup_area(3);
        let path_row = area.y + 4;

        app.handle_mouse(left_click(area.x + 2, path_row));

        assert_eq!(app.mode, Mode::Workspace);
        assert_eq!(app.focus, FocusedPanel::Workspace);
        match &app.popup {
            PopupState::WorkspaceCreate { focused_field, .. } => assert_eq!(*focused_field, 1),
            _ => panic!("expected workspace-create popup"),
        }
    }

    #[test]
    fn workspace_create_paste_targets_clicked_text_field() {
        let mut app = test_app_with_workspace_tree(HumuState::default(), vec![], HashMap::new());
        app.mode = Mode::Workspace;
        app.focus = FocusedPanel::Workspace;
        app.show_create_workspace_dialog();

        let area = app.dialog_popup_area(3);
        let path_row = area.y + 4;
        app.handle_mouse(left_click(area.x + 2, path_row));
        app.handle_paste_event("/tmp/humu");

        match &app.popup {
            PopupState::WorkspaceCreate {
                fields,
                focused_field,
                ..
            } => {
                assert_eq!(*focused_field, 1);
                assert!(matches!(
                    &fields[1],
                    DialogField::TextInput { value, .. } if value == "/tmp/humu"
                ));
            }
            _ => panic!("expected workspace-create popup"),
        }
    }

    #[test]
    fn workspace_click_does_not_start_terminal_selection() {
        let (mut app, _ws_id, _local_room_id, _feature_room_id) = workspace_room_fixture();
        let pane_id = PaneId::new();
        let pane = PtyPane::spawn("true", &[], None, 80, 24).unwrap();
        app.local_panes.insert(pane_id, pane);
        app.tabs.add_tab("shell".into(), SplitTree::leaf(pane_id));
        app.focused_pane = Some(pane_id);

        app.handle_mouse(left_click(2, 2));

        assert!(app.selection.is_none());
        assert_eq!(app.focus, FocusedPanel::Workspace);
    }

    #[test]
    fn tab_bar_click_does_not_start_terminal_selection() {
        let (mut app, _ws_id, _local_room_id, _feature_room_id) = workspace_room_fixture();
        let pane_id = PaneId::new();
        let pane = PtyPane::spawn("true", &[], None, 80, 24).unwrap();
        app.local_panes.insert(pane_id, pane);
        app.pane_presets.insert(pane_id, "shell".to_string());
        app.tabs.add_tab("shell".into(), SplitTree::leaf(pane_id));
        app.focused_pane = Some(pane_id);

        app.handle_mouse(left_click(
            app.panel_rects.tab_bar.x + 1,
            app.panel_rects.tab_bar.y,
        ));

        assert!(app.selection.is_none());
    }

    #[test]
    fn layout_hot_restore_reuses_suspended_room_state() {
        let (mut app, ws_id, local_room_id, _feature_room_id) = workspace_room_fixture();
        let pane_id = PaneId::new();
        let pane = PtyPane::spawn("true", &[], None, 80, 24).unwrap();
        app.local_panes.insert(pane_id, pane);
        app.pane_presets.insert(pane_id, "shell".to_string());
        app.tabs.add_tab("shell".into(), SplitTree::leaf(pane_id));
        app.focused_pane = Some(pane_id);

        app.suspend_current_room();

        assert!(app.local_panes.is_empty());
        assert!(app.suspended_rooms.contains_key(&(ws_id, local_room_id)));

        app.restore_room(ws_id, local_room_id);

        assert!(!app.suspended_rooms.contains_key(&(ws_id, local_room_id)));
        assert!(app.local_panes.contains_key(&pane_id));
        assert_eq!(
            app.pane_presets.get(&pane_id).map(String::as_str),
            Some("shell")
        );
        assert_eq!(app.focused_pane, Some(pane_id));
    }

    #[test]
    fn layout_cold_restore_spawns_persisted_presets() {
        let (mut app, ws_id, _local_room_id, feature_room_id) = workspace_room_fixture();
        app.state.active_workspace_id = Some(ws_id);
        app.state.active_room_id = Some(feature_room_id);
        app.tabs = TabContainer::new();
        app.local_panes.clear();
        app.pane_presets.clear();
        app.focused_pane = None;
        app.suspended_rooms.clear();

        app.config.presets.get_mut("shell").unwrap().command = "sh".to_string();
        app.config.presets.get_mut("shell").unwrap().args =
            vec!["-c".to_string(), "true".to_string()];

        if let Some(claude) = app.config.presets.get_mut("claude") {
            claude.command = "sh".to_string();
            claude.args = vec!["-c".to_string(), "true".to_string()];
        }

        let session = app.state.ensure_session(HumuState::DEFAULT_SESSION_NAME);
        session.tabs_by_room.insert(
            feature_room_id,
            humu::config::PersistedRoomLayout {
                active_tab: 0,
                tabs: vec![TabLayout {
                    name: "restored".to_string(),
                    split: SplitNode::Split {
                        direction: CfgDir::Vertical,
                        ratio: 0.5,
                        children: vec![
                            SplitNode::Leaf {
                                preset: "shell".to_string(),
                                session_id: None,
                            },
                            SplitNode::Leaf {
                                preset: "claude".to_string(),
                                session_id: None,
                            },
                        ],
                    },
                }],
            },
        );

        app.restore_room(ws_id, feature_room_id);

        let presets: std::collections::HashSet<_> = app.pane_presets.values().cloned().collect();
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.tabs.active_name(), "restored");
        assert!(presets.contains("shell"));
        assert!(presets.contains("claude"));
    }

    #[test]
    fn codex_restore_preserves_session_id_on_cold_restore() {
        let (mut app, ws_id, _local_room_id, feature_room_id) = workspace_room_fixture();
        app.state.active_workspace_id = Some(ws_id);
        app.state.active_room_id = Some(feature_room_id);
        app.tabs = TabContainer::new();
        app.local_panes.clear();
        app.pane_presets.clear();
        app.focused_pane = None;
        app.suspended_rooms.clear();

        if let Some(codex) = app.config.presets.get_mut("codex") {
            codex.command = "sh".to_string();
            codex.args = vec!["-c".to_string(), "true".to_string()];
        }

        let session = app.state.ensure_session(HumuState::DEFAULT_SESSION_NAME);
        session.tabs_by_room.insert(
            feature_room_id,
            humu::config::PersistedRoomLayout {
                active_tab: 0,
                tabs: vec![TabLayout {
                    name: "codex".to_string(),
                    split: SplitNode::Leaf {
                        preset: "codex".to_string(),
                        session_id: Some("session-xyz".to_string()),
                    },
                }],
            },
        );

        app.restore_room(ws_id, feature_room_id);

        let pane_id = app.focused_pane.expect("focused pane after codex restore");
        assert_eq!(
            app.agent_states
                .get(&pane_id)
                .and_then(|entry| entry.session_id.as_deref()),
            Some("session-xyz")
        );
        assert_eq!(
            app.pane_presets.get(&pane_id).map(String::as_str),
            Some("codex")
        );
    }

    #[test]
    fn codex_spawn_args_preserve_existing_args_before_resume() {
        let mut args = vec!["--yolo".to_string()];

        append_codex_args(&mut args, Some("session-123".to_string()));

        assert_eq!(
            args,
            vec![
                "--yolo".to_string(),
                "resume".to_string(),
                "session-123".to_string(),
            ]
        );
    }

    #[test]
    fn floating_pane_manual_close_unregisters_daemon_pane() {
        let mut app = test_app_with_workspace_tree(HumuState::default(), vec![], HashMap::new());
        let pane_id = PaneId::new();
        let pane = PtyPane::spawn("sh", &["-c".to_string(), "sleep 60".to_string()], None, 80, 24)
            .expect("spawn floating pane");
        app.local_panes.insert(pane_id, pane);
        app.pane_presets.insert(pane_id, "_editor".to_string());
        app.popup = PopupState::FloatingPane {
            pane_id,
            title: "editor".to_string(),
        };

        let requests = record_daemon_requests(&mut app, |app| {
            app.handle_floating_pane_key(pane_id, ctrl_char('q'));
        });

        assert!(!app.local_panes.contains_key(&pane_id));
        assert!(matches!(app.popup, PopupState::None));
        assert!(
            requests.contains(&ClientRequest::UnregisterPane { pane_id }),
            "expected unregister request, got {requests:?}"
        );
    }

    #[test]
    fn floating_pane_auto_close_unregisters_daemon_pane() {
        let mut app = test_app_with_workspace_tree(HumuState::default(), vec![], HashMap::new());
        let pane_id = PaneId::new();
        let pane = PtyPane::spawn("sh", &["-c".to_string(), "true".to_string()], None, 80, 24)
            .expect("spawn auto-close pane");
        app.local_panes.insert(pane_id, pane);
        app.pane_presets.insert(pane_id, "_diff".to_string());
        app.popup = PopupState::FloatingPane {
            pane_id,
            title: "diff".to_string(),
        };
        thread::sleep(Duration::from_millis(50));

        let requests = record_daemon_requests(&mut app, |app| {
            app.cleanup_exited_floating_pane();
        });

        assert!(!app.local_panes.contains_key(&pane_id));
        assert!(matches!(app.popup, PopupState::None));
        assert!(
            requests.contains(&ClientRequest::UnregisterPane { pane_id }),
            "expected unregister request, got {requests:?}"
        );
    }

    #[test]
    fn split_failure_unregisters_spawned_daemon_pane() {
        let mut app = test_app_with_workspace_tree(HumuState::default(), vec![], HashMap::new());
        app.config.presets.get_mut("shell").unwrap().command = "sh".to_string();
        app.config.presets.get_mut("shell").unwrap().args =
            vec!["-c".to_string(), "true".to_string()];

        let focused_pane_id = PaneId::new();
        let focused_pane = PtyPane::spawn("true", &[], None, 80, 24).expect("spawn focused pane");
        app.local_panes.insert(focused_pane_id, focused_pane);
        app.focused_pane = Some(focused_pane_id);

        let requests = record_daemon_requests(&mut app, |app| {
            app.split_pane_with_preset("shell", true);
        });

        let registered_pane_id = requests.iter().find_map(|request| match request {
            ClientRequest::RegisterPane { pane_id, .. } => Some(*pane_id),
            _ => None,
        });
        assert!(registered_pane_id.is_some(), "expected register request, got {requests:?}");
        assert!(
            requests.iter().any(|request| {
                matches!(
                    request,
                    ClientRequest::UnregisterPane { pane_id }
                        if Some(*pane_id) == registered_pane_id
                )
            }),
            "expected unregister for split-failure cleanup, got {requests:?}"
        );
    }

    #[test]
    fn attached_spawn_registers_daemon_pane_without_local_pty() {
        let (mut app, _ws_id, _local_room_id, _feature_room_id) = workspace_room_fixture();
        app.attached_snapshot = Some(FullSnapshot::fixture());
        app.config.presets.get_mut("shell").unwrap().command = "sh".to_string();
        app.config.presets.get_mut("shell").unwrap().args =
            vec!["-c".to_string(), "true".to_string()];

        let requests = record_daemon_requests(&mut app, |app| {
            let pane_id = app
                .spawn_pane("shell", None)
                .expect("spawn attached placeholder");
            app.tabs.add_tab("shell".into(), SplitTree::leaf(pane_id));
            app.focused_pane = Some(pane_id);
        });

        let registered_pane_id = requests.iter().find_map(|request| match request {
            ClientRequest::RegisterPane { pane_id, .. } => Some(*pane_id),
            _ => None,
        });
        assert!(
            registered_pane_id.is_some(),
            "expected register request, got {requests:?}"
        );
        assert!(
            app.local_panes.is_empty(),
            "attached-pane spawn should not leave a local PTY behind"
        );
        assert_eq!(app.tabs.len(), 1);
    }

    #[test]
    fn attached_room_hot_restore_keeps_layout_without_moving_local_floating_pty() {
        let (mut app, ws_id, local_room_id, _feature_room_id) = workspace_room_fixture();
        app.attached_snapshot = Some(FullSnapshot::fixture());

        let attached_pane_id = app.create_attached_placeholder_pane("shell", None);
        app.tabs.add_tab("shell".into(), SplitTree::leaf(attached_pane_id));
        app.focused_pane = Some(attached_pane_id);

        let floating_pane_id = PaneId::new();
        let floating_pane =
            PtyPane::spawn("sh", &["-c".to_string(), "sleep 60".to_string()], None, 80, 24)
                .expect("spawn floating pane");
        app.local_panes.insert(floating_pane_id, floating_pane);
        app.popup = PopupState::FloatingPane {
            pane_id: floating_pane_id,
            title: "floating".to_string(),
        };

        app.suspend_current_room();

        assert!(
            app.suspended_rooms.contains_key(&(ws_id, local_room_id)),
            "expected suspended room metadata"
        );
        assert!(
            app.local_panes.contains_key(&floating_pane_id),
            "floating panes should stay local across attached room suspend"
        );

        app.restore_room(ws_id, local_room_id);

        assert_eq!(app.focused_pane, Some(attached_pane_id));
        assert_eq!(
            app.pane_presets.get(&attached_pane_id).map(String::as_str),
            Some("shell")
        );
        assert!(
            app.local_panes.contains_key(&floating_pane_id),
            "floating panes should remain local after attached room restore"
        );
    }

    #[test]
    fn workspace_delete_unregisters_active_and_suspended_workspace_panes() {
        let (mut app, ws_id, local_room_id, feature_room_id) = workspace_room_fixture();
        app.state.active_workspace_id = Some(ws_id);
        app.state.active_room_id = Some(local_room_id);

        let active_pane_id = PaneId::new();
        let active_pane = PtyPane::spawn("true", &[], None, 80, 24).expect("spawn active pane");
        app.local_panes.insert(active_pane_id, active_pane);
        app.pane_presets.insert(active_pane_id, "shell".to_string());
        app.tabs.add_tab("shell".into(), SplitTree::leaf(active_pane_id));
        app.focused_pane = Some(active_pane_id);

        let suspended_pane_id = PaneId::new();
        let suspended_pane =
            PtyPane::spawn("true", &[], None, 80, 24).expect("spawn suspended pane");
        app.suspended_rooms.insert(
            (ws_id, feature_room_id),
            RoomState {
                local_panes: HashMap::from([(suspended_pane_id, suspended_pane)]),
                tabs: TabContainer::new(),
                pane_presets: HashMap::from([(suspended_pane_id, "shell".to_string())]),
                focused_pane: Some(suspended_pane_id),
                fullscreen_pane: None,
            },
        );

        let requests = record_daemon_requests(&mut app, |app| {
            app.execute_workspace_delete(
                vec![
                    DialogField::Confirm {
                        message: "Delete workspace?".to_string(),
                        yes: true,
                    },
                    DialogField::Checkbox {
                        label: "Delete from disk".to_string(),
                        checked: false,
                    },
                ],
                ws_id,
            );
        });

        assert!(
            requests.contains(&ClientRequest::UnregisterPane {
                pane_id: active_pane_id
            }),
            "expected active pane unregister, got {requests:?}"
        );
        assert!(
            requests.contains(&ClientRequest::UnregisterPane {
                pane_id: suspended_pane_id
            }),
            "expected suspended pane unregister, got {requests:?}"
        );
        assert!(app.local_panes.is_empty());
        assert!(!app.suspended_rooms.contains_key(&(ws_id, feature_room_id)));
    }
}
