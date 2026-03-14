use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

// ── WorkspaceEntry ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceEntry {
    pub path: PathBuf,
}

// ── HumuState ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HumuState {
    pub active_workspace: Option<String>,
    pub active_room: Option<String>,
    #[serde(default)]
    pub workspaces: HashMap<String, WorkspaceEntry>,
    /// layout[workspace][room] = RoomLayout
    #[serde(default)]
    pub layout: HashMap<String, HashMap<String, RoomLayout>>,
}

impl HumuState {
    pub fn load(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let state = toml::from_str(&contents)?;
        Ok(state)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}
