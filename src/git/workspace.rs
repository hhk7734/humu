use crate::config::{HumuState, WorkspaceEntry};
use crate::id::WorkspaceId;
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Default)]
pub struct WorkspaceManager;

impl WorkspaceManager {
    pub fn new() -> Self {
        Self
    }

    /// Register an existing git repo as a workspace.
    pub fn register(&self, state: &mut HumuState, path: &Path) -> Result<WorkspaceId> {
        self.register_with_trust_runner(state, path, trust_mise_file_if_present)
    }

    fn register_with_trust_runner<F>(
        &self,
        state: &mut HumuState,
        path: &Path,
        trust: F,
    ) -> Result<WorkspaceId>
    where
        F: FnOnce(&Path) -> Result<()>,
    {
        let path = std::fs::canonicalize(path)?;
        if !path.join(".git").exists() {
            bail!("not a git repository: {}", path.display());
        }
        if state
            .workspaces
            .iter()
            .any(|workspace| workspace.path == path)
        {
            bail!("workspace path already registered: {}", path.display());
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let id = WorkspaceId::new();
        state.workspaces.push(WorkspaceEntry {
            name,
            id,
            path,
            last_room_id: None,
            rooms: vec![],
        });
        let workspace_path = &state.workspaces.last().unwrap().path;
        if let Err(error) = trust(workspace_path) {
            crate::humu_log!(
                "failed to trust mise config for {}: {error}",
                workspace_path.display()
            );
        }
        Ok(id)
    }

    /// Clone a remote repo and register it.
    pub fn clone_remote(
        &self,
        state: &mut HumuState,
        url: &str,
        target_dir: &Path,
    ) -> Result<WorkspaceId> {
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
    pub fn init(&self, state: &mut HumuState, path: &Path) -> Result<WorkspaceId> {
        if path.join(".git").exists() {
            bail!("directory is already a git repository: {}", path.display());
        }
        std::fs::create_dir_all(path)?;
        let output = Command::new("git").arg("init").arg(path).output()?;
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
        id: WorkspaceId,
        remove_from_disk: bool,
    ) -> Result<()> {
        let idx = state
            .workspaces
            .iter()
            .position(|w| w.id == id)
            .ok_or_else(|| anyhow::anyhow!("workspace not found: {id}"))?;
        let entry = state.workspaces.remove(idx);

        let worktrees_dir = crate::config::humu_dir()
            .join("worktrees")
            .join(entry.id.to_string());
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
}

pub fn default_clone_target_dir(home: &Path, url: &str) -> Result<PathBuf> {
    let repo_path = clone_repo_path(url)?;
    let mut segments = repo_path.split('/').filter(|segment| !segment.is_empty());
    let mut collected: Vec<&str> = segments.by_ref().collect();
    if collected.len() < 2 {
        bail!("could not derive clone path from URL: {url}");
    }

    let repo = collected.pop().unwrap().trim_end_matches(".git");
    let owner = collected.pop().unwrap();
    if owner.is_empty() || repo.is_empty() {
        bail!("could not derive clone path from URL: {url}");
    }

    Ok(home.join(".humu").join("projects").join(owner).join(repo))
}

fn clone_repo_path(url: &str) -> Result<&str> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        bail!("URL is required for Clone");
    }

    if let Some((_, rest)) = trimmed.split_once("://") {
        let (_, path) = rest
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("could not derive clone path from URL: {url}"))?;
        return Ok(path);
    }

    if let Some((_, path)) = trimmed.split_once(':') {
        return Ok(path);
    }

    bail!("could not derive clone path from URL: {url}");
}

pub fn trust_mise_file_if_present(workspace_path: &Path) -> Result<()> {
    trust_mise_file_if_present_with(workspace_path, run_mise_trust)
}

pub fn trust_mise_file_if_present_with<F>(workspace_path: &Path, trust: F) -> Result<()>
where
    F: FnOnce(&Path) -> Result<()>,
{
    let mise_file = workspace_path.join("mise.toml");
    if !mise_file.exists() {
        return Ok(());
    }

    trust(&mise_file)
}

fn run_mise_trust(mise_file: &Path) -> Result<()> {
    let output = Command::new("mise").arg("trust").arg(mise_file).output()?;
    if !output.status.success() {
        bail!(
            "mise trust failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HumuState;
    use tempfile::TempDir;

    #[test]
    fn register_succeeds_even_if_mise_trust_fails() {
        let dir = TempDir::new().unwrap();
        std::process::Command::new("git")
            .args(["init", dir.path().to_str().unwrap()])
            .output()
            .unwrap();
        std::fs::write(dir.path().join("mise.toml"), "tools = {}\n").unwrap();

        let mut state = HumuState::default();
        let mgr = WorkspaceManager::new();

        let ws_id = mgr
            .register_with_trust_runner(&mut state, dir.path(), |_| bail!("boom"))
            .unwrap();

        assert!(state.ws_by_id(ws_id).is_some());
    }
}
