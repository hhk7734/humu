use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub struct RoomInfo {
    pub branch: String,
    pub path: PathBuf,
    pub is_default: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RoomGitStatus {
    pub diff_stat: Option<(usize, usize)>,
    pub untracked_count: usize,
    pub ahead_behind: Option<(usize, usize)>,
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

        let canon_repo = std::fs::canonicalize(repo_path).unwrap_or(repo_path.to_path_buf());

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut current_path: Option<PathBuf> = None;
        let mut current_branch: Option<String> = None;
        let mut current_head: Option<String> = None;

        for line in stdout.lines() {
            if let Some(path) = line.strip_prefix("worktree ") {
                current_path = Some(PathBuf::from(path));
                current_branch = None;
                current_head = None;
            } else if let Some(sha) = line.strip_prefix("HEAD ") {
                current_head = Some(sha.to_string());
            } else if let Some(branch_ref) = line.strip_prefix("branch refs/heads/") {
                current_branch = Some(branch_ref.to_string());
            } else if line.is_empty() {
                // For detached HEAD worktrees, fall back to short SHA
                if current_branch.is_none()
                    && let Some(ref sha) = current_head
                {
                    current_branch = Some(sha[..sha.len().min(7)].to_string());
                }
                if let (Some(path), Some(branch)) = (current_path.take(), current_branch.take()) {
                    // Skip the main worktree (already added as default)
                    let canon_wt = std::fs::canonicalize(&path).unwrap_or(path.clone());
                    if canon_wt != canon_repo {
                        rooms.push(RoomInfo {
                            branch,
                            path,
                            is_default: false,
                        });
                    }
                }
                current_head = None;
            }
        }

        // Handle last entry if no trailing newline
        if current_branch.is_none()
            && let Some(ref sha) = current_head
        {
            current_branch = Some(sha[..sha.len().min(7)].to_string());
        }
        if let (Some(path), Some(branch)) = (current_path, current_branch) {
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

    /// Get git status summary for a worktree path.
    pub fn status(&self, worktree_path: &Path) -> RoomGitStatus {
        RoomGitStatus {
            diff_stat: self.diff_stat(worktree_path),
            untracked_count: self.untracked_count(worktree_path).unwrap_or(0),
            ahead_behind: self.ahead_behind(worktree_path),
        }
    }

    fn diff_stat(&self, worktree_path: &Path) -> Option<(usize, usize)> {
        let output = Command::new("git")
            .args(["diff", "--shortstat"])
            .current_dir(worktree_path)
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        parse_shortstat(&text)
    }

    /// Count untracked, non-ignored files for a worktree path.
    pub fn untracked_count(&self, worktree_path: &Path) -> Option<usize> {
        let output = Command::new("git")
            .args(["ls-files", "--others", "--exclude-standard"])
            .current_dir(worktree_path)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).lines().count())
    }

    /// Get (ahead, behind) commit counts relative to the upstream tracking branch.
    /// Returns None if there's no upstream or an error occurs.
    pub fn ahead_behind(&self, worktree_path: &Path) -> Option<(usize, usize)> {
        let output = Command::new("git")
            .args(["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
            .current_dir(worktree_path)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = text.trim().split('\t').collect();
        if parts.len() == 2 {
            let ahead = parts[0].parse().ok()?;
            let behind = parts[1].parse().ok()?;
            Some((ahead, behind))
        } else {
            None
        }
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
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }

        // Detached HEAD — fall back to short commit hash
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["rev-parse", "--short", "HEAD"])
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            bail!("cannot determine branch for repo: {}", repo_path.display())
        }
    }
}

/// Parse `git diff --shortstat` output like " 3 files changed, 10 insertions(+), 5 deletions(-)"
fn parse_shortstat(text: &str) -> Option<(usize, usize)> {
    if text.trim().is_empty() {
        return Some((0, 0));
    }
    let mut insertions = 0usize;
    let mut deletions = 0usize;
    for part in text.split(',') {
        let part = part.trim();
        if part.contains("insertion") {
            insertions = part.split_whitespace().next()?.parse().ok()?;
        } else if part.contains("deletion") {
            deletions = part.split_whitespace().next()?.parse().ok()?;
        }
    }
    Some((insertions, deletions))
}
