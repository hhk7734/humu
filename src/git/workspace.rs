use crate::config::{HumuState, WorkspaceEntry};
use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

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
        state.workspaces.insert(
            name.clone(),
            WorkspaceEntry { path },
        );
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
            .args(["clone", url, target_dir.to_str().unwrap()])
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
        std::fs::create_dir_all(path)?;
        let output = Command::new("git")
            .args(["init", path.to_str().unwrap()])
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
        let entry = state
            .workspaces
            .remove(name)
            .ok_or_else(|| anyhow::anyhow!("workspace not found: {name}"))?;

        // Remove worktrees directory
        let worktrees_dir = crate::config::humu_dir()
            .join("worktrees")
            .join(name);
        if worktrees_dir.exists() {
            std::fs::remove_dir_all(&worktrees_dir)?;
        }

        // Remove layout state
        state.layout.remove(name);

        if remove_from_disk {
            if entry.path.exists() {
                std::fs::remove_dir_all(&entry.path)?;
            }
        }

        if state.active_workspace.as_deref() == Some(name) {
            state.active_workspace = None;
            state.active_room = None;
        }

        Ok(())
    }

    /// List all workspace names.
    pub fn list(&self, state: &HumuState) -> Vec<String> {
        let mut names: Vec<_> = state.workspaces.keys().cloned().collect();
        names.sort();
        names
    }

    /// Derive a unique workspace name from the directory name.
    fn unique_name(&self, state: &HumuState, path: &Path) -> String {
        let base = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        if !state.workspaces.contains_key(&base) {
            return base;
        }

        let mut suffix = 2;
        loop {
            let candidate = format!("{base}-{suffix}");
            if !state.workspaces.contains_key(&candidate) {
                return candidate;
            }
            suffix += 1;
        }
    }
}
