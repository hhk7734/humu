use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::id::{WorkspaceId, RoomId};

// ── Directory helper ──────────────────────────────────────────────────────────

/// Returns `~/.humu/`, creating it if it does not exist.
pub fn humu_dir() -> PathBuf {
    let dir = dirs::home_dir()
        .expect("cannot determine home directory")
        .join(".humu");
    std::fs::create_dir_all(&dir).expect("cannot create ~/.humu");
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

// ── HumuConfig ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumuConfig {
    #[serde(default)]
    pub presets: HashMap<String, Preset>,
    #[serde(default)]
    pub ui: UiSection,
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
        }
    }
}

impl HumuConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config = toml::from_str(&contents)?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let contents = toml::to_string_pretty(self)?;
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomLayout {
    pub active_tab: usize,
    pub tabs: Vec<TabLayout>,
}

// ── WorkspaceEntry / RoomEntry ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceEntry {
    pub id: WorkspaceId,
    pub path: PathBuf,
    #[serde(default)]
    pub rooms: HashMap<String, RoomEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomEntry {
    pub id: RoomId,
}

// ── HumuState ─────────────────────────────────────────────────────────────────

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

impl HumuState {
    pub fn load(path: &Path) -> Result<Self> {
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

    pub fn save(&self, path: &Path) -> Result<()> {
        let contents = toml::to_string_pretty(self)?;
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
    let ws = state.workspaces.get_mut(workspace_name)?;
    if let Some(entry) = ws.rooms.get(room_name) {
        Some(entry.id)
    } else {
        let id = RoomId::new();
        ws.rooms.insert(room_name.to_string(), RoomEntry { id });
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
    if let Some(ws) = state.workspaces.get_mut(workspace_name) {
        ws.rooms.retain(|name, _| discovered_rooms.contains(name));
    }
}
