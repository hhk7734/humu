use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub struct RoomInfo {
    pub branch: String,
    pub path: PathBuf,
    pub is_default: bool,
}

#[derive(Default)]
pub struct RoomManager;

impl RoomManager {
    pub fn new() -> Self {
        Self
    }

    /// List all rooms (default + worktrees) for a repo.
    pub fn list(&self, repo_path: &Path) -> Result<Vec<RoomInfo>> {
        let mut rooms = Vec::new();

        // Default room: repo's current branch
        let branch = self.current_branch(repo_path)?;
        rooms.push(RoomInfo {
            branch,
            path: repo_path.to_path_buf(),
            is_default: true,
        });

        // Additional rooms from worktrees
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["worktree", "list", "--porcelain"])
            .output()?;
        if !output.status.success() {
            bail!("git worktree list failed");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut current_path: Option<PathBuf> = None;
        let mut current_branch: Option<String> = None;

        for line in stdout.lines() {
            if let Some(path) = line.strip_prefix("worktree ") {
                current_path = Some(PathBuf::from(path));
                current_branch = None;
            } else if let Some(branch_ref) = line.strip_prefix("branch refs/heads/") {
                current_branch = Some(branch_ref.to_string());
            } else if line.is_empty() {
                if let (Some(path), Some(branch)) = (current_path.take(), current_branch.take()) {
                    // Skip the main worktree (already added as default)
                    let canon_repo =
                        std::fs::canonicalize(repo_path).unwrap_or(repo_path.to_path_buf());
                    let canon_wt = std::fs::canonicalize(&path).unwrap_or(path.clone());
                    if canon_wt != canon_repo {
                        rooms.push(RoomInfo {
                            branch,
                            path,
                            is_default: false,
                        });
                    }
                }
                current_path = None;
                current_branch = None;
            }
        }

        // Handle last entry if no trailing newline
        if let (Some(path), Some(branch)) = (current_path, current_branch) {
            let canon_repo =
                std::fs::canonicalize(repo_path).unwrap_or(repo_path.to_path_buf());
            let canon_wt = std::fs::canonicalize(&path).unwrap_or(path.clone());
            if canon_wt != canon_repo {
                rooms.push(RoomInfo {
                    branch,
                    path,
                    is_default: false,
                });
            }
        }

        Ok(rooms)
    }

    /// Create a new room (worktree) branching from base_branch.
    pub fn create(
        &self,
        repo_path: &Path,
        branch: &str,
        base_branch: &str,
        worktree_path: &Path,
    ) -> Result<()> {
        if let Some(parent) = worktree_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let output = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["worktree", "add", "-b", branch])
            .arg(worktree_path)
            .arg(base_branch)
            .output()?;

        if !output.status.success() {
            bail!(
                "git worktree add failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }

    /// Delete a room: remove worktree, then delete the local branch.
    pub fn delete(
        &self,
        repo_path: &Path,
        branch: &str,
        worktree_path: &Path,
    ) -> Result<()> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["worktree", "remove"])
            .arg(worktree_path)
            .output()?;

        if !output.status.success() {
            bail!(
                "git worktree remove failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let output = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["branch", "-D", branch])
            .output()?;

        if !output.status.success() {
            bail!(
                "git branch -D failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }

    fn current_branch(&self, repo_path: &Path) -> Result<String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["symbolic-ref", "--short", "HEAD"])
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            // Detached HEAD — fall back to short commit hash
            let output = Command::new("git")
                .arg("-C")
                .arg(repo_path)
                .args(["rev-parse", "--short", "HEAD"])
                .output()?;
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
    }
}
