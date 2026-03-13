# Humu Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a TUI-based multi-task manager where workspaces map to git repos, rooms map to worktrees, and terminal panes run presets (Claude Code, shell) within each room.

**Architecture:** Single Rust binary using ratatui for TUI, portable-pty for PTY spawning, and vt100 for terminal emulation. Zellij-style modal keybindings. Claude Code hook events received via Unix socket.

**Tech Stack:** Rust 1.93, ratatui, crossterm, portable-pty, vt100, serde, toml, tokio (for async Unix socket)

**Spec:** `docs/specs/2026-03-14-humu-design.md`

---

## File Structure

```
Cargo.toml
src/
├── main.rs                          # Entry point, init config dir, run app
├── app.rs                           # App state, main event loop
├── config.rs                        # Config + State TOML parsing/writing
├── preset.rs                        # Preset definition, env var expansion
├── git/
│   ├── mod.rs                       # Re-exports
│   ├── workspace.rs                 # Workspace CRUD (clone/init/register/delete/list)
│   └── room.rs                      # Room CRUD (worktree add/remove/list)
├── pty/
│   ├── mod.rs                       # Re-exports
│   └── pane.rs                      # PTY spawning, read/write, vt100 screen buffer
├── tui/
│   ├── mod.rs                       # Re-exports
│   ├── input.rs                     # Modal input handling (modes enum, key dispatch)
│   ├── layout.rs                    # Split tree data structure, tab container
│   └── widgets/
│       ├── mod.rs                   # Re-exports
│       ├── workspace_panel.rs       # Workspace list widget
│       ├── room_panel.rs            # Room list widget
│       ├── terminal_area.rs         # Tab bar + split pane container
│       ├── terminal_widget.rs       # Single pane: vt100 screen → ratatui cells
│       ├── status_bar.rs            # Mode-aware keybinding hints
│       ├── preset_selector.rs       # Popup preset picker
│       └── dialog.rs                # Create/delete confirmation dialogs
└── hook/
    ├── mod.rs                       # Re-exports
    └── server.rs                    # Unix socket server, event parsing, spinner state
tests/
├── config_test.rs
├── git_test.rs
├── pty_test.rs
├── layout_test.rs
└── hook_test.rs
```

---

## Chunk 1: Foundation

### Task 1: Project Scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`

- [ ] **Step 1: Initialize Cargo project**

```bash
cd /home/hhk7734/github/hhk7734/humu
cargo init --name humu
```

- [ ] **Step 2: Add dependencies to Cargo.toml**

```toml
[package]
name = "humu"
version = "0.1.0"
edition = "2024"

[dependencies]
ratatui = "0.29"
crossterm = "0.28"
portable-pty = "0.8"
vt100 = "0.15"
serde = { version = "1", features = ["derive"] }
toml = "0.8"
tokio = { version = "1", features = ["full"] }
dirs = "6"
anyhow = "1"
serde_json = "1"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Write minimal main.rs**

```rust
use anyhow::Result;

fn main() -> Result<()> {
    println!("humu v{}", env!("CARGO_PKG_VERSION"));
    Ok(())
}
```

- [ ] **Step 4: Verify it builds and runs**

Run: `cargo run`
Expected: `humu v0.1.0`

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/main.rs
git commit -m "feat: scaffold humu Rust project with dependencies"
```

---

### Task 2: Config Parsing

**Files:**
- Create: `src/config.rs`
- Create: `tests/config_test.rs`

- [ ] **Step 1: Write failing test for config parsing**

`tests/config_test.rs`:

```rust
use humu::config::{HumuConfig, Preset};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_parse_default_config() {
    let config = HumuConfig::default();
    assert!(config.presets.contains_key("claude"));
    assert!(config.presets.contains_key("shell"));
    assert_eq!(config.presets["claude"].command, "claude");
}

#[test]
fn test_parse_config_from_toml() {
    let toml = r#"
[presets.claude]
command = "claude"
args = []

[presets.shell]
command = "$SHELL"
args = []

[presets.cargo-watch]
command = "cargo"
args = ["watch", "-x", "test"]
"#;
    let config: HumuConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.presets.len(), 3);
    assert_eq!(config.presets["cargo-watch"].command, "cargo");
    assert_eq!(config.presets["cargo-watch"].args, vec!["watch", "-x", "test"]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test config_test`
Expected: FAIL — module `config` not found

- [ ] **Step 3: Write config module**

`src/config.rs`:

```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct Preset {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HumuConfig {
    #[serde(default = "default_presets")]
    pub presets: HashMap<String, Preset>,
}

fn default_presets() -> HashMap<String, Preset> {
    let mut presets = HashMap::new();
    presets.insert(
        "claude".into(),
        Preset {
            command: "claude".into(),
            args: vec![],
        },
    );
    presets.insert(
        "shell".into(),
        Preset {
            command: "$SHELL".into(),
            args: vec![],
        },
    );
    presets
}

impl Default for HumuConfig {
    fn default() -> Self {
        Self {
            presets: default_presets(),
        }
    }
}

impl HumuConfig {
    pub fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

/// Returns the humu config directory: ~/.humu/
pub fn humu_dir() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".humu")
}
```

- [ ] **Step 4: Write failing test for state parsing**

Add to `tests/config_test.rs`:

```rust
use humu::config::HumuState;

#[test]
fn test_state_round_trip() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.toml");

    let mut state = HumuState::default();
    state.active_workspace = Some("humu".into());
    state.active_room = Some("feat/auth".into());
    state.workspaces.insert(
        "humu".into(),
        humu::config::WorkspaceEntry {
            path: PathBuf::from("/home/user/github/humu"),
        },
    );

    state.save(&path).unwrap();
    let loaded = HumuState::load(&path).unwrap();
    assert_eq!(loaded.active_workspace, Some("humu".into()));
    assert_eq!(loaded.workspaces["humu"].path, PathBuf::from("/home/user/github/humu"));
}
```

- [ ] **Step 5: Implement state parsing**

Add to `src/config.rs`:

```rust
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct HumuState {
    pub active_workspace: Option<String>,
    pub active_room: Option<String>,
    #[serde(default)]
    pub workspaces: HashMap<String, WorkspaceEntry>,
    #[serde(default)]
    pub layout: HashMap<String, HashMap<String, RoomLayout>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    pub path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RoomLayout {
    pub active_tab: usize,
    pub tabs: Vec<TabLayout>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TabLayout {
    pub name: String,
    pub split: SplitNode,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Vertical,
    Horizontal,
}

impl HumuState {
    pub fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
```

- [ ] **Step 6: Export from lib.rs**

Create `src/lib.rs`:

```rust
pub mod config;
```

- [ ] **Step 7: Run all tests**

Run: `cargo test --test config_test`
Expected: All tests PASS

- [ ] **Step 8: Commit**

```bash
git add src/config.rs src/lib.rs tests/config_test.rs
git commit -m "feat: add config and state TOML parsing"
```

---

### Task 3: Preset Expansion

**Files:**
- Create: `src/preset.rs`
- Create or modify: `tests/config_test.rs`

- [ ] **Step 1: Write failing test for env var expansion**

Add to `tests/config_test.rs`:

```rust
use humu::preset::expand_env;

#[test]
fn test_expand_env_shell() {
    std::env::set_var("TEST_HUMU_VAR", "/bin/zsh");
    assert_eq!(expand_env("$TEST_HUMU_VAR"), "/bin/zsh");
    assert_eq!(expand_env("literal"), "literal");
    assert_eq!(expand_env("$NONEXISTENT_HUMU_VAR_12345"), "");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test config_test test_expand_env`
Expected: FAIL

- [ ] **Step 3: Implement preset module**

`src/preset.rs`:

```rust
use std::env;

/// Expand environment variables in a string.
/// Supports `$VAR` syntax. Unknown variables expand to empty string.
pub fn expand_env(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' {
            let mut var_name = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_alphanumeric() || next == '_' {
                    var_name.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            if var_name.is_empty() {
                result.push('$');
            } else {
                result.push_str(&env::var(&var_name).unwrap_or_default());
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Expand env vars in command and args, returning the resolved command line.
pub fn resolve_preset(command: &str, args: &[String]) -> (String, Vec<String>) {
    (
        expand_env(command),
        args.iter().map(|a| expand_env(a)).collect(),
    )
}
```

- [ ] **Step 4: Export and run tests**

Add to `src/lib.rs`:

```rust
pub mod preset;
```

Run: `cargo test --test config_test test_expand_env`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/preset.rs src/lib.rs tests/config_test.rs
git commit -m "feat: add preset env var expansion"
```

---

### Task 4: Git Workspace Manager

**Files:**
- Create: `src/git/mod.rs`
- Create: `src/git/workspace.rs`
- Create: `tests/git_test.rs`

- [ ] **Step 1: Write failing tests for workspace operations**

`tests/git_test.rs`:

```rust
use humu::config::{HumuState, WorkspaceEntry};
use humu::git::workspace::WorkspaceManager;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_register_existing_repo() {
    let dir = TempDir::new().unwrap();
    // Init a git repo in the temp dir
    std::process::Command::new("git")
        .args(["init", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    let mut state = HumuState::default();
    let mgr = WorkspaceManager::new();
    let name = mgr.register(&mut state, dir.path()).unwrap();

    assert!(state.workspaces.contains_key(&name));
    assert_eq!(state.workspaces[&name].path, dir.path());
}

#[test]
fn test_register_non_git_dir_fails() {
    let dir = TempDir::new().unwrap();
    let mut state = HumuState::default();
    let mgr = WorkspaceManager::new();
    let result = mgr.register(&mut state, dir.path());
    assert!(result.is_err());
}

#[test]
fn test_init_new_project() {
    let dir = TempDir::new().unwrap();
    let project_path = dir.path().join("my-project");

    let mut state = HumuState::default();
    let mgr = WorkspaceManager::new();
    let name = mgr.init(&mut state, &project_path).unwrap();

    assert_eq!(name, "my-project");
    assert!(project_path.join(".git").exists());
    assert!(state.workspaces.contains_key("my-project"));
}

#[test]
fn test_name_collision_appends_suffix() {
    let dir = TempDir::new().unwrap();
    let repo1 = dir.path().join("a/infra");
    let repo2 = dir.path().join("b/infra");

    let mut state = HumuState::default();
    let mgr = WorkspaceManager::new();

    mgr.init(&mut state, &repo1).unwrap();
    let name2 = mgr.init(&mut state, &repo2).unwrap();

    assert_eq!(name2, "infra-2");
}

#[test]
fn test_delete_workspace_keeps_repo() {
    let dir = TempDir::new().unwrap();
    let mut state = HumuState::default();
    let mgr = WorkspaceManager::new();
    let name = mgr.init(&mut state, dir.path().join("proj")).unwrap();

    mgr.delete(&mut state, &name, false).unwrap();

    assert!(!state.workspaces.contains_key(&name));
    assert!(dir.path().join("proj").exists()); // repo kept on disk
}

#[test]
fn test_delete_workspace_removes_repo() {
    let dir = TempDir::new().unwrap();
    let project_path = dir.path().join("proj");
    let mut state = HumuState::default();
    let mgr = WorkspaceManager::new();
    let name = mgr.init(&mut state, &project_path).unwrap();

    mgr.delete(&mut state, &name, true).unwrap();

    assert!(!state.workspaces.contains_key(&name));
    assert!(!project_path.exists()); // repo deleted
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test git_test`
Expected: FAIL — module not found

- [ ] **Step 3: Implement workspace manager**

`src/git/mod.rs`:

```rust
pub mod workspace;
pub mod room;
```

`src/git/workspace.rs`:

```rust
use crate::config::{HumuState, WorkspaceEntry};
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
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
```

- [ ] **Step 4: Export git module from lib.rs**

Add to `src/lib.rs`:

```rust
pub mod git;
```

- [ ] **Step 5: Run tests**

Run: `cargo test --test git_test`
Expected: All tests PASS

- [ ] **Step 6: Commit**

```bash
git add src/git/ tests/git_test.rs src/lib.rs
git commit -m "feat: add git workspace manager with CRUD operations"
```

---

### Task 5: Git Room Manager

**Files:**
- Create: `src/git/room.rs`
- Modify: `tests/git_test.rs`

- [ ] **Step 1: Write failing tests for room operations**

Add to `tests/git_test.rs`:

```rust
use humu::git::room::RoomManager;

#[test]
fn test_list_rooms_default_only() {
    let dir = TempDir::new().unwrap();
    std::process::Command::new("git")
        .args(["init", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    // Create initial commit so branch exists
    std::process::Command::new("git")
        .args(["-C", dir.path().to_str().unwrap(), "commit", "--allow-empty", "-m", "init"])
        .output()
        .unwrap();

    let mgr = RoomManager::new();
    let rooms = mgr.list(dir.path()).unwrap();

    assert_eq!(rooms.len(), 1);
    assert!(rooms[0].is_default);
}

#[test]
fn test_create_and_list_room() {
    let dir = TempDir::new().unwrap();
    let repo = dir.path().join("repo");
    std::process::Command::new("git")
        .args(["init", repo.to_str().unwrap()])
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["-C", repo.to_str().unwrap(), "commit", "--allow-empty", "-m", "init"])
        .output()
        .unwrap();

    let worktree_base = dir.path().join("worktrees");
    let mgr = RoomManager::new();
    mgr.create(&repo, "feat/auth", "main", &worktree_base.join("repo").join("feat/auth"))
        .unwrap();

    let rooms = mgr.list(&repo).unwrap();
    assert_eq!(rooms.len(), 2);
    assert!(rooms.iter().any(|r| r.branch == "feat/auth" && !r.is_default));
}

#[test]
fn test_delete_room() {
    let dir = TempDir::new().unwrap();
    let repo = dir.path().join("repo");
    std::process::Command::new("git")
        .args(["init", repo.to_str().unwrap()])
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["-C", repo.to_str().unwrap(), "commit", "--allow-empty", "-m", "init"])
        .output()
        .unwrap();

    let wt_path = dir.path().join("worktrees/repo/feat-x");
    let mgr = RoomManager::new();
    mgr.create(&repo, "feat-x", "main", &wt_path).unwrap();
    assert!(wt_path.exists());

    mgr.delete(&repo, "feat-x", &wt_path).unwrap();
    assert!(!wt_path.exists());

    let rooms = mgr.list(&repo).unwrap();
    assert_eq!(rooms.len(), 1); // only default remains
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test git_test test_list_rooms`
Expected: FAIL

- [ ] **Step 3: Implement room manager**

`src/git/room.rs`:

```rust
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub struct RoomInfo {
    pub branch: String,
    pub path: PathBuf,
    pub is_default: bool,
}

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
            .args(["-C", repo_path.to_str().unwrap(), "worktree", "list", "--porcelain"])
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
                    let canon_repo = std::fs::canonicalize(repo_path).unwrap_or(repo_path.to_path_buf());
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
            let canon_repo = std::fs::canonicalize(repo_path).unwrap_or(repo_path.to_path_buf());
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
            .args([
                "-C",
                repo_path.to_str().unwrap(),
                "worktree",
                "add",
                "-b",
                branch,
                worktree_path.to_str().unwrap(),
                base_branch,
            ])
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
            .args([
                "-C",
                repo_path.to_str().unwrap(),
                "worktree",
                "remove",
                worktree_path.to_str().unwrap(),
            ])
            .output()?;

        if !output.status.success() {
            bail!(
                "git worktree remove failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let output = Command::new("git")
            .args([
                "-C",
                repo_path.to_str().unwrap(),
                "branch",
                "-D",
                branch,
            ])
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
            .args(["-C", repo_path.to_str().unwrap(), "symbolic-ref", "--short", "HEAD"])
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            // Detached HEAD — fall back to short commit hash
            let output = Command::new("git")
                .args(["-C", repo_path.to_str().unwrap(), "rev-parse", "--short", "HEAD"])
                .output()?;
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --test git_test`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/git/room.rs tests/git_test.rs
git commit -m "feat: add git room manager with worktree CRUD"
```

---

## Chunk 2: PTY & Terminal Rendering

### Task 6: PTY Pane Manager

**Files:**
- Create: `src/pty/mod.rs`
- Create: `src/pty/pane.rs`
- Create: `tests/pty_test.rs`

- [ ] **Step 1: Write failing test for PTY spawning**

`tests/pty_test.rs`:

```rust
use humu::pty::pane::PtyPane;
use std::io::Read;
use std::time::Duration;

#[test]
fn test_spawn_and_read_output() {
    let mut pane = PtyPane::spawn("echo", &["hello".into()], None, 80, 24).unwrap();

    // Give it time to produce output
    std::thread::sleep(Duration::from_millis(500));
    pane.process_output().unwrap();

    let screen = pane.screen();
    let first_line = screen.rows_formatted(0, 80).next().unwrap_or_default();
    // The output should contain "hello" somewhere
    assert!(
        screen.contents().contains("hello"),
        "screen contents: {:?}",
        screen.contents()
    );
}

#[test]
fn test_pane_detects_exit() {
    let mut pane = PtyPane::spawn("true", &[], None, 80, 24).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    pane.process_output().unwrap();

    assert!(pane.exit_status().is_some());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test pty_test`
Expected: FAIL

- [ ] **Step 3: Implement PTY pane**

`src/pty/mod.rs`:

```rust
pub mod pane;
```

`src/pty/pane.rs`:

```rust
use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct PtyPane {
    master: Box<dyn MasterPty + Send>,
    reader: Box<dyn Read + Send>,
    parser: Arc<Mutex<vt100::Parser>>,
    child: Box<dyn portable_pty::Child + Send>,
    exit_code: Option<i32>,
    cols: u16,
    rows: u16,
}

impl PtyPane {
    pub fn spawn(
        command: &str,
        args: &[String],
        cwd: Option<&Path>,
        cols: u16,
        rows: u16,
    ) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(command);
        cmd.args(args);
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }

        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave); // Close slave side

        let reader = pair.master.try_clone_reader()?;
        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 0)));

        Ok(Self {
            master: pair.master,
            reader,
            parser,
            child,
            exit_code: None,
            cols,
            rows,
        })
    }

    /// Read available PTY output and feed it to the vt100 parser.
    pub fn process_output(&mut self) -> Result<()> {
        let mut buf = [0u8; 4096];
        // Non-blocking read: try to read what's available
        loop {
            match self.reader.read(&mut buf) {
                Ok(0) => {
                    // EOF — process exited
                    self.check_exit();
                    break;
                }
                Ok(n) => {
                    self.parser.lock().unwrap().process(&buf[..n]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    // Broken pipe or similar — process likely exited
                    self.check_exit();
                    break;
                }
            }
        }
        Ok(())
    }

    /// Write input to the PTY (user keystrokes).
    pub fn write_input(&mut self, data: &[u8]) -> Result<()> {
        use std::io::Write;
        self.master.write_all(data)?;
        Ok(())
    }

    /// Resize the PTY and vt100 parser.
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.cols = cols;
        self.rows = rows;
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        self.parser.lock().unwrap().set_size(rows, cols);
        Ok(())
    }

    /// Get a snapshot of the terminal screen.
    pub fn screen(&self) -> vt100::Screen {
        self.parser.lock().unwrap().screen().clone()
    }

    /// Get exit status if the process has exited.
    pub fn exit_status(&mut self) -> Option<i32> {
        self.check_exit();
        self.exit_code
    }

    fn check_exit(&mut self) {
        if self.exit_code.is_some() {
            return;
        }
        if let Ok(Some(status)) = self.child.try_wait() {
            self.exit_code = status.exit_code().map(|c| c as i32);
        }
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }
}
```

- [ ] **Step 4: Export from lib.rs**

Add to `src/lib.rs`:

```rust
pub mod pty;
```

- [ ] **Step 5: Run tests**

Run: `cargo test --test pty_test`
Expected: All tests PASS

- [ ] **Step 6: Commit**

```bash
git add src/pty/ tests/pty_test.rs src/lib.rs
git commit -m "feat: add PTY pane spawning with vt100 terminal emulation"
```

---

### Task 7: Terminal Widget (vt100 → ratatui)

**Files:**
- Create: `src/tui/mod.rs`
- Create: `src/tui/widgets/mod.rs`
- Create: `src/tui/widgets/terminal_widget.rs`

- [ ] **Step 1: Implement terminal widget**

This widget converts a vt100 screen buffer into ratatui cells. No test file needed — this is a rendering widget best validated visually.

`src/tui/mod.rs`:

```rust
pub mod widgets;
pub mod input;
pub mod layout;
```

`src/tui/widgets/mod.rs`:

```rust
pub mod terminal_widget;
pub mod status_bar;
pub mod workspace_panel;
pub mod room_panel;
pub mod terminal_area;
pub mod preset_selector;
pub mod dialog;
```

`src/tui/widgets/terminal_widget.rs`:

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;
use vt100::Screen;

pub struct TerminalWidget<'a> {
    screen: &'a Screen,
    has_focus: bool,
    exited: Option<i32>, // exit code if process ended
}

impl<'a> TerminalWidget<'a> {
    pub fn new(screen: &'a Screen) -> Self {
        Self {
            screen,
            has_focus: false,
            exited: None,
        }
    }

    pub fn focus(mut self, focused: bool) -> Self {
        self.has_focus = focused;
        self
    }

    pub fn exited(mut self, exit_code: Option<i32>) -> Self {
        self.exited = exit_code;
        self
    }
}

impl Widget for TerminalWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let rows = area.height.min(self.screen.size().0);
        let cols = area.width.min(self.screen.size().1);

        for row in 0..rows {
            for col in 0..cols {
                let cell = self.screen.cell(row, col);
                if let Some(cell) = cell {
                    let x = area.x + col;
                    let y = area.y + row;

                    if x < area.right() && y < area.bottom() {
                        let fg = convert_color(cell.fgcolor());
                        let bg = convert_color(cell.bgcolor());
                        let mut style = Style::default().fg(fg).bg(bg);

                        if cell.bold() {
                            style = style.add_modifier(Modifier::BOLD);
                        }
                        if cell.italic() {
                            style = style.add_modifier(Modifier::ITALIC);
                        }
                        if cell.underline() {
                            style = style.add_modifier(Modifier::UNDERLINED);
                        }
                        if cell.inverse() {
                            style = Style::default().fg(bg).bg(fg);
                        }

                        let ch = cell.contents();
                        let display_char = if ch.is_empty() { " " } else { &ch };
                        buf.set_string(x, y, display_char, style);
                    }
                }
            }
        }

        // Show exit status overlay if process exited
        if let Some(code) = self.exited {
            let msg = format!(" [exited: {code}] Press Enter to restart ");
            let msg_len = msg.len() as u16;
            if area.width >= msg_len && area.height > 0 {
                let x = area.x + (area.width - msg_len) / 2;
                let y = area.y + area.height / 2;
                let style = Style::default()
                    .fg(Color::Black)
                    .bg(if code == 0 { Color::Green } else { Color::Red });
                buf.set_string(x, y, &msg, style);
            }
        }
    }
}

fn convert_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: No errors (widgets that aren't yet implemented can be empty files)

- [ ] **Step 3: Create stub files for other widgets**

Create each remaining widget file with a placeholder:

`src/tui/widgets/status_bar.rs`, `src/tui/widgets/workspace_panel.rs`, `src/tui/widgets/room_panel.rs`, `src/tui/widgets/terminal_area.rs`, `src/tui/widgets/preset_selector.rs`, `src/tui/widgets/dialog.rs`:

```rust
// TODO: implement in later task
```

`src/tui/input.rs`:

```rust
// TODO: implement in later task
```

`src/tui/layout.rs`:

```rust
// TODO: implement in later task
```

- [ ] **Step 4: Export from lib.rs**

Add to `src/lib.rs`:

```rust
pub mod tui;
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check`
Expected: No errors

- [ ] **Step 6: Commit**

```bash
git add src/tui/ src/lib.rs
git commit -m "feat: add terminal widget rendering vt100 screen to ratatui cells"
```

---

## Chunk 3: TUI Application Shell

### Task 8: Input Mode System

**Files:**
- Modify: `src/tui/input.rs`

- [ ] **Step 1: Implement modal input handling**

`src/tui/input.rs`:

```rust
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Locked,
    Pane,
    Tab,
    Workspace,
    Resize,
}

#[derive(Debug, Clone)]
pub enum Action {
    // Mode transitions
    EnterMode(Mode),
    ExitToNormal,

    // Pane actions
    NewPane,
    SplitDown,
    SplitRight,
    ClosePane,
    MoveFocus(Direction),
    ToggleFullscreen,

    // Tab actions
    NewTab,
    CloseTab,
    PrevTab,
    NextTab,
    GoToTab(usize),
    RenameTab,

    // Workspace actions
    FocusWorkspacePanel,
    FocusRoomPanel,
    NavigateUp,
    NavigateDown,
    Select,
    Create,
    Delete,

    // Resize actions
    Resize(Direction),
    ResizeReverse(Direction),

    // Terminal input
    PassThrough(KeyEvent),

    // No-op
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Down,
    Up,
    Right,
}

pub fn handle_key(mode: Mode, key: KeyEvent) -> Action {
    match mode {
        Mode::Locked => handle_locked(key),
        Mode::Normal => handle_normal(key),
        Mode::Pane => handle_pane(key),
        Mode::Tab => handle_tab(key),
        Mode::Workspace => handle_workspace(key),
        Mode::Resize => handle_resize(key),
    }
}

fn handle_locked(key: KeyEvent) -> Action {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('g') {
        Action::EnterMode(Mode::Normal)
    } else {
        Action::PassThrough(key)
    }
}

fn handle_normal(key: KeyEvent) -> Action {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('g') => Action::EnterMode(Mode::Locked),
            KeyCode::Char('p') => Action::EnterMode(Mode::Pane),
            KeyCode::Char('t') => Action::EnterMode(Mode::Tab),
            KeyCode::Char('w') => Action::EnterMode(Mode::Workspace),
            KeyCode::Char('n') => Action::EnterMode(Mode::Resize),
            _ => Action::PassThrough(key),
        }
    } else if key.modifiers.contains(KeyModifiers::ALT) {
        match key.code {
            KeyCode::Char('h') | KeyCode::Left => Action::MoveFocus(Direction::Left),
            KeyCode::Char('l') | KeyCode::Right => Action::MoveFocus(Direction::Right),
            KeyCode::Char('j') | KeyCode::Down => Action::NavigateDown,
            KeyCode::Char('k') | KeyCode::Up => Action::NavigateUp,
            _ => Action::PassThrough(key),
        }
    } else {
        Action::PassThrough(key)
    }
}

fn handle_pane(key: KeyEvent) -> Action {
    // Check shared Alt bindings first
    if let Some(action) = check_shared_alt(key) {
        return action;
    }

    match key.code {
        KeyCode::Char('n') => Action::NewPane,
        KeyCode::Char('d') => Action::SplitDown,
        KeyCode::Char('r') => Action::SplitRight,
        KeyCode::Char('x') => Action::ClosePane,
        KeyCode::Char('h') | KeyCode::Left => Action::MoveFocus(Direction::Left),
        KeyCode::Char('j') | KeyCode::Down => Action::MoveFocus(Direction::Down),
        KeyCode::Char('k') | KeyCode::Up => Action::MoveFocus(Direction::Up),
        KeyCode::Char('l') | KeyCode::Right => Action::MoveFocus(Direction::Right),
        KeyCode::Char('f') => Action::ToggleFullscreen,
        KeyCode::Esc => Action::ExitToNormal,
        _ if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('p') => {
            Action::ExitToNormal
        }
        _ => Action::None,
    }
}

fn handle_tab(key: KeyEvent) -> Action {
    if let Some(action) = check_shared_alt(key) {
        return action;
    }

    match key.code {
        KeyCode::Char('n') => Action::NewTab,
        KeyCode::Char('x') => Action::CloseTab,
        KeyCode::Char('h') | KeyCode::Left => Action::PrevTab,
        KeyCode::Char('l') | KeyCode::Right => Action::NextTab,
        KeyCode::Char('r') => Action::RenameTab,
        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
            Action::GoToTab((c as usize) - ('1' as usize))
        }
        KeyCode::Esc => Action::ExitToNormal,
        _ if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('t') => {
            Action::ExitToNormal
        }
        _ => Action::None,
    }
}

fn handle_workspace(key: KeyEvent) -> Action {
    if let Some(action) = check_shared_alt(key) {
        return action;
    }

    match key.code {
        KeyCode::Char('h') | KeyCode::Left => Action::FocusWorkspacePanel,
        KeyCode::Char('l') | KeyCode::Right => Action::FocusRoomPanel,
        KeyCode::Char('j') | KeyCode::Down => Action::NavigateDown,
        KeyCode::Char('k') | KeyCode::Up => Action::NavigateUp,
        KeyCode::Enter => Action::Select,
        KeyCode::Char('n') => Action::Create,
        KeyCode::Char('x') => Action::Delete,
        KeyCode::Esc => Action::ExitToNormal,
        _ if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('w') => {
            Action::ExitToNormal
        }
        _ => Action::None,
    }
}

fn handle_resize(key: KeyEvent) -> Action {
    if let Some(action) = check_shared_alt(key) {
        return action;
    }

    match key.code {
        KeyCode::Char('h') | KeyCode::Left => Action::Resize(Direction::Left),
        KeyCode::Char('j') | KeyCode::Down => Action::Resize(Direction::Down),
        KeyCode::Char('k') | KeyCode::Up => Action::Resize(Direction::Up),
        KeyCode::Char('l') | KeyCode::Right => Action::Resize(Direction::Right),
        KeyCode::Char('H') => Action::ResizeReverse(Direction::Left),
        KeyCode::Char('J') => Action::ResizeReverse(Direction::Down),
        KeyCode::Char('K') => Action::ResizeReverse(Direction::Up),
        KeyCode::Char('L') => Action::ResizeReverse(Direction::Right),
        KeyCode::Esc => Action::ExitToNormal,
        _ if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('n') => {
            Action::ExitToNormal
        }
        _ => Action::None,
    }
}

fn check_shared_alt(key: KeyEvent) -> Option<Action> {
    if !key.modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    match key.code {
        KeyCode::Char('h') | KeyCode::Left => Some(Action::MoveFocus(Direction::Left)),
        KeyCode::Char('l') | KeyCode::Right => Some(Action::MoveFocus(Direction::Right)),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::NavigateDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::NavigateUp),
        _ => None,
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add src/tui/input.rs
git commit -m "feat: add zellij-style modal input handling"
```

---

### Task 9: Split Tree & Tab Layout

**Files:**
- Modify: `src/tui/layout.rs`
- Create: `tests/layout_test.rs`

- [ ] **Step 1: Write failing tests**

`tests/layout_test.rs`:

```rust
use humu::tui::layout::{SplitTree, SplitDirection, TabContainer};

#[test]
fn test_single_pane() {
    let tree = SplitTree::leaf(0);
    assert_eq!(tree.pane_ids(), vec![0]);
}

#[test]
fn test_split_vertical() {
    let mut tree = SplitTree::leaf(0);
    tree.split_vertical(0, 1);
    assert_eq!(tree.pane_ids(), vec![0, 1]);
}

#[test]
fn test_split_horizontal() {
    let mut tree = SplitTree::leaf(0);
    tree.split_horizontal(0, 1);
    assert_eq!(tree.pane_ids(), vec![0, 1]);
}

#[test]
fn test_remove_pane() {
    let mut tree = SplitTree::leaf(0);
    tree.split_vertical(0, 1);
    tree.remove_pane(0);
    assert_eq!(tree.pane_ids(), vec![1]);
}

#[test]
fn test_tab_container() {
    let mut tabs = TabContainer::new();
    tabs.add_tab("shell".into(), SplitTree::leaf(0));
    tabs.add_tab("claude".into(), SplitTree::leaf(1));
    assert_eq!(tabs.len(), 2);
    assert_eq!(tabs.active_index(), 0);

    tabs.set_active(1);
    assert_eq!(tabs.active_index(), 1);
    assert_eq!(tabs.active_name(), "claude");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test layout_test`
Expected: FAIL

- [ ] **Step 3: Implement split tree and tab container**

`src/tui/layout.rs`:

```rust
use ratatui::layout::Rect;

pub type PaneId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone)]
pub enum SplitTree {
    Leaf(PaneId),
    Split {
        direction: SplitDirection,
        ratio: f64,
        children: Box<(SplitTree, SplitTree)>,
    },
}

impl SplitTree {
    pub fn leaf(id: PaneId) -> Self {
        Self::Leaf(id)
    }

    pub fn pane_ids(&self) -> Vec<PaneId> {
        match self {
            Self::Leaf(id) => vec![*id],
            Self::Split { children, .. } => {
                let mut ids = children.0.pane_ids();
                ids.extend(children.1.pane_ids());
                ids
            }
        }
    }

    /// Split the pane with the given ID vertically (top/bottom), inserting new_id below.
    pub fn split_vertical(&mut self, target: PaneId, new_id: PaneId) -> bool {
        self.split(target, new_id, SplitDirection::Vertical)
    }

    /// Split the pane with the given ID horizontally (left/right), inserting new_id to the right.
    pub fn split_horizontal(&mut self, target: PaneId, new_id: PaneId) -> bool {
        self.split(target, new_id, SplitDirection::Horizontal)
    }

    fn split(&mut self, target: PaneId, new_id: PaneId, direction: SplitDirection) -> bool {
        match self {
            Self::Leaf(id) if *id == target => {
                let old = Self::Leaf(target);
                let new = Self::Leaf(new_id);
                *self = Self::Split {
                    direction,
                    ratio: 0.5,
                    children: Box::new((old, new)),
                };
                true
            }
            Self::Leaf(_) => false,
            Self::Split { children, .. } => {
                children.0.split(target, new_id, direction)
                    || children.1.split(target, new_id, direction)
            }
        }
    }

    /// Remove a pane from the tree. If it's part of a split, the sibling takes over.
    pub fn remove_pane(&mut self, target: PaneId) -> bool {
        match self {
            Self::Leaf(id) => *id == target,
            Self::Split { children, .. } => {
                // Check direct children first before recursing
                if matches!(children.0, Self::Leaf(id) if id == target) {
                    *self = children.1.clone();
                    return true;
                }
                if matches!(children.1, Self::Leaf(id) if id == target) {
                    *self = children.0.clone();
                    return true;
                }
                // Recurse into subtrees
                children.0.remove_pane(target) || children.1.remove_pane(target)
            }
        }
    }

    /// Compute the rects for each pane given the available area.
    pub fn compute_rects(&self, area: Rect) -> Vec<(PaneId, Rect)> {
        let mut result = Vec::new();
        self.compute_rects_inner(area, &mut result);
        result
    }

    fn compute_rects_inner(&self, area: Rect, result: &mut Vec<(PaneId, Rect)>) {
        match self {
            Self::Leaf(id) => {
                result.push((*id, area));
            }
            Self::Split {
                direction,
                ratio,
                children,
            } => {
                let (first, second) = match direction {
                    SplitDirection::Vertical => {
                        let first_h = (area.height as f64 * ratio) as u16;
                        let second_h = area.height.saturating_sub(first_h);
                        (
                            Rect::new(area.x, area.y, area.width, first_h),
                            Rect::new(area.x, area.y + first_h, area.width, second_h),
                        )
                    }
                    SplitDirection::Horizontal => {
                        let first_w = (area.width as f64 * ratio) as u16;
                        let second_w = area.width.saturating_sub(first_w);
                        (
                            Rect::new(area.x, area.y, first_w, area.height),
                            Rect::new(area.x + first_w, area.y, second_w, area.height),
                        )
                    }
                };
                children.0.compute_rects_inner(first, result);
                children.1.compute_rects_inner(second, result);
            }
        }
    }

    /// Adjust the ratio of the split containing the target pane.
    pub fn resize(&mut self, target: PaneId, delta: f64) -> bool {
        match self {
            Self::Leaf(_) => false,
            Self::Split {
                ratio, children, ..
            } => {
                if children.0.contains(target) || children.1.contains(target) {
                    *ratio = (*ratio + delta).clamp(0.1, 0.9);
                    true
                } else {
                    children.0.resize(target, delta)
                        || children.1.resize(target, delta)
                }
            }
        }
    }

    pub fn contains(&self, target: PaneId) -> bool {
        match self {
            Self::Leaf(id) => *id == target,
            Self::Split { children, .. } => {
                children.0.contains(target) || children.1.contains(target)
            }
        }
    }
}

#[derive(Debug)]
pub struct TabContainer {
    tabs: Vec<TabEntry>,
    active: usize,
}

#[derive(Debug)]
struct TabEntry {
    name: String,
    tree: SplitTree,
}

impl TabContainer {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
        }
    }

    pub fn add_tab(&mut self, name: String, tree: SplitTree) {
        self.tabs.push(TabEntry { name, tree });
    }

    pub fn remove_tab(&mut self, index: usize) -> Option<SplitTree> {
        if index < self.tabs.len() && self.tabs.len() > 1 {
            let entry = self.tabs.remove(index);
            if self.active >= self.tabs.len() {
                self.active = self.tabs.len() - 1;
            }
            Some(entry.tree)
        } else {
            None
        }
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn set_active(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
        }
    }

    pub fn active_tree(&self) -> Option<&SplitTree> {
        self.tabs.get(self.active).map(|t| &t.tree)
    }

    pub fn active_tree_mut(&mut self) -> Option<&mut SplitTree> {
        self.tabs.get_mut(self.active).map(|t| &mut t.tree)
    }

    pub fn active_name(&self) -> &str {
        self.tabs.get(self.active).map(|t| t.name.as_str()).unwrap_or("")
    }

    pub fn tab_names(&self) -> Vec<&str> {
        self.tabs.iter().map(|t| t.name.as_str()).collect()
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn rename_tab(&mut self, index: usize, name: String) {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.name = name;
        }
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --test layout_test`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/tui/layout.rs tests/layout_test.rs
git commit -m "feat: add split tree and tab container for pane layout management"
```

---

### Task 10: Status Bar Widget

**Files:**
- Modify: `src/tui/widgets/status_bar.rs`

- [ ] **Step 1: Implement status bar**

`src/tui/widgets/status_bar.rs`:

```rust
use crate::tui::input::Mode;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

pub struct StatusBar {
    mode: Mode,
}

impl StatusBar {
    pub fn new(mode: Mode) -> Self {
        Self { mode }
    }
}

impl Widget for StatusBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let bg = Style::default().bg(Color::DarkGray).fg(Color::White);
        // Fill background
        for x in area.x..area.right() {
            buf.set_string(x, area.y, " ", bg);
        }

        let hints = mode_hints(self.mode);
        let mut x = area.x + 1;

        for (i, (key, label)) in hints.iter().enumerate() {
            if i > 0 {
                let sep = " │ ";
                buf.set_string(x, area.y, sep, bg);
                x += sep.len() as u16;
            }

            let key_style = bg.add_modifier(Modifier::BOLD);
            buf.set_string(x, area.y, key, key_style);
            x += key.len() as u16;

            buf.set_string(x, area.y, " ", bg);
            x += 1;

            buf.set_string(x, area.y, label, bg);
            x += label.len() as u16;
        }
    }
}

fn mode_hints(mode: Mode) -> Vec<(&'static str, &'static str)> {
    match mode {
        Mode::Normal => vec![
            ("Ctrl+", ""),
            ("g", "LOCK"),
            ("p", "PANE"),
            ("t", "TAB"),
            ("w", "WORKSPACE"),
            ("n", "RESIZE"),
        ],
        Mode::Locked => vec![("Ctrl+g", "UNLOCK")],
        Mode::Pane => vec![
            ("n", "New"),
            ("d", "Split↓"),
            ("r", "Split→"),
            ("x", "Close"),
            ("hjkl", "Move"),
            ("f", "Fullscreen"),
            ("Esc", "Back"),
        ],
        Mode::Tab => vec![
            ("n", "New"),
            ("x", "Close"),
            ("h/l", "Prev/Next"),
            ("1-9", "GoTo"),
            ("r", "Rename"),
            ("Esc", "Back"),
        ],
        Mode::Workspace => vec![
            ("h/l", "Panel"),
            ("j/k", "Navigate"),
            ("Enter", "Select"),
            ("n", "Create"),
            ("x", "Delete"),
            ("Esc", "Back"),
        ],
        Mode::Resize => vec![
            ("hjkl", "Resize"),
            ("HJKL", "Reverse"),
            ("Esc", "Back"),
        ],
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add src/tui/widgets/status_bar.rs
git commit -m "feat: add mode-aware status bar widget"
```

---

### Task 11: Workspace & Room Panel Widgets

**Files:**
- Modify: `src/tui/widgets/workspace_panel.rs`
- Modify: `src/tui/widgets/room_panel.rs`

- [ ] **Step 1: Implement workspace panel**

`src/tui/widgets/workspace_panel.rs`:

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Widget};

pub struct WorkspacePanel<'a> {
    workspaces: &'a [WorkspaceItem],
    selected: Option<usize>,
    has_focus: bool,
}

pub struct WorkspaceItem {
    pub name: String,
    pub active: bool, // spinner indicator
}

impl<'a> WorkspacePanel<'a> {
    pub fn new(workspaces: &'a [WorkspaceItem]) -> Self {
        Self {
            workspaces,
            selected: None,
            has_focus: false,
        }
    }

    pub fn selected(mut self, index: Option<usize>) -> Self {
        self.selected = index;
        self
    }

    pub fn focus(mut self, focused: bool) -> Self {
        self.has_focus = focused;
        self
    }
}

impl Widget for WorkspacePanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_style = if self.has_focus {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(" WORKSPACES ")
            .borders(Borders::ALL)
            .border_style(border_style);
        let inner = block.inner(area);
        block.render(area, buf);

        for (i, ws) in self.workspaces.iter().enumerate() {
            if i as u16 >= inner.height {
                break;
            }
            let y = inner.y + i as u16;
            let is_selected = self.selected == Some(i);

            let prefix = if is_selected { "▸ " } else { "  " };
            let suffix = if ws.active { " ⠋" } else { "" };
            let text = format!("{prefix}{}{suffix}", ws.name);

            let style = if is_selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            buf.set_string(inner.x, y, &text, style);
        }
    }
}
```

- [ ] **Step 2: Implement room panel**

`src/tui/widgets/room_panel.rs`:

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Widget};

pub struct RoomPanel<'a> {
    rooms: &'a [RoomItem],
    selected: Option<usize>,
    has_focus: bool,
}

pub struct RoomItem {
    pub name: String,
    pub is_default: bool,
    pub active: bool, // spinner indicator
}

impl<'a> RoomPanel<'a> {
    pub fn new(rooms: &'a [RoomItem]) -> Self {
        Self {
            rooms,
            selected: None,
            has_focus: false,
        }
    }

    pub fn selected(mut self, index: Option<usize>) -> Self {
        self.selected = index;
        self
    }

    pub fn focus(mut self, focused: bool) -> Self {
        self.has_focus = focused;
        self
    }
}

impl Widget for RoomPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_style = if self.has_focus {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(" ROOMS ")
            .borders(Borders::ALL)
            .border_style(border_style);
        let inner = block.inner(area);
        block.render(area, buf);

        for (i, room) in self.rooms.iter().enumerate() {
            if i as u16 >= inner.height {
                break;
            }
            let y = inner.y + i as u16;
            let is_selected = self.selected == Some(i);

            let prefix = if is_selected { "▸ " } else { "  " };
            let suffix = if room.active { " ⠋" } else { "" };
            let text = format!("{prefix}{}{suffix}", room.name);

            let style = if is_selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            buf.set_string(inner.x, y, &text, style);
        }
    }
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add src/tui/widgets/workspace_panel.rs src/tui/widgets/room_panel.rs
git commit -m "feat: add workspace and room panel widgets"
```

---

### Task 12: App Shell — Main Event Loop

Note: This task creates the TUI shell with placeholder panels. Terminal panes are not wired yet — that happens in Task 13. The terminal area shows an empty bordered box.

**Files:**
- Modify: `src/app.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Create app.rs with basic ratatui event loop**

`src/app.rs`:

```rust
use crate::config::{humu_dir, HumuConfig, HumuState};
use crate::tui::input::{handle_key, Action, Mode};
use crate::tui::widgets::status_bar::StatusBar;
use crate::tui::widgets::workspace_panel::{WorkspaceItem, WorkspacePanel};
use crate::tui::widgets::room_panel::{RoomItem, RoomPanel};
use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::Terminal;
use std::io::stdout;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedPanel {
    Workspace,
    Room,
    Terminal,
}

pub struct App {
    pub config: HumuConfig,
    pub state: HumuState,
    pub mode: Mode,
    pub focus: FocusedPanel,
    pub workspace_selected: Option<usize>,
    pub room_selected: Option<usize>,
    pub running: bool,
}

impl App {
    pub fn new() -> Result<Self> {
        let config_path = humu_dir().join("config.toml");
        let state_path = humu_dir().join("state.toml");

        let config = HumuConfig::load(&config_path)?;
        let state = HumuState::load(&state_path)?;

        Ok(Self {
            config,
            state,
            mode: Mode::Normal,
            focus: FocusedPanel::Terminal,
            workspace_selected: None,
            room_selected: None,
            running: true,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        stdout().execute(EnterAlternateScreen)?;
        crossterm::execute!(stdout(), crossterm::event::EnableMouseCapture)?;

        let backend = ratatui::backend::CrosstermBackend::new(stdout());
        let mut terminal = Terminal::new(backend)?;

        // Restore last active workspace/room
        self.restore_selection();

        while self.running {
            terminal.draw(|frame| self.render(frame))?;

            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.handle_action(handle_key(self.mode, key));
                    }
                }
            }
        }

        crossterm::execute!(stdout(), crossterm::event::DisableMouseCapture)?;
        stdout().execute(LeaveAlternateScreen)?;
        disable_raw_mode()?;

        // Save state on exit
        let state_path = humu_dir().join("state.toml");
        self.state.save(&state_path)?;

        Ok(())
    }

    fn render(&self, frame: &mut ratatui::Frame) {
        let size = frame.area();

        // Main layout: [workspace | room | terminal] + status bar
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(size);

        let panel_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(20),
                Constraint::Length(18),
                Constraint::Min(1),
            ])
            .split(main_chunks[0]);

        // Workspace panel
        let workspaces = self.workspace_items();
        let ws_widget = WorkspacePanel::new(&workspaces)
            .selected(self.workspace_selected)
            .focus(self.focus == FocusedPanel::Workspace);
        frame.render_widget(ws_widget, panel_chunks[0]);

        // Room panel
        let rooms = self.room_items();
        let room_widget = RoomPanel::new(&rooms)
            .selected(self.room_selected)
            .focus(self.focus == FocusedPanel::Room);
        frame.render_widget(room_widget, panel_chunks[1]);

        // Terminal area placeholder (implemented in later task)
        let terminal_block = ratatui::widgets::Block::default()
            .title(" TERMINAL ")
            .borders(ratatui::widgets::Borders::ALL)
            .border_style(ratatui::style::Style::default().fg(
                if self.focus == FocusedPanel::Terminal {
                    ratatui::style::Color::Cyan
                } else {
                    ratatui::style::Color::DarkGray
                },
            ));
        frame.render_widget(terminal_block, panel_chunks[2]);

        // Status bar
        frame.render_widget(StatusBar::new(self.mode), main_chunks[1]);
    }

    fn handle_action(&mut self, action: Action) {
        match action {
            Action::EnterMode(mode) => self.mode = mode,
            Action::ExitToNormal => self.mode = Mode::Normal,

            Action::FocusWorkspacePanel => self.focus = FocusedPanel::Workspace,
            Action::FocusRoomPanel => self.focus = FocusedPanel::Room,

            Action::NavigateUp => self.navigate(-1),
            Action::NavigateDown => self.navigate(1),
            Action::Select => self.select_current(),

            // TODO: implement remaining actions in later tasks
            _ => {}
        }
    }

    fn navigate(&mut self, delta: i32) {
        match self.focus {
            FocusedPanel::Workspace => {
                let count = self.state.workspaces.len();
                if count > 0 {
                    let current = self.workspace_selected.unwrap_or(0) as i32;
                    let next = (current + delta).clamp(0, count as i32 - 1) as usize;
                    self.workspace_selected = Some(next);
                }
            }
            FocusedPanel::Room => {
                // TODO: navigate rooms
            }
            FocusedPanel::Terminal => {}
        }
    }

    fn select_current(&mut self) {
        // TODO: implement workspace/room selection
    }

    fn restore_selection(&mut self) {
        // TODO: restore from state.toml
    }

    fn workspace_items(&self) -> Vec<WorkspaceItem> {
        let mut names: Vec<_> = self.state.workspaces.keys().cloned().collect();
        names.sort();
        names
            .into_iter()
            .map(|name| WorkspaceItem {
                name,
                active: false, // TODO: hook integration
            })
            .collect()
    }

    fn room_items(&self) -> Vec<RoomItem> {
        // TODO: list rooms from git
        vec![]
    }
}
```

- [ ] **Step 2: Update main.rs**

`src/main.rs`:

```rust
mod app;

use anyhow::Result;

fn main() -> Result<()> {
    let mut app = app::App::new()?;
    app.run()
}
```

- [ ] **Step 3: Add app module to lib.rs for testing**

Note: `app.rs` lives in the binary crate (next to `main.rs`), not in `lib.rs`. This is fine — the binary uses `lib.rs` modules, and `app.rs` is the glue.

- [ ] **Step 4: Add Ctrl+q quit keybinding**

In `src/tui/input.rs`, add `Quit` to the `Action` enum. In `handle_normal`, map `Ctrl+q` → `Action::Quit`. In `app.rs`, handle `Action::Quit` by setting `self.running = false`.

- [ ] **Step 5: Build and run manually**

Run: `cargo run`
Expected: TUI renders with workspace panel, room panel, terminal placeholder, and status bar. `Ctrl+g` toggles lock mode (status bar changes). `Ctrl+q` exits cleanly.

- [ ] **Step 6: Commit**

```bash
git add src/app.rs src/main.rs src/tui/input.rs
git commit -m "feat: add TUI app shell with panels, status bar, and modal input"
```

---

## Chunk 4: Terminal Integration

### Task 13: Wire Terminal Panes into TUI

**Files:**
- Modify: `src/tui/widgets/terminal_area.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Implement terminal area widget**

`src/tui/widgets/terminal_area.rs`:

```rust
use crate::tui::layout::{PaneId, SplitTree, TabContainer};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Widget};

pub struct TabBar<'a> {
    tab_names: &'a [&'a str],
    active: usize,
    active_indicators: &'a [bool], // which tabs have active Claude
}

impl<'a> TabBar<'a> {
    pub fn new(tab_names: &'a [&'a str], active: usize, active_indicators: &'a [bool]) -> Self {
        Self {
            tab_names,
            active,
            active_indicators,
        }
    }
}

impl Widget for TabBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let bg = Style::default().bg(Color::Black);
        for x in area.x..area.right() {
            buf.set_string(x, area.y, " ", bg);
        }

        let mut x = area.x;
        for (i, name) in self.tab_names.iter().enumerate() {
            let is_active = i == self.active;
            let spinner = if self.active_indicators.get(i).copied().unwrap_or(false) {
                " ⠋"
            } else {
                ""
            };
            let text = format!(" {name}{spinner} ");

            let style = if is_active {
                Style::default()
                    .fg(Color::Cyan)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray).bg(Color::Black)
            };

            buf.set_string(x, area.y, &text, style);
            x += text.len() as u16;
        }

        // "+" button
        let plus_style = Style::default().fg(Color::DarkGray).bg(Color::Black);
        buf.set_string(x, area.y, " + ", plus_style);
    }
}
```

- [ ] **Step 2: Add pane map to App state**

In `src/app.rs`, add a `HashMap<PaneId, PtyPane>` to the App struct. On room selection, create default tabs and spawn PTY panes. On each tick, call `process_output()` on each pane.

Key additions to `App`:

```rust
use crate::pty::pane::PtyPane;
use crate::tui::layout::{PaneId, TabContainer, SplitTree};
use std::collections::HashMap;

// In App struct:
pub panes: HashMap<PaneId, PtyPane>,
pub tabs: TabContainer,
pub next_pane_id: PaneId,
pub focused_pane: Option<PaneId>,
```

- [ ] **Step 3: Implement rendering loop for terminal area**

In `render()`, after drawing the tab bar:
1. Get the active tab's `SplitTree`
2. Call `compute_rects(terminal_rect)` to get per-pane rects
3. For each `(pane_id, rect)`, render `TerminalWidget` with the pane's vt100 screen

- [ ] **Step 4: Forward key input to focused pane in Normal/Locked mode**

In `handle_action`, when `Action::PassThrough(key)`:
- Convert the `KeyEvent` to bytes and write to the focused pane's PTY via `pane.write_input()`
- Key-to-bytes encoding (add a helper function `key_event_to_bytes`):

```rust
fn key_event_to_bytes(key: &KeyEvent) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char(c) if ctrl => vec![(c as u8) & 0x1f], // Ctrl+a = 0x01, etc.
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            s.as_bytes().to_vec()
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
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
```

- [ ] **Step 5: Process PTY output each tick**

In the event loop, after polling: iterate all panes and call `process_output()`.

- [ ] **Step 6: Build and test manually**

Run: `cargo run`
Expected: A shell spawns in the terminal area. You can type commands. `Ctrl+p` enters Pane mode.

- [ ] **Step 7: Commit**

```bash
git add src/tui/widgets/terminal_area.rs src/app.rs
git commit -m "feat: wire PTY terminal panes into TUI with tab bar rendering"
```

---

### Task 14: Tab and Split Actions

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Implement tab actions**

In `handle_action`, implement:
- `NewTab` → open preset selector (placeholder: spawn shell), add tab to `TabContainer`
- `CloseTab` → remove tab, close all panes in it
- `PrevTab` / `NextTab` → switch active tab
- `GoToTab(n)` → set active tab

- [ ] **Step 2: Implement split actions**

In `handle_action`, implement:
- `SplitDown` → split focused pane vertically, spawn new PTY in the new pane
- `SplitRight` → split focused pane horizontally, spawn new PTY
- `ClosePane` → remove pane from split tree, kill PTY
- `MoveFocus(direction)` → find adjacent pane based on rects, update `focused_pane`

- [ ] **Step 3: Test manually**

Run: `cargo run`
Expected: `Ctrl+p` → `d` splits down, `r` splits right, `x` closes pane, `Ctrl+t` → `n` adds tab.

- [ ] **Step 4: Commit**

```bash
git add src/app.rs
git commit -m "feat: implement tab and split pane actions"
```

---

### Task 15: Layout Persistence

**Files:**
- Modify: `src/app.rs`
- Modify: `src/config.rs`

- [ ] **Step 1: Implement save_layout method on App**

Convert the current `TabContainer` + pane presets into the `RoomLayout` struct from `config.rs`. Save to `state.toml` on room switch and app exit.

- [ ] **Step 2: Implement restore_layout method on App**

On room selection, check `state.layout` for a saved layout. If found, reconstruct `TabContainer` and spawn PTY panes per the saved preset. If not found, create a single tab with a shell pane.

- [ ] **Step 3: Test manually**

Run: `cargo run`
Expected: Create splits, exit, relaunch — layout restored.

- [ ] **Step 4: Commit**

```bash
git add src/app.rs src/config.rs
git commit -m "feat: persist and restore tab/pane layout per room"
```

---

## Chunk 5: Dialogs & CRUD

### Task 16: Preset Selector Widget

**Files:**
- Modify: `src/tui/widgets/preset_selector.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Implement preset selector popup**

`src/tui/widgets/preset_selector.rs`:

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Widget};

pub struct PresetSelector<'a> {
    presets: &'a [String],
    selected: usize,
}

impl<'a> PresetSelector<'a> {
    pub fn new(presets: &'a [String], selected: usize) -> Self {
        Self { presets, selected }
    }
}

impl Widget for PresetSelector<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Center the popup
        let width = 30u16.min(area.width);
        let height = (self.presets.len() as u16 + 2).min(area.height);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup = Rect::new(x, y, width, height);

        Clear.render(popup, buf);
        let block = Block::default()
            .title(" Select Preset ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(popup);
        block.render(popup, buf);

        for (i, name) in self.presets.iter().enumerate() {
            if i as u16 >= inner.height {
                break;
            }
            let style = if i == self.selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let prefix = if i == self.selected { " ▸ " } else { "   " };
            buf.set_string(inner.x, inner.y + i as u16, &format!("{prefix}{name}"), style);
        }
    }
}
```

- [ ] **Step 2: Wire into app — show popup when NewTab/NewPane action fires**

Add a `popup: Option<PopupState>` field to `App`. When `NewTab` or `NewPane`, set popup to `PresetSelectorPopup`. Handle `j/k/Enter/Esc` when popup is active.

- [ ] **Step 3: Test manually**

Run: `cargo run`
Expected: `Ctrl+t` → `n` shows preset popup, select with `j/k`, confirm with `Enter`.

- [ ] **Step 4: Commit**

```bash
git add src/tui/widgets/preset_selector.rs src/app.rs
git commit -m "feat: add preset selector popup for creating tabs and panes"
```

---

### Task 17: Create/Delete Dialogs

**Files:**
- Modify: `src/tui/widgets/dialog.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Implement dialog widget**

`src/tui/widgets/dialog.rs` — a generic modal dialog supporting:
- Text input fields (for workspace path, URL, room branch name, base branch)
- Dropdown selection (for workspace creation mode)
- Confirmation dialogs ("Delete workspace? Also remove from disk?")

Key struct:

```rust
pub struct Dialog {
    pub title: String,
    pub fields: Vec<DialogField>,
    pub focused_field: usize,
    pub confirmed: bool,
    pub cancelled: bool,
}

pub enum DialogField {
    TextInput { label: String, value: String },
    Select { label: String, options: Vec<String>, selected: usize },
    Confirm { message: String, yes: bool },
}
```

- [ ] **Step 2: Wire workspace creation dialog**

When `Create` action fires with `FocusedPanel::Workspace`:
- Show dialog with fields: Mode (Clone/Existing/New), Path, URL (if Clone)
- On confirm: call `WorkspaceManager.clone_remote()`, `register()`, or `init()`

- [ ] **Step 3: Wire room creation dialog**

When `Create` action fires with `FocusedPanel::Room`:
- Show dialog with fields: Branch name, Base branch
- On confirm: call `RoomManager.create()`

- [ ] **Step 4: Wire delete confirmations**

When `Delete` action fires:
- Workspace: "Delete workspace '{name}'? Also delete repo from disk? [Yes/No/Cancel]"
- Room: "Delete room '{name}'? This will remove the worktree and branch. [Yes/No]"

- [ ] **Step 5: Test manually**

Run: `cargo run`
Expected: `Ctrl+w` → `n` shows create dialog, fill fields, confirm creates workspace/room.

- [ ] **Step 6: Commit**

```bash
git add src/tui/widgets/dialog.rs src/app.rs
git commit -m "feat: add create/delete dialogs for workspace and room management"
```

---

## Chunk 6: Hook Integration

### Task 18: Unix Socket Hook Server

**Files:**
- Create: `src/hook/mod.rs`
- Create: `src/hook/server.rs`
- Create: `tests/hook_test.rs`

- [ ] **Step 1: Write failing test**

`tests/hook_test.rs`:

```rust
use humu::hook::server::HookServer;
use std::io::Write;
use std::os::unix::net::UnixStream;
use tempfile::TempDir;

#[tokio::test]
async fn test_hook_server_receives_event() {
    let dir = TempDir::new().unwrap();
    let sock_path = dir.path().join("humu.sock");

    let server = HookServer::new(&sock_path).await.unwrap();
    let mut rx = server.subscribe();

    // Send an event from a "client" (hook script)
    let mut stream = UnixStream::connect(&sock_path).unwrap();
    let event = r#"{"workspace":"humu","room":"feat/auth","hook_type":"PreToolUse","tool":"Edit"}"#;
    writeln!(stream, "{event}").unwrap();
    drop(stream);

    // Server should receive it
    let received = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        rx.recv(),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(received.workspace, "humu");
    assert_eq!(received.room, "feat/auth");
    assert_eq!(received.hook_type, "PreToolUse");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test hook_test`
Expected: FAIL

- [ ] **Step 3: Implement hook server**

`src/hook/mod.rs`:

```rust
pub mod server;
```

`src/hook/server.rs`:

```rust
use anyhow::Result;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tokio::io::AsyncBufReadExt;
use tokio::net::UnixListener;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Deserialize)]
pub struct HookEvent {
    pub workspace: String,
    pub room: String,
    pub hook_type: String,
    /// Additional fields vary by hook type, passed through as-is.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

pub struct HookServer {
    sock_path: PathBuf,
    tx: broadcast::Sender<HookEvent>,
}

impl HookServer {
    pub async fn new(sock_path: &Path) -> Result<Self> {
        // Remove stale socket file
        if sock_path.exists() {
            std::fs::remove_file(sock_path)?;
        }
        if let Some(parent) = sock_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(sock_path)?;
        let (tx, _) = broadcast::channel(256);
        let tx_clone = tx.clone();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let tx = tx_clone.clone();
                        tokio::spawn(async move {
                            let reader = tokio::io::BufReader::new(stream);
                            let mut lines = reader.lines();
                            while let Ok(Some(line)) = lines.next_line().await {
                                if let Ok(event) = serde_json::from_str::<HookEvent>(&line) {
                                    let _ = tx.send(event);
                                }
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("hook server accept error: {e}");
                    }
                }
            }
        });

        Ok(Self {
            sock_path: sock_path.to_path_buf(),
            tx,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<HookEvent> {
        self.tx.subscribe()
    }
}

impl Drop for HookServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.sock_path);
    }
}
```

- [ ] **Step 4: Export from lib.rs**

Add to `src/lib.rs`:

```rust
pub mod hook;
```

- [ ] **Step 5: Run tests**

Run: `cargo test --test hook_test`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/hook/ tests/hook_test.rs src/lib.rs
git commit -m "feat: add Unix socket hook server for Claude Code events"
```

---

### Task 19: Spinner State Integration

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add spinner state tracking to App**

```rust
use std::collections::HashMap;
use std::time::Instant;

// In App:
pub spinner_state: HashMap<(String, String), Instant>, // (workspace, room) → last event time
```

- [ ] **Step 2: Process hook events in the event loop**

In the main loop, check `hook_rx.try_recv()` each tick:
- On `PreToolUse`: insert/update `spinner_state[(workspace, room)] = Instant::now()`
- On `Stop`: remove the entry
- Each tick: remove entries older than 10 seconds (timeout)

- [ ] **Step 3: Pass spinner state to panel widgets**

When constructing `WorkspaceItem` and `RoomItem`, check if `spinner_state` has an active entry for the corresponding workspace/room.

- [ ] **Step 4: Set HUMU_* env vars when spawning Claude preset**

In the pane spawn logic, when the preset name is `"claude"` (the built-in preset), inject hook env vars. Note: only the built-in `"claude"` preset is treated as a Claude instance. If users create custom Claude presets with different names, they won't get hook integration — this is acceptable for now.

```rust
cmd.env("HUMU_SOCKET", humu_dir().join("humu.sock"));
cmd.env("HUMU_WORKSPACE", &current_workspace);
cmd.env("HUMU_ROOM", &current_room);
```

- [ ] **Step 5: Test manually**

Run: `cargo run`, create a Claude tab, verify spinner appears while Claude is working.

- [ ] **Step 6: Commit**

```bash
git add src/app.rs
git commit -m "feat: integrate Claude hook events with spinner indicators"
```

---

## Chunk 7: Mouse & Resize

### Task 20: Mouse Click Support

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Handle mouse events in the event loop**

```rust
Event::Mouse(mouse) => {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            self.handle_click(mouse.column, mouse.row);
        }
        _ => {}
    }
}
```

- [ ] **Step 2: Implement handle_click**

Based on the click coordinates, determine which panel was clicked:
- WorkspacePanel area → select workspace at that row
- RoomPanel area → select room at that row
- Tab bar area → switch to clicked tab, or open preset selector if `+` clicked
- Terminal pane area → focus that pane

Store panel rects from the last render to perform hit testing.

- [ ] **Step 3: Test manually**

Run: `cargo run`
Expected: Click workspace/room to select, click tabs to switch.

- [ ] **Step 4: Commit**

```bash
git add src/app.rs
git commit -m "feat: add mouse click support for panels, tabs, and panes"
```

---

### Task 21: Draggable Resize Handles

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Track panel widths as state**

```rust
pub panel_widths: [u16; 2], // [workspace_panel_width, room_panel_width]
```

- [ ] **Step 2: Handle mouse drag on panel borders**

Detect `MouseEventKind::Drag` near panel border columns. Update `panel_widths` accordingly. Use the updated widths in the Layout constraints.

- [ ] **Step 3: Handle resize for split panes**

When dragging on a split boundary within the terminal area, find the corresponding `SplitTree` node and adjust its `ratio`.

- [ ] **Step 4: Test manually**

Run: `cargo run`
Expected: Drag panel borders to resize.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat: add draggable resize handles for panels and split panes"
```

---

## Chunk 8: Hook Script & Final Polish

### Task 22: Ship Hook Script

**Files:**
- Create: `scripts/humu-hook.sh`

- [ ] **Step 1: Create the hook script**

`scripts/humu-hook.sh`:

```bash
#!/bin/bash
# Claude Code hook script for humu integration.
# Merges workspace/room into the hook JSON as flat top-level fields.
#
# Install: Add to ~/.claude/settings.json hooks configuration
# Requires: jq, socat

if [ -n "$HUMU_SOCKET" ] && command -v socat &> /dev/null && command -v jq &> /dev/null; then
  INPUT=$(cat)
  echo "$INPUT" | jq -c \
    --arg ws "$HUMU_WORKSPACE" \
    --arg rm "$HUMU_ROOM" \
    --arg ht "$CLAUDE_HOOK_TYPE" \
    '. + {workspace: $ws, room: $rm, hook_type: $ht}' \
    | socat - UNIX-CONNECT:"$HUMU_SOCKET" 2>/dev/null || true
fi
```

- [ ] **Step 2: Make executable**

```bash
chmod +x scripts/humu-hook.sh
```

- [ ] **Step 3: Commit**

```bash
git add scripts/humu-hook.sh
git commit -m "feat: add Claude Code hook script for humu integration"
```

---

### Task 23: Graceful Shutdown

`Ctrl+q` is already implemented in Task 12. This task adds proper cleanup.

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Implement graceful shutdown**

On quit (when `self.running` becomes false):
1. Save layout state to `state.toml`
2. Kill all PTY child processes (iterate `self.panes`, drop them)
3. Remove `humu.sock`
4. Restore terminal (already handled by the run loop's cleanup)

- [ ] **Step 2: Test manually**

Run: `cargo run`, create some splits/tabs, `Ctrl+q`. Relaunch — layout should restore.

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "feat: add graceful shutdown with PTY cleanup and state save"
```
