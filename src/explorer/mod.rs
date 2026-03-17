pub mod icons;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The kind of a file entry.
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum FileKind {
    File,
    Directory,
}

/// Git status for a file.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum GitStatus {
    Modified,
    Added,
}

/// A single entry in the file tree.
#[derive(Clone, Debug)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub kind: FileKind,
    pub git_status: Option<GitStatus>,
    pub depth: usize,
    pub expanded: bool,
}

/// Parses `git status --porcelain` output into a map of path -> GitStatus.
pub fn parse_git_status(output: &str) -> HashMap<PathBuf, GitStatus> {
    let mut map = HashMap::new();
    for line in output.lines() {
        if line.len() < 4 {
            continue;
        }
        let x = line.as_bytes()[0];
        let y = line.as_bytes()[1];
        // char 2 must be space
        let rest = &line[3..];

        // Deleted in either column → skip
        if x == b'D' || y == b'D' {
            continue;
        }

        // Rename or copy → parse "old -> new", insert new as Added
        if x == b'R' || x == b'C' {
            if let Some(arrow_pos) = rest.find(" -> ") {
                let new_path = &rest[arrow_pos + 4..];
                map.insert(PathBuf::from(new_path), GitStatus::Added);
            }
            continue;
        }

        // Untracked
        if x == b'?' && y == b'?' {
            map.insert(PathBuf::from(rest), GitStatus::Added);
            continue;
        }

        // Added
        if x == b'A' {
            map.insert(PathBuf::from(rest), GitStatus::Added);
            continue;
        }

        // Modified in either column
        if x == b'M' || y == b'M' {
            map.insert(PathBuf::from(rest), GitStatus::Modified);
            continue;
        }

        // Everything else → Modified
        map.insert(PathBuf::from(rest), GitStatus::Modified);
    }
    map
}

/// State for the file explorer panel.
pub struct ExplorerState {
    pub entries: Vec<FileEntry>,
    pub selected: usize,
    pub scroll_offset: usize,
    pub expanded_dirs: HashSet<PathBuf>,
    pub show_ignored: bool,
    pub root: PathBuf,
    pub delta_available: Option<bool>,
}

impl ExplorerState {
    pub fn new(root: PathBuf) -> Self {
        Self {
            entries: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            expanded_dirs: HashSet::new(),
            show_ignored: false,
            root,
            delta_available: None,
        }
    }

    /// Rebuild the file tree from the filesystem and git status.
    pub fn scan(&mut self) {
        let git_status = self.read_git_status();
        let mut entries = Vec::new();
        self.build_tree(&self.root.clone(), 0, &git_status, &mut entries);
        propagate_git_status(&mut entries);
        self.entries = entries;

        // Clamp selected index
        if !self.entries.is_empty() && self.selected >= self.entries.len() {
            self.selected = self.entries.len() - 1;
        }
    }

    /// Toggle expand/collapse for the currently selected directory.
    pub fn toggle_dir(&mut self) {
        if let Some(entry) = self.entries.get(self.selected) {
            if entry.kind == FileKind::Directory {
                let path = entry.path.clone();
                if self.expanded_dirs.contains(&path) {
                    self.expanded_dirs.remove(&path);
                } else {
                    self.expanded_dirs.insert(path);
                }
                self.scan();
            }
        }
    }

    /// Move selection up by one.
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move selection down by one.
    pub fn move_down(&mut self) {
        if !self.entries.is_empty() && self.selected < self.entries.len() - 1 {
            self.selected += 1;
        }
    }

    /// Adjust scroll_offset so the selected entry is visible within the viewport.
    pub fn scroll_to_visible(&mut self, viewport_height: usize) {
        if viewport_height == 0 {
            return;
        }
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + viewport_height {
            self.scroll_offset = self.selected - viewport_height + 1;
        }
    }

    /// Check if `delta` is available on the system (cached).
    pub fn check_delta(&mut self) -> bool {
        if let Some(avail) = self.delta_available {
            return avail;
        }
        let avail = Command::new("which")
            .arg("delta")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        self.delta_available = Some(avail);
        avail
    }

    /// Returns a reference to the currently selected entry, if any.
    pub fn selected_entry(&self) -> Option<&FileEntry> {
        self.entries.get(self.selected)
    }

    // ── private helpers ──

    fn read_git_status(&self) -> HashMap<PathBuf, GitStatus> {
        let output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.root)
            .output();
        match output {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                parse_git_status(&stdout)
            }
            Err(_) => HashMap::new(),
        }
    }

    fn build_tree(
        &self,
        dir: &Path,
        depth: usize,
        git_status: &HashMap<PathBuf, GitStatus>,
        entries: &mut Vec<FileEntry>,
    ) {
        let mut children = self.list_children(dir);

        // Sort: directories first, then case-insensitive alphabetical
        children.sort_by(|a, b| {
            let a_is_dir = a.is_dir();
            let b_is_dir = b.is_dir();
            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    let a_name = a.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                    let b_name = b.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                    a_name.cmp(&b_name)
                }
            }
        });

        for child in children {
            let name = child
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            // Skip .git directory
            if name == ".git" {
                continue;
            }

            let is_dir = child.is_dir();
            let rel_path = child.strip_prefix(&self.root).unwrap_or(&child).to_path_buf();
            let expanded = is_dir && self.expanded_dirs.contains(&child);

            let file_git_status = if is_dir {
                None // will be propagated later
            } else {
                git_status.get(&rel_path).copied()
            };

            entries.push(FileEntry {
                name,
                path: child.clone(),
                kind: if is_dir {
                    FileKind::Directory
                } else {
                    FileKind::File
                },
                git_status: file_git_status,
                depth,
                expanded,
            });

            if is_dir && expanded {
                self.build_tree(&child, depth + 1, git_status, entries);
            }
        }
    }

    fn list_children(&self, dir: &Path) -> Vec<PathBuf> {
        if self.show_ignored {
            self.list_children_all(dir)
        } else {
            self.list_children_git(dir)
        }
    }

    /// List children using plain read_dir (skip .git/).
    fn list_children_all(&self, dir: &Path) -> Vec<PathBuf> {
        let Ok(read_dir) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        read_dir
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .map(|n| n != ".git")
                    .unwrap_or(true)
            })
            .collect()
    }

    /// List children using git ls-files to respect .gitignore.
    fn list_children_git(&self, dir: &Path) -> Vec<PathBuf> {
        let rel_dir = dir.strip_prefix(&self.root).unwrap_or(dir);

        // Get tracked files
        let tracked = Command::new("git")
            .args(["ls-files"])
            .arg(rel_dir)
            .current_dir(&self.root)
            .output();

        // Get untracked (non-ignored) files
        let untracked = Command::new("git")
            .args(["ls-files", "--others", "--exclude-standard"])
            .arg(rel_dir)
            .current_dir(&self.root)
            .output();

        let mut allowed: HashSet<PathBuf> = HashSet::new();

        for output in [tracked, untracked] {
            if let Ok(o) = output {
                let stdout = String::from_utf8_lossy(&o.stdout);
                for line in stdout.lines() {
                    let p = self.root.join(line);
                    // Collect direct children: either the file itself, or the
                    // first directory component relative to `dir`.
                    if let Ok(rel) = p.strip_prefix(dir) {
                        let mut components = rel.components();
                        if let Some(first) = components.next() {
                            allowed.insert(dir.join(first));
                        }
                    }
                }
            }
        }

        allowed.into_iter().collect()
    }
}

/// Propagate git status from files upward to parent directories.
fn propagate_git_status(entries: &mut Vec<FileEntry>) {
    // Walk backwards so children are processed before parents in collapsed trees.
    // For expanded trees, we need to propagate from deeper entries to shallower ones.
    let len = entries.len();
    for i in (0..len).rev() {
        if entries[i].kind == FileKind::Directory && entries[i].git_status.is_none() {
            // Look at all subsequent entries that are deeper (children of this dir)
            let dir_depth = entries[i].depth;
            let dir_path = entries[i].path.clone();
            let mut status: Option<GitStatus> = None;

            for j in (i + 1)..len {
                if entries[j].depth <= dir_depth {
                    break;
                }
                // Only consider direct and indirect children under this directory
                if entries[j].path.starts_with(&dir_path) {
                    if let Some(s) = entries[j].git_status {
                        status = Some(match (status, s) {
                            (None, s) => s,
                            (Some(GitStatus::Modified), _) => GitStatus::Modified,
                            (_, GitStatus::Modified) => GitStatus::Modified,
                            (Some(GitStatus::Added), GitStatus::Added) => GitStatus::Added,
                        });
                    }
                }
            }

            entries[i].git_status = status;
        }
    }
}
