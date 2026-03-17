# File Explorer Panel Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a file explorer panel to the right side of the TUI layout showing the workspace directory tree with Nerd Font icons and git status indicators.

**Architecture:** New `src/explorer/` module for tree data model and scanning, new `src/tui/widgets/explorer_panel.rs` for rendering. Layout extends from 3 to 4 horizontal panels. New `Mode::Explorer` with `Ctrl+E` binding. Enter opens files in `$EDITOR`, Shift+Enter shows diff via `delta`.

**Tech Stack:** `std::fs` for filesystem traversal, `std::process::Command` for git commands, existing Ratatui widgets.

**Spec:** `docs/PRDs/007-file-explorer.md`

---

## Chunk 1: Explorer Module (data model + scanning)

### Task 1: Create icons module

**Files:**
- Create: `src/explorer/icons.rs`
- Create: `src/explorer/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create icon lookup module**

Create `src/explorer/icons.rs`:

```rust
/// Returns the Nerd Font icon for a file extension.
pub fn file_icon(filename: &str) -> &'static str {
    // Check exact filename first (Dockerfile, Makefile, etc.)
    match filename {
        "Dockerfile" | "dockerfile" => return "\u{f0868}",  // 󰡨
        "Makefile" | "makefile" | "GNUmakefile" => return "\u{e779}",  //
        ".gitignore" | ".gitmodules" | ".gitattributes" => return "\u{e702}",  //
        _ => {}
    }

    // Then check extension
    let ext = filename.rsplit('.').next().unwrap_or("");
    match ext {
        "rs" => "\u{e7a8}",      //
        "py" | "pyw" | "pyi" => "\u{e73c}",  //
        "js" | "mjs" | "cjs" => "\u{e74e}",  //
        "ts" | "mts" | "cts" => "\u{e628}",  //
        "jsx" => "\u{e7ba}",     //
        "tsx" => "\u{e7ba}",     //
        "go" => "\u{e627}",      //
        "java" => "\u{e738}",    //
        "c" => "\u{e61e}",       //
        "cpp" | "cc" | "cxx" => "\u{e61d}",  //
        "h" => "\u{e61e}",       //
        "hpp" | "hxx" => "\u{e61d}",  //
        "sh" | "bash" | "zsh" | "fish" => "\u{e795}",  //
        "lua" => "\u{e620}",     //
        "json" | "jsonc" | "json5" => "\u{e60b}",  //
        "yaml" | "yml" => "\u{e6a8}",  //
        "toml" => "\u{e6b2}",    //
        "xml" => "\u{f05c0}",    // 󰗀
        "html" | "htm" => "\u{e736}",  //
        "css" => "\u{e749}",     //
        "scss" | "sass" => "\u{e74b}",  //
        "md" | "mdx" => "\u{e73e}",  //
        "txt" => "\u{f0219}",    // 󰈙
        "lock" => "\u{e672}",    //
        "svg" => "\u{f0721}",    // 󰜡
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "bmp" => "\u{e60d}",  //
        "pdf" => "\u{e67d}",     //
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" => "\u{e6aa}",  //
        "sql" | "sqlite" | "sqlite3" => "\u{e706}",  //
        "log" => "\u{f04cb}",    // 󰓋
        "env" => "\u{e615}",     //
        "docker" => "\u{f0868}", // 󰡨
        _ => "\u{e612}",         //  (default file icon)
    }
}

/// Returns the Nerd Font icon for a directory.
pub fn dir_icon(expanded: bool) -> &'static str {
    if expanded {
        "\u{f0770}"  //  (open folder)
    } else {
        "\u{f024b}"  //  (closed folder)
    }
}
```

Create `src/explorer/mod.rs`:

```rust
pub mod icons;
```

Add `pub mod explorer;` to `src/lib.rs`.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add src/explorer/ src/lib.rs
git commit -m "feat(explorer): add Nerd Font icon lookup module"
```

---

### Task 2: Implement FileEntry and git status parsing

**Files:**
- Modify: `src/explorer/mod.rs`
- Create: `tests/explorer_test.rs`

- [ ] **Step 1: Write tests**

Create `tests/explorer_test.rs`:

```rust
use humu::explorer::{GitStatus, parse_git_status};
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn parse_porcelain_modified() {
    let output = " M src/app.rs\n";
    let status = parse_git_status(output);
    assert_eq!(status.get(&PathBuf::from("src/app.rs")), Some(&GitStatus::Modified));
}

#[test]
fn parse_porcelain_added() {
    let output = "A  src/new.rs\n";
    let status = parse_git_status(output);
    assert_eq!(status.get(&PathBuf::from("src/new.rs")), Some(&GitStatus::Added));
}

#[test]
fn parse_porcelain_untracked() {
    let output = "?? src/untracked.rs\n";
    let status = parse_git_status(output);
    assert_eq!(status.get(&PathBuf::from("src/untracked.rs")), Some(&GitStatus::Added));
}

#[test]
fn parse_porcelain_rename() {
    let output = "R  old.rs -> new.rs\n";
    let status = parse_git_status(output);
    assert_eq!(status.get(&PathBuf::from("new.rs")), Some(&GitStatus::Added));
    assert_eq!(status.get(&PathBuf::from("old.rs")), None);
}

#[test]
fn parse_porcelain_deleted_excluded() {
    let output = " D deleted.rs\n";
    let status = parse_git_status(output);
    assert!(status.is_empty());
}

#[test]
fn parse_porcelain_mixed() {
    let output = " M src/app.rs\nA  src/new.rs\n?? README.md\n D gone.rs\n";
    let status = parse_git_status(output);
    assert_eq!(status.len(), 3);
    assert_eq!(status.get(&PathBuf::from("src/app.rs")), Some(&GitStatus::Modified));
    assert_eq!(status.get(&PathBuf::from("src/new.rs")), Some(&GitStatus::Added));
    assert_eq!(status.get(&PathBuf::from("README.md")), Some(&GitStatus::Added));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test explorer_test`
Expected: FAIL — types not found

- [ ] **Step 3: Implement data model and git parsing**

Update `src/explorer/mod.rs`:

```rust
pub mod icons;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitStatus {
    Modified,
    Added,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub kind: FileKind,
    pub git_status: Option<GitStatus>,
    pub depth: usize,
    pub expanded: bool,
}

/// Parse `git status --porcelain` output into a map of relative paths to git status.
/// Deleted files are excluded.
pub fn parse_git_status(output: &str) -> HashMap<PathBuf, GitStatus> {
    let mut map = HashMap::new();
    for line in output.lines() {
        if line.len() < 4 {
            continue;
        }
        let xy = &line[..2];
        let path_part = &line[3..];

        // Skip deleted files
        if xy.contains('D') {
            continue;
        }

        // Handle renames: "R  old -> new"
        if xy.starts_with('R') || xy.starts_with('C') {
            if let Some(arrow_pos) = path_part.find(" -> ") {
                let new_path = &path_part[arrow_pos + 4..];
                map.insert(PathBuf::from(new_path), GitStatus::Added);
            }
            continue;
        }

        let path = PathBuf::from(path_part);

        match xy {
            "??" => { map.insert(path, GitStatus::Added); }
            _ if xy.starts_with('A') => { map.insert(path, GitStatus::Added); }
            _ if xy.contains('M') => { map.insert(path, GitStatus::Modified); }
            _ => { map.insert(path, GitStatus::Modified); }
        }
    }
    map
}

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

    /// Rescan the file tree from the root directory.
    pub fn scan(&mut self) {
        let git_status = self.run_git_status();
        let allowed_files = if !self.show_ignored {
            Some(self.run_git_ls_files())
        } else {
            None
        };
        self.entries.clear();
        self.build_tree(&self.root.clone(), 0, &git_status, &allowed_files);
        // Clamp selected index
        if !self.entries.is_empty() {
            self.selected = self.selected.min(self.entries.len() - 1);
        } else {
            self.selected = 0;
        }
    }

    fn build_tree(
        &mut self,
        dir: &Path,
        depth: usize,
        git_status: &HashMap<PathBuf, GitStatus>,
        allowed_files: &Option<HashSet<PathBuf>>,
    ) {
        let mut dirs = Vec::new();
        let mut files = Vec::new();

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let full_path = entry.path();
            let rel_path = full_path.strip_prefix(&self.root).unwrap_or(&full_path).to_path_buf();

            // Skip .git directory always
            if name == ".git" {
                continue;
            }

            let is_dir = full_path.is_dir();

            // Filter by allowed files (gitignore) — directories are allowed if
            // any of their children are allowed.
            if !is_dir {
                if let Some(allowed) = allowed_files {
                    if !allowed.contains(&rel_path) {
                        continue;
                    }
                }
            }

            let status = git_status.get(&rel_path).copied();

            if is_dir {
                dirs.push((name, rel_path, full_path));
            } else {
                files.push((name, rel_path, status));
            }
        }

        // Sort: alphabetical
        dirs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

        // Directories first
        for (name, rel_path, full_path) in dirs {
            let expanded = self.expanded_dirs.contains(&rel_path);
            // Propagate git status from children
            let dir_status = self.dir_git_status(&rel_path, git_status);

            // If filtering by allowed files, skip empty dirs
            if let Some(allowed) = allowed_files {
                if !self.dir_has_allowed_children(&full_path, allowed) {
                    continue;
                }
            }

            self.entries.push(FileEntry {
                name,
                path: rel_path.clone(),
                kind: FileKind::Directory,
                git_status: dir_status,
                depth,
                expanded,
            });

            if expanded {
                self.build_tree(&full_path, depth + 1, git_status, allowed_files);
            }
        }

        // Then files
        for (name, rel_path, status) in files {
            self.entries.push(FileEntry {
                name,
                path: rel_path,
                kind: FileKind::File,
                git_status: status,
                depth,
                expanded: false,
            });
        }
    }

    /// Check if a directory has any allowed (non-ignored) children recursively.
    fn dir_has_allowed_children(&self, dir: &Path, allowed: &HashSet<PathBuf>) -> bool {
        let prefix = dir.strip_prefix(&self.root).unwrap_or(dir);
        allowed.iter().any(|p| p.starts_with(prefix))
    }

    /// Propagate git status upward for a directory.
    fn dir_git_status(
        &self,
        dir_rel: &Path,
        git_status: &HashMap<PathBuf, GitStatus>,
    ) -> Option<GitStatus> {
        let mut result = None;
        for (path, status) in git_status {
            if path.starts_with(dir_rel) {
                match (result, status) {
                    (None, s) => result = Some(*s),
                    (Some(GitStatus::Added), GitStatus::Modified) => result = Some(GitStatus::Modified),
                    _ => {}
                }
            }
        }
        result
    }

    fn run_git_status(&self) -> HashMap<PathBuf, GitStatus> {
        let output = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.root)
            .output();
        match output {
            Ok(o) => parse_git_status(&String::from_utf8_lossy(&o.stdout)),
            Err(_) => HashMap::new(),
        }
    }

    fn run_git_ls_files(&self) -> HashSet<PathBuf> {
        let mut files = HashSet::new();
        // Tracked files
        if let Ok(o) = std::process::Command::new("git")
            .args(["ls-files"])
            .current_dir(&self.root)
            .output()
        {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                files.insert(PathBuf::from(line));
            }
        }
        // Untracked but not ignored
        if let Ok(o) = std::process::Command::new("git")
            .args(["ls-files", "--others", "--exclude-standard"])
            .current_dir(&self.root)
            .output()
        {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                files.insert(PathBuf::from(line));
            }
        }
        files
    }

    /// Toggle directory expand/collapse and rescan.
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

    /// Get the currently selected entry.
    pub fn selected_entry(&self) -> Option<&FileEntry> {
        self.entries.get(self.selected)
    }

    /// Move selection up.
    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Move selection down.
    pub fn move_down(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }

    /// Ensure selected item is within the visible scroll window.
    pub fn scroll_to_visible(&mut self, viewport_height: usize) {
        if viewport_height == 0 { return; }
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + viewport_height {
            self.scroll_offset = self.selected - viewport_height + 1;
        }
    }

    /// Check if delta is available (cached).
    pub fn check_delta(&mut self) -> bool {
        if let Some(available) = self.delta_available {
            return available;
        }
        let available = std::process::Command::new("which")
            .arg("delta")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        self.delta_available = Some(available);
        available
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test explorer_test`
Expected: all 6 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/explorer/mod.rs tests/explorer_test.rs
git commit -m "feat(explorer): add file tree data model with git status parsing"
```

---

## Chunk 2: Input system + Widget

### Task 3: Add Mode::Explorer and keybindings

**Files:**
- Modify: `src/tui/input.rs`

- [ ] **Step 1: Add Mode::Explorer to the enum**

Add `Explorer` variant to the `Mode` enum (after `Room`).

- [ ] **Step 2: Add Ctrl+E to handle_terminal()**

In `handle_terminal`, add in the Ctrl key match:

```rust
KeyCode::Char('e') => Action::EnterMode(Mode::Explorer),
```

- [ ] **Step 3: Add handle_explorer() function**

```rust
fn handle_explorer(key: KeyEvent) -> Action {
    if let Some(action) = check_mode_switch(Mode::Explorer, key) {
        return action;
    }
    if let Some(action) = check_shared_alt(key) {
        return action;
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        match key.code {
            KeyCode::Left => return Action::Resize(Direction::Left),
            KeyCode::Right => return Action::Resize(Direction::Right),
            KeyCode::Enter => return Action::DiffFile,
            KeyCode::Char('I') => return Action::ToggleIgnored,
            _ => {}
        }
    }
    match key.code {
        KeyCode::Down => Action::NavigateDown,
        KeyCode::Up => Action::NavigateUp,
        KeyCode::Enter => Action::Select,
        KeyCode::Esc => Action::EnterMode(Mode::Terminal),
        _ => Action::None,
    }
}
```

- [ ] **Step 4: Add new Action variants**

Add to the `Action` enum:

```rust
DiffFile,
ToggleIgnored,
```

- [ ] **Step 5: Wire handle_explorer into handle_key**

Add the match arm in `handle_key`:

```rust
Mode::Explorer => handle_explorer(key),
```

- [ ] **Step 6: Add Ctrl+E to check_mode_switch**

Add in `check_mode_switch()`:

```rust
KeyCode::Char('e') => Some(Action::EnterMode(Mode::Explorer)),
```

- [ ] **Step 7: Update hint_click_action for Explorer mode**

Add Explorer case to `hint_click_action()`:

```rust
Mode::Explorer => match hint_index {
    0 => None,                                        // ↑↓ Navigate
    1 => Some(Action::Select),                        // Enter Open
    2 => Some(Action::DiffFile),                      // S+Enter Diff
    3 => Some(Action::ToggleIgnored),                 // S+I Ignored
    4 => None,                                        // S+←→ Resize
    5 => Some(Action::EnterMode(Mode::Terminal)),     // Esc Back
    _ => None,
},
```

- [ ] **Step 8: Verify it compiles and all tests pass**

Run: `cargo build && cargo test`
Expected: compiles (unused variant warnings OK), tests pass

- [ ] **Step 9: Commit**

```bash
git add src/tui/input.rs
git commit -m "feat(input): add Mode::Explorer with keybindings and Ctrl+E"
```

---

### Task 4: Create ExplorerPanel widget

**Files:**
- Create: `src/tui/widgets/explorer_panel.rs`
- Modify: `src/tui/widgets/mod.rs` (if it exists, otherwise the widget is imported directly)

- [ ] **Step 1: Create the explorer panel widget**

Create `src/tui/widgets/explorer_panel.rs`:

```rust
use crate::explorer::{ExplorerState, FileKind, GitStatus};
use crate::explorer::icons;
use crate::tui::theme::{Palette, UiConfig};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Widget};

pub struct ExplorerPanel<'a> {
    state: &'a ExplorerState,
    palette: &'a Palette,
    ui_config: &'a UiConfig,
    focused: bool,
    show_ignored: bool,
}

impl<'a> ExplorerPanel<'a> {
    pub fn new(
        state: &'a ExplorerState,
        palette: &'a Palette,
        ui_config: &'a UiConfig,
    ) -> Self {
        Self {
            state,
            palette,
            ui_config,
            focused: false,
            show_ignored: state.show_ignored,
        }
    }

    pub fn focus(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl Widget for ExplorerPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_color = if self.focused {
            self.palette.accent_blue
        } else {
            self.palette.fg_muted
        };

        let title = if self.show_ignored {
            " Explorer [+ignored] "
        } else {
            " Explorer "
        };

        let bc = self.ui_config.border_chars();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(title)
            .title_style(Style::default().fg(border_color).add_modifier(Modifier::BOLD));
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let viewport_height = inner.height as usize;
        let start = self.state.scroll_offset;
        let end = (start + viewport_height).min(self.state.entries.len());

        for (i, entry) in self.state.entries[start..end].iter().enumerate() {
            let y = inner.y + i as u16;
            let is_selected = (start + i) == self.state.selected;

            // Background for selected line
            let line_bg = if is_selected {
                self.palette.bg_tertiary
            } else {
                self.palette.bg_primary
            };

            // Clear line
            for x in inner.x..inner.x + inner.width {
                buf[(x, y)].set_char(' ').set_style(Style::default().bg(line_bg));
            }

            let mut x = inner.x;
            let max_x = inner.x + inner.width;

            // Selection indicator
            if is_selected {
                if x < max_x {
                    buf[(x, y)].set_char('▸').set_style(
                        Style::default().fg(self.palette.accent_blue).bg(line_bg)
                    );
                }
            }
            x += 1; // space after indicator

            // Indentation
            let indent = entry.depth * 2;
            x += indent as u16;

            // Icon
            let icon = if entry.kind == FileKind::Directory {
                icons::dir_icon(entry.expanded)
            } else {
                icons::file_icon(&entry.name)
            };
            let icon_color = if entry.kind == FileKind::Directory {
                self.palette.fg_primary
            } else {
                self.palette.fg_secondary
            };
            for ch in icon.chars() {
                if x >= max_x { break; }
                buf[(x, y)].set_char(ch).set_style(
                    Style::default().fg(icon_color).bg(line_bg)
                );
                x += 1;
            }
            // Space after icon
            if x < max_x {
                x += 1;
            }

            // Filename
            let name_color = match entry.git_status {
                Some(GitStatus::Modified) => self.palette.accent_orange,
                Some(GitStatus::Added) => self.palette.accent_green,
                None => self.palette.fg_primary,
            };
            for ch in entry.name.chars() {
                if x >= max_x { break; }
                buf[(x, y)].set_char(ch).set_style(
                    Style::default().fg(name_color).bg(line_bg)
                );
                x += 1;
            }

            // Git status indicator
            if let Some(status) = entry.git_status {
                let (indicator, color) = match status {
                    GitStatus::Modified => (" ✗", self.palette.accent_orange),
                    GitStatus::Added => (" ★", self.palette.accent_green),
                };
                if x + 1 < max_x {
                    buf[(x, y)].set_char(' ').set_style(Style::default().bg(line_bg));
                    x += 1;
                }
                for ch in indicator.trim().chars() {
                    if x >= max_x { break; }
                    buf[(x, y)].set_char(ch).set_style(
                        Style::default().fg(color).bg(line_bg)
                    );
                    x += 1;
                }
            }
        }
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles (may have unused warnings — OK)

- [ ] **Step 3: Commit**

```bash
git add src/tui/widgets/explorer_panel.rs
git commit -m "feat(tui): add ExplorerPanel widget with icons and git status"
```

---

## Chunk 3: Layout + App Integration

### Task 5: Extend panel_widths and PanelRects

**Files:**
- Modify: `src/config.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Update panel_widths type with backward-compatible deserialization**

In `src/config.rs`, change `panel_widths` on `HumuState` from `Option<[u16; 2]>` to a custom type that deserializes both 2-element and 3-element arrays:

```rust
/// Panel widths: [workspace_panel, room_panel, explorer_panel]. Persisted across restarts.
/// Accepts both 2-element (legacy) and 3-element arrays for backward compatibility.
#[serde(default, deserialize_with = "deserialize_panel_widths")]
pub panel_widths: Option<[u16; 3]>,
```

Add the deserializer:

```rust
fn deserialize_panel_widths<'de, D>(deserializer: D) -> Result<Option<[u16; 3]>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<Vec<u16>> = Option::deserialize(deserializer)?;
    Ok(opt.map(|v| match v.len() {
        2 => [v[0], v[1], 25],
        3 => [v[0], v[1], v[2]],
        _ => [20, 18, 25],
    }))
}
```

- [ ] **Step 2: Update App struct and construction**

In `src/app.rs`:
- Change `panel_widths: [u16; 2]` to `panel_widths: [u16; 3]`
- Update default: `state.panel_widths.unwrap_or([20, 18, 25])`
- Add `explorer_state: humu::explorer::ExplorerState` field
- Construct `ExplorerState::new(root)` where root comes from `current_room_path().unwrap_or_default()`

Add `FocusedPanel::Explorer` variant to the enum.

Add `explorer: Rect` to `PanelRects`.

- [ ] **Step 3: Update render layout to 4 panels**

Change the layout constraints in `render()`:

```rust
let panel_chunks = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([
        Constraint::Length(self.panel_widths[0]),
        Constraint::Length(self.panel_widths[1]),
        Constraint::Min(1),
        Constraint::Length(self.panel_widths[2]),
    ])
    .split(main_chunks[0]);
```

Update `PanelRects` population:

```rust
let tab_bar_rect = Rect::new(panel_chunks[2].x, panel_chunks[2].y, panel_chunks[2].width, 1);
self.panel_rects = PanelRects {
    workspace: panel_chunks[0],
    room: panel_chunks[1],
    terminal: panel_chunks[2],
    explorer: panel_chunks[3],
    tab_bar: tab_bar_rect,
    status_bar: main_chunks[1],
};
```

Add explorer panel rendering:

```rust
use humu::tui::widgets::explorer_panel::ExplorerPanel;

// Explorer panel
let explorer_widget = ExplorerPanel::new(
    &self.explorer_state,
    &self.palette,
    &self.ui_config,
).focus(self.focus == FocusedPanel::Explorer);
frame.render_widget(explorer_widget, self.panel_rects.explorer);
```

- [ ] **Step 4: Update all exhaustive FocusedPanel matches**

For every `match self.focus { ... }` that handles `Workspace`, `Room`, `Terminal`, add `Explorer` arm. Key locations:

- `handle_resize_action()`: Explorer → resize `panel_widths[2]` with Shift+Left/Right, clamped 5-60
- `show_create_dialog()`: Explorer → no-op (no create from explorer)
- `show_delete_dialog()`: Explorer → no-op
- `navigate()`: Explorer → `self.explorer_state.move_up()` / `move_down()`
- `select_current()`: Explorer → handle enter (toggle dir or open file)
- `handle_action(EnterMode)`: `Mode::Explorer => self.focus = FocusedPanel::Explorer` + trigger rescan

- [ ] **Step 5: Add explorer click handling**

In `handle_click()`, add before the tab_bar check:

```rust
else if self.panel_rects.explorer.contains(pos) {
    self.mode = Mode::Explorer;
    self.focus = FocusedPanel::Explorer;
}
```

- [ ] **Step 6: Handle Explorer-specific actions in handle_action**

Add to `handle_action()`:

```rust
Action::DiffFile => { self.explorer_diff_file(); }
Action::ToggleIgnored => {
    self.explorer_state.show_ignored = !self.explorer_state.show_ignored;
    self.explorer_state.scan();
}
```

- [ ] **Step 7: Implement explorer_open_file and explorer_diff_file**

```rust
fn spawn_command(&mut self, cmd: &str, args: &[String], cwd: &Path, preset_name: &str) -> Option<PaneId> {
    let id = PaneId::new();
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let pane = PtyPane::spawn_with_envs(cmd, &arg_refs, Some(cwd), 80, 24, &[]).ok()?;
    self.panes.insert(id, pane);
    self.pane_presets.insert(id, preset_name.to_string());
    Some(id)
}

fn explorer_open_file(&mut self) {
    let entry = match self.explorer_state.selected_entry() {
        Some(e) if e.kind == FileKind::File => e.clone(),
        _ => return,
    };
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let cwd = self.explorer_state.root.clone();
    let filepath = cwd.join(&entry.path);
    let args = vec![filepath.to_string_lossy().into_owned()];
    if let Some(id) = self.spawn_command(&editor, &args, &cwd, "_editor") {
        if let Some(tree) = self.tabs.active_tree_mut() {
            if let Some(focused) = self.focused_pane {
                tree.split_horizontal(focused, id);
            }
        }
        self.focused_pane = Some(id);
        self.mode = Mode::Terminal;
        self.focus = FocusedPanel::Terminal;
        self.persist_layout();
    }
}

fn explorer_diff_file(&mut self) {
    let entry = match self.explorer_state.selected_entry() {
        Some(e) if e.kind == FileKind::File && e.git_status == Some(GitStatus::Modified) => e.clone(),
        _ => return,
    };
    if !self.explorer_state.check_delta() {
        self.last_error = Some("delta not installed — install from https://github.com/dandavison/delta".to_string());
        return;
    }
    let cwd = self.explorer_state.root.clone();
    let diff_cmd = format!("git diff {} | delta --paging=always", entry.path.display());
    let args = vec!["-c".to_string(), diff_cmd];
    if let Some(id) = self.spawn_command("sh", &args, &cwd, "_diff") {
        if let Some(tree) = self.tabs.active_tree_mut() {
            if let Some(focused) = self.focused_pane {
                tree.split_horizontal(focused, id);
            }
        }
        self.focused_pane = Some(id);
        self.mode = Mode::Terminal;
        self.focus = FocusedPanel::Terminal;
        self.persist_layout();
    }
}
```

- [ ] **Step 8: Trigger rescan when entering Explorer mode**

In the `EnterMode(Mode::Explorer)` handler, after setting focus:

```rust
Mode::Explorer => {
    self.focus = FocusedPanel::Explorer;
    // Update root and rescan
    if let Some(path) = self.current_room_path() {
        if self.explorer_state.root != path {
            self.explorer_state = humu::explorer::ExplorerState::new(path);
        }
        self.explorer_state.scan();
    }
}
```

- [ ] **Step 9: Reset explorer on workspace/room switch**

In `switch_to_selected_room()`, after restoring the room, reset the explorer:

```rust
if let Some(path) = self.current_room_path() {
    self.explorer_state = humu::explorer::ExplorerState::new(path);
}
```

- [ ] **Step 10: Verify it compiles and all tests pass**

Run: `cargo build && cargo test`
Expected: compiles, all tests PASS

- [ ] **Step 11: Commit**

```bash
git add src/config.rs src/app.rs src/tui/widgets/explorer_panel.rs
git commit -m "feat(app): integrate file explorer panel with 4-panel layout"
```

---

### Task 6: Update status bar and theme for Explorer mode

**Files:**
- Modify: `src/tui/widgets/status_bar.rs`
- Modify: `src/tui/theme.rs`

- [ ] **Step 1: Add Explorer mode color**

In `src/tui/theme.rs`, add to `mode_color()`:

```rust
Mode::Explorer => self.accent_yellow,
```

- [ ] **Step 2: Add Explorer mode label and hints**

In `src/tui/widgets/status_bar.rs`:

Add to `mode_label()`:
```rust
Mode::Explorer => "EXPLORER",
```

Add to `mode_hints()`:
```rust
Mode::Explorer => vec![
    ("\u{2191}\u{2193}", "Navigate"),
    ("Enter", "Open"),
    ("S+Enter", "Diff"),
    ("S+I", "Ignored"),
    ("S+\u{2190}\u{2192}", "Resize"),
    ("Esc", "Back"),
],
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: compiles

- [ ] **Step 4: Commit**

```bash
git add src/tui/widgets/status_bar.rs src/tui/theme.rs
git commit -m "feat(tui): add Explorer mode to status bar and theme"
```

---

### Task 7: Update documentation

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Add explorer module to project structure**

Add to the project structure in `CLAUDE.md`:

```
├── explorer/
│   ├── mod.rs       # ExplorerState, FileEntry, tree scan/toggle operations
│   └── icons.rs     # Nerd Font file extension icon lookup
```

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: add explorer module to project structure"
```

---

### Task 8: Manual integration test

- [ ] **Step 1: Build and run**

Run: `cargo build && cargo run`

- [ ] **Step 2: Verify explorer panel appears**

1. Confirm layout is `[Workspace | Room | Terminal | Explorer]`
2. Explorer panel shows on the right with directory tree
3. Nerd Font icons display correctly

- [ ] **Step 3: Test navigation**

1. Press `Ctrl+E` to enter Explorer mode
2. `↑/↓` navigates the tree
3. `Enter` on a directory expands/collapses
4. `Shift+←/→` resizes the panel

- [ ] **Step 4: Test file actions**

1. `Enter` on a file opens `$EDITOR` in a new pane
2. Modify a file, rescan (`Ctrl+E` re-enter)
3. `Shift+Enter` on modified file opens delta diff

- [ ] **Step 5: Test git status**

1. Modified files show `✗` in orange
2. New untracked files show `★` in green
3. Parent directories inherit child status

- [ ] **Step 6: Test gitignore toggle**

1. `Shift+I` shows previously hidden files (node_modules, target/, etc.)
2. Panel title changes to `" Explorer [+ignored] "`
3. `Shift+I` again hides them
