use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::id::{RoomId, WorkspaceId};

// ── Directory helper ──────────────────────────────────────────────────────────

/// Returns the humu data directory, creating it if it does not exist.
///
/// Resolution order:
/// 1. `HUMU_DIR` environment variable (useful for testing)
/// 2. `~/.humu/` (default)
pub fn humu_dir() -> PathBuf {
    let dir = match std::env::var("HUMU_DIR") {
        Ok(val) if !val.is_empty() => PathBuf::from(val),
        _ => dirs::home_dir()
            .expect("cannot determine home directory")
            .join(".humu"),
    };
    std::fs::create_dir_all(&dir).expect("cannot create humu directory");
    dir
}

// ── Preset ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Preset {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

// ── UI Section ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSection {
    #[serde(default)]
    pub simplified_ui: bool,
    #[serde(default = "default_rounded_corners")]
    pub rounded_corners: bool,
}

fn default_rounded_corners() -> bool {
    true
}

impl Default for UiSection {
    fn default() -> Self {
        Self {
            simplified_ui: false,
            rounded_corners: true,
        }
    }
}

// ── Notification config ──────────────────────────────────────────────────────

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsNotificationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub only_unfocused: bool,
}

impl Default for OsNotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            only_unfocused: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoundNotificationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub only_unfocused: bool,
}

impl Default for SoundNotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            only_unfocused: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelegramNotificationConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub only_unfocused: bool,
    #[serde(default)]
    pub bot_token_encrypted: String,
    #[serde(default)]
    pub chat_id_encrypted: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationsConfig {
    #[serde(default)]
    pub os: OsNotificationConfig,
    #[serde(default)]
    pub sound: SoundNotificationConfig,
    #[serde(default)]
    pub telegram: TelegramNotificationConfig,
}

// ── HumuConfig ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumuConfig {
    #[serde(default)]
    pub presets: HashMap<String, Preset>,
    #[serde(default)]
    pub ui: UiSection,
    #[serde(default)]
    pub notifications: NotificationsConfig,
}

impl Default for HumuConfig {
    fn default() -> Self {
        let mut presets = HashMap::new();
        presets.insert(
            "claude".to_string(),
            Preset {
                command: "claude".to_string(),
                args: vec!["--dangerously-skip-permissions".to_string()],
            },
        );
        presets.insert(
            "gemini".to_string(),
            Preset {
                command: "gemini".to_string(),
                args: vec!["--yolo".to_string()],
            },
        );
        presets.insert(
            "codex".to_string(),
            Preset {
                command: "codex".to_string(),
                args: vec!["--yolo".to_string()],
            },
        );
        presets.insert(
            "shell".to_string(),
            Preset {
                command: std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
                args: vec![],
            },
        );
        Self {
            presets,
            ui: UiSection::default(),
            notifications: NotificationsConfig::default(),
        }
    }
}

impl HumuConfig {
    pub fn apply_builtin_presets(&mut self) {
        let defaults = Self::default();
        for (name, preset) in defaults.presets {
            self.presets.entry(name).or_insert(preset);
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let mut config: Self = serde_yaml::from_str(&contents)?;
        config.apply_builtin_presets();
        Ok(config)
    }

    /// Load from TOML format (migration from old config.toml).
    pub fn load_toml(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let mut config: Self = toml::from_str(&contents)?;
        config.apply_builtin_presets();
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let contents = serde_yaml::to_string(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}

// ── Layout types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Vertical,
    Horizontal,
}

/// IMPORTANT: `Leaf` must come before `Split` so that `#[serde(untagged)]`
/// tries the simpler variant first during deserialization.
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TabLayout {
    pub name: String,
    pub split: SplitNode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PersistedRoomLayout {
    #[serde(default)]
    pub active_tab: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<TabLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SessionSize {
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SessionState {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_workspace_id: Option<WorkspaceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_room_id: Option<RoomId>,
    #[serde(default)]
    pub tabs_by_room: HashMap<RoomId, PersistedRoomLayout>,
    #[serde(default)]
    pub attached: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_size: Option<SessionSize>,
}

// ── RoomEntry ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomEntry {
    pub name: String,
    pub id: RoomId,
    /// Actual worktree path (repo root for default room).
    #[serde(default)]
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_tab: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<TabLayout>,
}

// ── WorkspaceEntry ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceEntry {
    pub name: String,
    pub id: WorkspaceId,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_room_id: Option<RoomId>,
    #[serde(default)]
    pub rooms: Vec<RoomEntry>,
}

impl WorkspaceEntry {
    pub fn room_by_path(&self, path: &Path) -> Option<&RoomEntry> {
        self.rooms.iter().find(|r| paths_match(&r.path, path))
    }

    pub fn room_by_id(&self, id: RoomId) -> Option<&RoomEntry> {
        self.rooms.iter().find(|r| r.id == id)
    }

    pub fn room_by_id_mut(&mut self, id: RoomId) -> Option<&mut RoomEntry> {
        self.rooms.iter_mut().find(|r| r.id == id)
    }
}

// ── HumuState ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HumuState {
    pub active_workspace_id: Option<WorkspaceId>,
    pub active_room_id: Option<RoomId>,
    #[serde(default)]
    pub workspaces: Vec<WorkspaceEntry>,
    #[serde(default)]
    pub sessions: Vec<SessionState>,
    /// Panel widths: [workspace_panel, explorer_panel]. Persisted across restarts.
    #[serde(default, deserialize_with = "deserialize_panel_widths")]
    pub panel_widths: Option<[u16; 2]>,
}

fn deserialize_panel_widths<'de, D>(deserializer: D) -> Result<Option<[u16; 2]>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<Vec<u16>> = Option::deserialize(deserializer)?;
    Ok(opt.map(|v| match v.len() {
        2 => [v[0], v[1]],
        3 => [v[0], v[2]], // legacy: [workspace, room, explorer] → [workspace, explorer]
        _ => [25, 25],
    }))
}

impl HumuState {
    pub const DEFAULT_SESSION_NAME: &str = "default";

    pub fn ws_by_id(&self, id: WorkspaceId) -> Option<&WorkspaceEntry> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    pub fn ws_by_id_mut(&mut self, id: WorkspaceId) -> Option<&mut WorkspaceEntry> {
        self.workspaces.iter_mut().find(|w| w.id == id)
    }

    pub fn session_by_name(&self, name: &str) -> Option<&SessionState> {
        self.sessions.iter().find(|session| session.name == name)
    }

    pub fn session_by_name_mut(&mut self, name: &str) -> Option<&mut SessionState> {
        self.sessions.iter_mut().find(|session| session.name == name)
    }

    pub fn ensure_session(&mut self, name: &str) -> &mut SessionState {
        if let Some(index) = self.sessions.iter().position(|session| session.name == name) {
            return &mut self.sessions[index];
        }

        self.sessions.push(SessionState {
            name: name.to_string(),
            ..SessionState::default()
        });
        self.sessions
            .last_mut()
            .expect("session list contains newly pushed session")
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let state: Self = serde_yaml::from_str(&content)?;
        Ok(state.migrate_legacy_layout_state())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let contents = serde_yaml::to_string(&self.persistable_state())?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    pub fn migrate_legacy_layout_state(mut self) -> Self {
        let legacy_active_workspace_id = self.active_workspace_id;
        let legacy_active_room_id = self.active_room_id;
        let mut legacy_layouts = Vec::new();

        for workspace in &mut self.workspaces {
            for room in &mut workspace.rooms {
                if !room.tabs.is_empty() {
                    legacy_layouts.push((
                        room.id,
                        PersistedRoomLayout {
                            active_tab: room.active_tab.unwrap_or(0),
                            tabs: std::mem::take(&mut room.tabs),
                        },
                    ));
                }
                room.active_tab = None;
                room.tabs.clear();
            }
        }

        let (active_workspace_id, active_room_id) = {
            let session = self.ensure_session(Self::DEFAULT_SESSION_NAME);
            if session.active_workspace_id.is_none() {
                session.active_workspace_id = legacy_active_workspace_id;
            }
            if session.active_room_id.is_none() {
                session.active_room_id = legacy_active_room_id;
            }
            for (room_id, layout) in legacy_layouts {
                session.tabs_by_room.entry(room_id).or_insert(layout);
            }
            (session.active_workspace_id, session.active_room_id)
        };

        self.active_workspace_id = active_workspace_id;
        self.active_room_id = active_room_id;
        self
    }

    pub fn persistable_state(&self) -> Self {
        let mut state = self.clone().migrate_legacy_layout_state();
        let active_workspace_id = state.active_workspace_id;
        let active_room_id = state.active_room_id;
        let session = state.ensure_session(Self::DEFAULT_SESSION_NAME);
        session.active_workspace_id = active_workspace_id;
        session.active_room_id = active_room_id;
        state
    }
}

// ── Room ID helpers ────────────────────────────────────────────────────────────

/// Creates a new room entry in the workspace. Returns the new `RoomId`,
/// or `None` if `workspace_id` is not found in `state`.
pub fn create_room_for_workspace(
    state: &mut HumuState,
    workspace_id: WorkspaceId,
    room_name: &str,
    room_path: PathBuf,
) -> Option<RoomId> {
    let ws = state.ws_by_id_mut(workspace_id)?;
    let id = RoomId::new();
    ws.rooms.push(RoomEntry {
        name: room_name.to_string(),
        id,
        path: room_path,
        active_tab: None,
        tabs: vec![],
    });
    Some(id)
}

/// Removes room entries whose paths don't match any discovered worktree.
pub fn prune_stale_rooms_for_workspace(
    state: &mut HumuState,
    workspace_id: WorkspaceId,
    discovered_paths: &HashSet<PathBuf>,
) {
    if let Some(ws) = state.ws_by_id_mut(workspace_id) {
        ws.rooms.retain(|r| {
            discovered_paths
                .iter()
                .any(|path| paths_match(&r.path, path))
        });
    }
}

fn paths_match(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }

    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}
