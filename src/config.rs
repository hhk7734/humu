use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::id::{WorkspaceId, RoomId};

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
        Self { enabled: true, only_unfocused: true }
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
        Self { enabled: true, only_unfocused: false }
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
                args: vec![],
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
    pub fn load(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config = serde_yaml::from_str(&contents)?;
        Ok(config)
    }

    /// Load from TOML format (migration from old config.toml).
    pub fn load_toml(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config = toml::from_str(&contents)?;
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

// ── RoomEntry ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomEntry {
    pub name: String,
    pub id: RoomId,
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
    pub fn room_by_name(&self, name: &str) -> Option<&RoomEntry> {
        self.rooms.iter().find(|r| r.name == name)
    }

    pub fn room_by_name_mut(&mut self, name: &str) -> Option<&mut RoomEntry> {
        self.rooms.iter_mut().find(|r| r.name == name)
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
    /// Panel widths: [workspace_panel, room_panel, explorer_panel]. Persisted across restarts.
    #[serde(default, deserialize_with = "deserialize_panel_widths")]
    pub panel_widths: Option<[u16; 3]>,
}

fn deserialize_panel_widths<'de, D>(deserializer: D) -> Result<Option<[u16; 3]>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<Vec<u16>> = Option::deserialize(deserializer)?;
    Ok(opt.map(|v| match v.len() {
        2 => [v[0], v[1], 25],
        3 => [v[0], v[1], v[2]],
        _ => [20, 18, 25],
    }))
}

impl HumuState {
    pub fn ws_by_name(&self, name: &str) -> Option<&WorkspaceEntry> {
        self.workspaces.iter().find(|w| w.name == name)
    }

    pub fn ws_by_name_mut(&mut self, name: &str) -> Option<&mut WorkspaceEntry> {
        self.workspaces.iter_mut().find(|w| w.name == name)
    }

    pub fn ws_by_id(&self, id: WorkspaceId) -> Option<&WorkspaceEntry> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    pub fn ws_by_id_mut(&mut self, id: WorkspaceId) -> Option<&mut WorkspaceEntry> {
        self.workspaces.iter_mut().find(|w| w.id == id)
    }

    pub fn ws_names_sorted(&self) -> Vec<String> {
        let mut names: Vec<_> = self.workspaces.iter().map(|w| w.name.clone()).collect();
        names.sort();
        names
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let state: Self = serde_yaml::from_str(&content)?;
        Ok(state)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let contents = serde_yaml::to_string(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}

// ── Room ID helpers ────────────────────────────────────────────────────────────

/// Returns the `RoomId` for `room_name` within the named workspace, creating a
/// new entry if one does not already exist (lazy assignment).
///
/// Returns `None` if `workspace_name` is not found in `state`.
pub fn ensure_room_id_for_workspace(
    state: &mut HumuState,
    workspace_name: &str,
    room_name: &str,
) -> Option<RoomId> {
    let ws = state.ws_by_name_mut(workspace_name)?;
    if let Some(entry) = ws.room_by_name(room_name) {
        Some(entry.id)
    } else {
        let id = RoomId::new();
        ws.rooms.push(RoomEntry {
            name: room_name.to_string(),
            id,
            active_tab: None,
            tabs: vec![],
        });
        Some(id)
    }
}

/// Removes any room entries from the named workspace whose names are not present
/// in `discovered_rooms`. Silently does nothing if `workspace_name` is not
/// found in `state`.
pub fn prune_stale_rooms_for_workspace(
    state: &mut HumuState,
    workspace_name: &str,
    discovered_rooms: &HashSet<String>,
) {
    if let Some(ws) = state.ws_by_name_mut(workspace_name) {
        ws.rooms.retain(|r| discovered_rooms.contains(&r.name));
    }
}
