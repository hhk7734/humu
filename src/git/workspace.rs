use crate::config::{HumuState, WorkspaceEntry};
use crate::id::WorkspaceId;
use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

#[derive(Default)]
pub struct WorkspaceManager;

impl WorkspaceManager {
    pub fn new() -> Self {
        Self
    }

    /// Register an existing git repo as a workspace.
    pub fn register(&self, state: &mut HumuState, path: &Path) -> Result<String> {
        let path = std::fs::canonicalize(path)?;
        if !path.join(".git").exists() {
            bail!("not a git repository: {}", path.display());
        }
        let name = self.unique_name(state, &path);
        state.workspaces.push(WorkspaceEntry {
            name: name.clone(),
            id: WorkspaceId::new(),
            path,
            rooms: vec![],
        });
        Ok(name)
    }

    /// Clone a remote repo and register it.
    pub fn clone_remote(
        &self,
        state: &mut HumuState,
        url: &str,
        target_dir: &Path,
    ) -> Result<String> {
        let output = Command::new("git")
            .arg("clone")
            .arg(url)
            .arg(target_dir)
            .output()?;
        if !output.status.success() {
            bail!(
                "git clone failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        self.register(state, target_dir)
    }

    /// Initialize a new git repo and register it.
    pub fn init(&self, state: &mut HumuState, path: &Path) -> Result<String> {
        if path.join(".git").exists() {
            bail!("directory is already a git repository: {}", path.display());
        }
        std::fs::create_dir_all(path)?;
        let output = Command::new("git")
            .arg("init")
            .arg(path)
            .output()?;
        if !output.status.success() {
            bail!(
                "git init failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        self.register(state, path)
    }

    /// Delete a workspace. Optionally remove the repo from disk.
    pub fn delete(
        &self,
        state: &mut HumuState,
        name: &str,
        remove_from_disk: bool,
    ) -> Result<()> {
        let idx = state
            .workspaces
            .iter()
            .position(|w| w.name == name)
            .ok_or_else(|| anyhow::anyhow!("workspace not found: {name}"))?;
        let entry = state.workspaces.remove(idx);

        let worktrees_dir = crate::config::humu_dir()
            .join("worktrees")
            .join(name);
        if worktrees_dir.exists() {
            std::fs::remove_dir_all(&worktrees_dir)?;
        }

        if remove_from_disk && entry.path.exists() {
            std::fs::remove_dir_all(&entry.path)?;
        }

        // Only clear active IDs if the deleted workspace was the active one
        if state.active_workspace_id == Some(entry.id) {
            state.active_workspace_id = None;
            state.active_room_id = None;
        }

        Ok(())
    }

    /// List all workspace names.
    pub fn list(&self, state: &HumuState) -> Vec<String> {
        state.ws_names_sorted()
    }

    /// Derive a unique workspace name from the directory name.
    fn unique_name(&self, state: &HumuState, path: &Path) -> String {
        let base = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        if state.ws_by_name(&base).is_none() {
            return base;
        }

        let mut suffix = 2;
        loop {
            let candidate = format!("{base}-{suffix}");
            if state.ws_by_name(&candidate).is_none() {
                return candidate;
            }
            suffix += 1;
        }
    }
}
