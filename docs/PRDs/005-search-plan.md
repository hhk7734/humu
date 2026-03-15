# Search Feature Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add terminal pane search with regex, scrollback scanning, match highlighting, and navigation.

**Architecture:** Two new modes (EnterSearch, Search) added to the modal system. A `SearchState` struct on `App` holds the query, matches, and options. A new `search` module handles text extraction from the vt100 parser and regex matching. `TerminalWidget` accepts search matches for overlay highlighting. `StatusBar` renders the search input bar and match counter.

**Tech Stack:** regex crate, vt100 0.15 (scrollback access via `Parser::set_scrollback`), ratatui 0.29

**Spec:** `docs/PRDs/005-search.md`

---

## File Structure

| File | Role |
|------|------|
| `Cargo.toml` | Add `regex` dependency |
| `src/tui/input.rs` | Add `EnterSearch`/`Search` modes, new `Action` variants, keybinding handlers |
| `src/tui/theme.rs` | Add `accent_cyan` to `Palette`, extend `mode_color` |
| `src/tui/search.rs` | New: `SearchState`, `SearchMatch`, text extraction, regex search engine |
| `src/tui/mod.rs` | Add `pub mod search;` |
| `src/tui/widgets/status_bar.rs` | Render search input bar and match counter for search modes |
| `src/tui/widgets/terminal_widget.rs` | Accept and render match highlights |
| `src/app.rs` | Wire search state, handle search actions, clear on room/mode switch |
| `tests/input_test.rs` | Tests for new modes and keybindings |
| `tests/search_test.rs` | New: tests for search engine (text extraction, regex, case toggle) |

---

## Chunk 1: Foundation — Modes, Actions, Theme

### Task 1: Add EnterSearch/Search modes and search actions to input.rs

**Files:**
- Modify: `src/tui/input.rs`
- Test: `tests/input_test.rs`

- [ ] **Step 1: Add modes and actions to input.rs**

Add `EnterSearch` and `Search` to the `Mode` enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Terminal,
    Locked,
    Pane,
    Tab,
    Workspace,
    Room,
    EnterSearch,
    Search,
}
```

Add new `Action` variants:

```rust
pub enum Action {
    // ... existing variants ...
    SearchInput(KeyEvent),
    SearchConfirm,
    SearchCancel,
    SearchNext,
    SearchPrev,
    SearchToggleCase,
    SearchToggleWrap,
    ScrollUp,
    ScrollDown,
    ScrollPageUp,
    ScrollPageDown,
}
```

- [ ] **Step 2: Add Ctrl+f to handle_terminal**

In `handle_terminal`, add `Ctrl+f` before the existing Ctrl matches:

```rust
KeyCode::Char('f') => Action::EnterMode(Mode::EnterSearch),
```

- [ ] **Step 3: Add handle_enter_search and handle_search functions**

```rust
fn handle_enter_search(key: KeyEvent) -> Action {
    if let Some(action) = check_mode_switch(Mode::EnterSearch, key) {
        return action;
    }
    match key.code {
        KeyCode::Enter => Action::SearchConfirm,
        KeyCode::Esc => Action::SearchCancel,
        _ => Action::SearchInput(key),
    }
}

fn handle_search(key: KeyEvent) -> Action {
    if let Some(action) = check_mode_switch(Mode::Search, key) {
        return action;
    }
    match key.code {
        KeyCode::Char('n') => Action::SearchNext,
        KeyCode::Char('N') => Action::SearchPrev,
        KeyCode::Char('c') => Action::SearchToggleCase,
        KeyCode::Char('w') => Action::SearchToggleWrap,
        KeyCode::Up => Action::ScrollUp,
        KeyCode::Down => Action::ScrollDown,
        KeyCode::PageUp => Action::ScrollPageUp,
        KeyCode::PageDown => Action::ScrollPageDown,
        KeyCode::Esc => Action::SearchCancel,
        _ => Action::None,
    }
}
```

- [ ] **Step 4: Wire new modes in handle_key**

```rust
Mode::EnterSearch => handle_enter_search(key),
Mode::Search => handle_search(key),
```

- [ ] **Step 5: Write tests for new keybindings**

In `tests/input_test.rs`, add:

```rust
#[test]
fn terminal_ctrl_f_enters_search() {
    let action = handle_key(Mode::Terminal, ctrl('f'));
    assert!(matches!(action, Action::EnterMode(Mode::EnterSearch)));
}

#[test]
fn enter_search_enter_confirms() {
    let action = handle_key(Mode::EnterSearch, key(KeyCode::Enter));
    assert!(matches!(action, Action::SearchConfirm));
}

#[test]
fn enter_search_esc_cancels() {
    let action = handle_key(Mode::EnterSearch, key(KeyCode::Esc));
    assert!(matches!(action, Action::SearchCancel));
}

#[test]
fn enter_search_char_is_input() {
    let action = handle_key(Mode::EnterSearch, key(KeyCode::Char('a')));
    assert!(matches!(action, Action::SearchInput(_)));
}

#[test]
fn enter_search_ctrl_w_switches_mode() {
    let action = handle_key(Mode::EnterSearch, ctrl('w'));
    assert!(matches!(action, Action::EnterMode(Mode::Workspace)));
}

#[test]
fn search_n_goes_next() {
    let action = handle_key(Mode::Search, key(KeyCode::Char('n')));
    assert!(matches!(action, Action::SearchNext));
}

#[test]
fn search_shift_n_goes_prev() {
    let k = KeyEvent {
        code: KeyCode::Char('N'),
        modifiers: KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    let action = handle_key(Mode::Search, k);
    assert!(matches!(action, Action::SearchPrev));
}

#[test]
fn search_c_toggles_case() {
    let action = handle_key(Mode::Search, key(KeyCode::Char('c')));
    assert!(matches!(action, Action::SearchToggleCase));
}

#[test]
fn search_w_toggles_wrap() {
    let action = handle_key(Mode::Search, key(KeyCode::Char('w')));
    assert!(matches!(action, Action::SearchToggleWrap));
}

#[test]
fn search_esc_cancels() {
    let action = handle_key(Mode::Search, key(KeyCode::Esc));
    assert!(matches!(action, Action::SearchCancel));
}

#[test]
fn search_arrows_scroll() {
    assert!(matches!(handle_key(Mode::Search, key(KeyCode::Up)), Action::ScrollUp));
    assert!(matches!(handle_key(Mode::Search, key(KeyCode::Down)), Action::ScrollDown));
    assert!(matches!(handle_key(Mode::Search, key(KeyCode::PageUp)), Action::ScrollPageUp));
    assert!(matches!(handle_key(Mode::Search, key(KeyCode::PageDown)), Action::ScrollPageDown));
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test`
Expected: all tests pass (new tests + existing tests compile with new Mode/Action variants)

- [ ] **Step 7: Commit**

```bash
git add src/tui/input.rs tests/input_test.rs
git commit -m "feat(input): add EnterSearch/Search modes and search actions"
```

### Task 2: Extend theme with accent_cyan and mode_color for search modes

**Files:**
- Modify: `src/tui/theme.rs`
- Modify: `src/tui/widgets/status_bar.rs` (add mode_label and mode_hints for new modes)

- [ ] **Step 1: Add accent_cyan to Palette**

In `src/tui/theme.rs`, add field to `Palette` struct and `GITHUB_DARK`:

```rust
pub struct Palette {
    // ... existing fields ...
    pub accent_cyan: Color,
}

pub const GITHUB_DARK: Self = Self {
    // ... existing values ...
    accent_cyan: Color::Rgb(86, 212, 221),
};
```

- [ ] **Step 2: Extend mode_color**

```rust
pub fn mode_color(&self, mode: &Mode) -> Color {
    match mode {
        // ... existing arms ...
        Mode::EnterSearch | Mode::Search => self.accent_cyan,
    }
}
```

- [ ] **Step 3: Add mode_label and mode_hints for search modes in status_bar.rs**

In `StatusBar::mode_label`:

```rust
Mode::EnterSearch | Mode::Search => "SEARCH",
```

In `StatusBar::mode_hints`: return empty vec for now (search status bar will be handled separately in Task 5):

```rust
Mode::EnterSearch | Mode::Search => vec![],
```

- [ ] **Step 4: Build to verify all exhaustive matches compile**

Run: `cargo build`
Expected: compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add src/tui/theme.rs src/tui/widgets/status_bar.rs
git commit -m "feat(theme): add accent_cyan and search mode colors"
```

---

## Chunk 2: Search Engine

### Task 3: Add regex dependency

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add regex to Cargo.toml**

```bash
cargo add regex
```

- [ ] **Step 2: Verify build**

Run: `cargo build`
Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "build: add regex dependency"
```

### Task 4: Create search module with SearchState and search engine

**Files:**
- Create: `src/tui/search.rs`
- Modify: `src/tui/mod.rs`
- Test: `tests/search_test.rs`

- [ ] **Step 1: Create src/tui/search.rs with data types**

```rust
use regex::Regex;

#[derive(Debug, Clone)]
pub struct SearchMatch {
    /// Absolute row (0 = oldest scrollback line).
    pub row: usize,
    pub col_start: usize,
    pub col_end: usize,
}

pub struct SearchState {
    pub query: String,
    pub matches: Vec<SearchMatch>,
    pub active_index: Option<usize>,
    pub case_sensitive: bool,
    pub wrap: bool,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            matches: Vec::new(),
            active_index: None,
            case_sensitive: true,
            wrap: false,
        }
    }

    /// Re-run search against the given rows of text.
    /// Each element in `rows` is a (row_text, col_offsets) pair where
    /// col_offsets[byte_index] = column_index.
    pub fn execute(&mut self, rows: &[(String, Vec<usize>)]) {
        self.matches.clear();
        self.active_index = None;

        let pattern = if self.case_sensitive {
            self.query.clone()
        } else {
            format!("(?i){}", self.query)
        };

        let re = match Regex::new(&pattern) {
            Ok(re) => re,
            Err(_) => return, // Invalid regex — no matches, no panic.
        };

        for (abs_row, (text, col_offsets)) in rows.iter().enumerate() {
            for mat in re.find_iter(text) {
                let byte_start = mat.start();
                let byte_end = mat.end();
                // Map byte offsets to column indices.
                let col_start = col_offsets.get(byte_start).copied().unwrap_or(0);
                let col_end = if byte_end > 0 {
                    col_offsets
                        .get(byte_end)
                        .copied()
                        .unwrap_or_else(|| col_offsets.last().copied().unwrap_or(0) + 1)
                } else {
                    col_start
                };
                self.matches.push(SearchMatch {
                    row: abs_row,
                    col_start,
                    col_end,
                });
            }
        }

        if !self.matches.is_empty() {
            self.active_index = Some(0);
        }
    }

    /// Move to the next match. Returns true if the active match changed.
    pub fn next(&mut self) -> bool {
        let len = self.matches.len();
        if len == 0 {
            return false;
        }
        match self.active_index {
            Some(i) if i + 1 < len => {
                self.active_index = Some(i + 1);
                true
            }
            Some(_) if self.wrap => {
                self.active_index = Some(0);
                true
            }
            None => {
                self.active_index = Some(0);
                true
            }
            _ => false,
        }
    }

    /// Move to the previous match. Returns true if the active match changed.
    pub fn prev(&mut self) -> bool {
        let len = self.matches.len();
        if len == 0 {
            return false;
        }
        match self.active_index {
            Some(0) if self.wrap => {
                self.active_index = Some(len - 1);
                true
            }
            Some(i) if i > 0 => {
                self.active_index = Some(i - 1);
                true
            }
            None => {
                self.active_index = Some(len - 1);
                true
            }
            _ => false,
        }
    }

    /// Return the active match, if any.
    pub fn active_match(&self) -> Option<&SearchMatch> {
        self.active_index.and_then(|i| self.matches.get(i))
    }

    /// Returns true if the query is a valid regex.
    pub fn is_valid_regex(&self) -> bool {
        let pattern = if self.case_sensitive {
            self.query.clone()
        } else {
            format!("(?i){}", self.query)
        };
        Regex::new(&pattern).is_ok()
    }
}

/// Extract all text from a vt100 parser, returning (text, byte-to-col mapping) per row.
/// Row 0 = oldest scrollback line.
///
/// Probes scrollback depth via set_scrollback(usize::MAX), then steps through
/// viewport-sized chunks from max to 0, building text cell-by-cell.
pub fn extract_rows(parser: &std::sync::Arc<std::sync::Mutex<vt100::Parser>>) -> Vec<(String, Vec<usize>)> {
    let mut parser_guard = parser.lock().unwrap();

    // Probe scrollback depth.
    let original_offset = parser_guard.screen().scrollback();
    parser_guard.set_scrollback(usize::MAX);
    let max_offset = parser_guard.screen().scrollback();
    parser_guard.set_scrollback(original_offset);

    let (screen_rows, screen_cols) = parser_guard.screen().size();
    let screen_rows = screen_rows as usize;
    let screen_cols = screen_cols as usize;
    let total_rows = max_offset + screen_rows;

    let mut all_rows: Vec<(String, Vec<usize>)> = Vec::with_capacity(total_rows);

    // Step from max_offset down to 0.
    let mut offset = max_offset;
    loop {
        parser_guard.set_scrollback(offset);
        let screen = parser_guard.screen().clone();

        let rows_to_read = if offset == max_offset {
            screen_rows // first chunk: full viewport
        } else {
            // subsequent chunks: only read the newly revealed rows at the bottom
            screen_rows.min(offset + screen_rows)
        };

        // For the first chunk, read all rows. For subsequent chunks, we only
        // need the bottom rows that weren't visible at the previous offset.
        let start_row = if offset == max_offset {
            0
        } else {
            // The top (screen_rows - step) rows overlap with the previous chunk.
            // We only want the new rows at the bottom.
            let step = if offset + screen_rows <= max_offset {
                screen_rows
            } else {
                max_offset - offset
            };
            screen_rows - step.min(screen_rows)
        };

        for row_idx in start_row..screen_rows {
            let mut text = String::new();
            let mut col_offsets = Vec::new();
            for col in 0..screen_cols {
                col_offsets.push(text.len());
                if let Some(cell) = screen.cell(row_idx as u16, col as u16) {
                    let contents = cell.contents();
                    if contents.is_empty() {
                        text.push(' ');
                    } else {
                        text.push_str(&contents);
                    }
                } else {
                    text.push(' ');
                }
            }
            // Sentinel for end-of-row byte offset mapping.
            col_offsets.push(text.len());
            all_rows.push((text, col_offsets));
        }

        if offset == 0 {
            break;
        }
        offset = offset.saturating_sub(screen_rows);
    }

    // Restore original scrollback offset.
    parser_guard.set_scrollback(original_offset);

    all_rows
}
```

- [ ] **Step 2: Add pub mod search to src/tui/mod.rs**

```rust
pub mod search;
```

- [ ] **Step 3: Add PtyPane::parser_ref() accessor**

In `src/pty/pane.rs`, add a method to expose the parser Arc for search to use:

```rust
/// Returns a reference to the parser Arc for search operations.
pub fn parser_ref(&self) -> &std::sync::Arc<std::sync::Mutex<vt100::Parser>> {
    &self.parser
}
```

- [ ] **Step 4: Write tests in tests/search_test.rs**

```rust
use humu::tui::search::{SearchState, SearchMatch};

#[test]
fn search_literal_substring() {
    let rows = vec![
        ("hello world".to_string(), (0..12).collect()),
        ("foo bar".to_string(), (0..8).collect()),
    ];
    let mut state = SearchState::new();
    state.query = "world".to_string();
    state.execute(&rows);
    assert_eq!(state.matches.len(), 1);
    assert_eq!(state.matches[0].row, 0);
    assert_eq!(state.matches[0].col_start, 6);
    assert_eq!(state.matches[0].col_end, 11);
}

#[test]
fn search_regex_pattern() {
    let rows = vec![
        ("error: file not found".to_string(), (0..22).collect()),
        ("warning: deprecated".to_string(), (0..20).collect()),
        ("error: timeout".to_string(), (0..15).collect()),
    ];
    let mut state = SearchState::new();
    state.query = "error:.*".to_string();
    state.execute(&rows);
    assert_eq!(state.matches.len(), 2);
    assert_eq!(state.matches[0].row, 0);
    assert_eq!(state.matches[1].row, 2);
}

#[test]
fn search_case_insensitive() {
    let rows = vec![
        ("Hello World".to_string(), (0..12).collect()),
        ("hello world".to_string(), (0..12).collect()),
    ];
    let mut state = SearchState::new();
    state.query = "hello".to_string();
    state.case_sensitive = false;
    state.execute(&rows);
    assert_eq!(state.matches.len(), 2);
}

#[test]
fn search_case_sensitive() {
    let rows = vec![
        ("Hello World".to_string(), (0..12).collect()),
        ("hello world".to_string(), (0..12).collect()),
    ];
    let mut state = SearchState::new();
    state.query = "hello".to_string();
    state.case_sensitive = true;
    state.execute(&rows);
    assert_eq!(state.matches.len(), 1);
    assert_eq!(state.matches[0].row, 1);
}

#[test]
fn search_invalid_regex_no_panic() {
    let rows = vec![("test".to_string(), (0..5).collect())];
    let mut state = SearchState::new();
    state.query = "[invalid".to_string();
    state.execute(&rows);
    assert_eq!(state.matches.len(), 0);
    assert!(!state.is_valid_regex());
}

#[test]
fn search_next_prev_navigation() {
    let rows = vec![
        ("aaa".to_string(), (0..4).collect()),
        ("aaa".to_string(), (0..4).collect()),
        ("aaa".to_string(), (0..4).collect()),
    ];
    let mut state = SearchState::new();
    state.query = "a".to_string();
    state.execute(&rows);
    // 3 rows x 3 matches each = 9 matches
    assert_eq!(state.active_index, Some(0));
    state.next();
    assert_eq!(state.active_index, Some(1));
    state.prev();
    assert_eq!(state.active_index, Some(0));
}

#[test]
fn search_wrap_navigation() {
    let rows = vec![("ab".to_string(), vec![0, 1, 2])];
    let mut state = SearchState::new();
    state.query = "a".to_string();
    state.wrap = true;
    state.execute(&rows);
    assert_eq!(state.matches.len(), 1);
    assert_eq!(state.active_index, Some(0));
    // next wraps around
    assert!(!state.next()); // only 1 match, wraps to same
    // prev wraps around
    assert!(!state.prev());
}

#[test]
fn search_no_wrap_stops() {
    let rows = vec![("ab".to_string(), vec![0, 1, 2])];
    let mut state = SearchState::new();
    state.query = "a".to_string();
    state.wrap = false;
    state.execute(&rows);
    assert_eq!(state.active_index, Some(0));
    assert!(!state.next()); // only 1 match, stops
}

#[test]
fn search_empty_query() {
    let rows = vec![("hello".to_string(), (0..6).collect())];
    let mut state = SearchState::new();
    state.query = String::new();
    state.execute(&rows);
    assert_eq!(state.matches.len(), 0);
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test`
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/tui/search.rs src/tui/mod.rs src/pty/pane.rs tests/search_test.rs
git commit -m "feat(search): add search engine with regex, navigation, and text extraction"
```

---

## Chunk 3: App Wiring and Rendering

### Task 5: Wire search state into App and handle search actions

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add search_state field to App**

```rust
use humu::tui::search::SearchState;

// In App struct:
pub search_state: Option<SearchState>,
```

Initialize as `None` in `App::new()`.

- [ ] **Step 2: Handle search actions in handle_action**

Add these match arms to `handle_action`:

```rust
Action::EnterMode(Mode::EnterSearch) => {
    self.mode = Mode::EnterSearch;
    self.focus = FocusedPanel::Terminal;
    self.search_state = Some(SearchState::new());
}
Action::SearchInput(key) => {
    if let Some(ref mut state) = self.search_state {
        match key.code {
            KeyCode::Char(c) => {
                state.query.push(c);
                self.run_search();
            }
            KeyCode::Backspace => {
                state.query.pop();
                self.run_search();
            }
            _ => {}
        }
    }
}
Action::SearchConfirm => {
    if let Some(ref state) = self.search_state {
        if state.query.is_empty() {
            self.search_state = None;
            self.mode = Mode::Terminal;
        } else {
            self.mode = Mode::Search;
        }
    }
}
Action::SearchCancel => {
    self.search_state = None;
    self.mode = Mode::Terminal;
}
Action::SearchNext => {
    if let Some(ref mut state) = self.search_state {
        if state.next() {
            self.scroll_to_active_match();
        }
    }
}
Action::SearchPrev => {
    if let Some(ref mut state) = self.search_state {
        if state.prev() {
            self.scroll_to_active_match();
        }
    }
}
Action::SearchToggleCase => {
    if let Some(ref mut state) = self.search_state {
        state.case_sensitive = !state.case_sensitive;
        self.run_search();
    }
}
Action::SearchToggleWrap => {
    if let Some(ref mut state) = self.search_state {
        state.wrap = !state.wrap;
    }
}
Action::ScrollUp => {
    if let Some(pane_id) = self.focused_pane {
        if let Some(pane) = self.panes.get(&pane_id) {
            let current = pane.scrollback();
            pane.set_scrollback(current.saturating_add(1));
        }
    }
}
Action::ScrollDown => {
    if let Some(pane_id) = self.focused_pane {
        if let Some(pane) = self.panes.get(&pane_id) {
            let current = pane.scrollback();
            pane.set_scrollback(current.saturating_sub(1));
        }
    }
}
Action::ScrollPageUp => {
    if let Some(pane_id) = self.focused_pane {
        if let Some(pane) = self.panes.get(&pane_id) {
            let page = pane.rows() as usize;
            let current = pane.scrollback();
            pane.set_scrollback(current.saturating_add(page));
        }
    }
}
Action::ScrollPageDown => {
    if let Some(pane_id) = self.focused_pane {
        if let Some(pane) = self.panes.get(&pane_id) {
            let page = pane.rows() as usize;
            let current = pane.scrollback();
            pane.set_scrollback(current.saturating_sub(page));
        }
    }
}
```

- [ ] **Step 3: Update EnterMode handler to clear search on mode switch**

In the existing `Action::EnterMode(mode)` arm, add search cleanup:

```rust
Action::EnterMode(mode) => {
    // Clear search state when switching away from search modes.
    if self.mode == Mode::EnterSearch || self.mode == Mode::Search {
        if mode != Mode::EnterSearch && mode != Mode::Search {
            self.search_state = None;
        }
    }
    self.mode = mode;
    match mode {
        Mode::Workspace => self.focus = FocusedPanel::Workspace,
        Mode::Room => self.focus = FocusedPanel::Room,
        Mode::Terminal | Mode::Pane | Mode::Tab | Mode::Locked
        | Mode::EnterSearch | Mode::Search => {
            self.focus = FocusedPanel::Terminal;
        }
    }
}
```

- [ ] **Step 4: Add run_search and scroll_to_active_match helper methods**

```rust
fn run_search(&mut self) {
    let pane_id = match self.focused_pane {
        Some(id) => id,
        None => return,
    };
    let pane = match self.panes.get(&pane_id) {
        Some(p) => p,
        None => return,
    };
    let rows = humu::tui::search::extract_rows(pane.parser_ref());
    if let Some(ref mut state) = self.search_state {
        state.execute(&rows);
        // Scroll to first match near current viewport.
        self.scroll_to_active_match();
    }
}

fn scroll_to_active_match(&mut self) {
    let active = match self.search_state.as_ref().and_then(|s| s.active_match()) {
        Some(m) => m.row,
        None => return,
    };
    let pane_id = match self.focused_pane {
        Some(id) => id,
        None => return,
    };
    let pane = match self.panes.get(&pane_id) {
        Some(p) => p,
        None => return,
    };
    // Probe max_offset to compute absolute row → scrollback offset.
    let parser = pane.parser_ref();
    let mut guard = parser.lock().unwrap();
    let original = guard.screen().scrollback();
    guard.set_scrollback(usize::MAX);
    let max_offset = guard.screen().scrollback();
    guard.set_scrollback(original);
    let screen_rows = guard.screen().size().0 as usize;
    drop(guard);

    // target_offset centers the match row in the viewport.
    let target_offset = if active < max_offset {
        (max_offset - active).saturating_sub(screen_rows / 2)
    } else {
        0
    };
    let target_offset = target_offset.min(max_offset);
    pane.set_scrollback(target_offset);
}
```

- [ ] **Step 5: Clear search state on room/workspace switch**

In `suspend_current_room`, add:

```rust
self.search_state = None;
```

- [ ] **Step 6: Build and test**

Run: `cargo build && cargo test`
Expected: compiles and all tests pass

- [ ] **Step 7: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): wire search state, actions, and scrollback navigation"
```

### Task 6: Render search status bar and match highlighting

**Files:**
- Modify: `src/tui/widgets/status_bar.rs`
- Modify: `src/tui/widgets/terminal_widget.rs`
- Modify: `src/app.rs` (pass search state to widgets)

- [ ] **Step 1: Add search_query and search_info fields to StatusBar**

In `src/tui/widgets/status_bar.rs`, add fields and builder methods:

```rust
pub struct StatusBar<'a> {
    // ... existing fields ...
    search_query: Option<&'a str>,
    search_info: Option<(usize, usize, bool, bool)>, // (active_idx_1based, total, case_sensitive, wrap)
    search_valid: bool,
}
```

Add builder methods:

```rust
pub fn search_query(mut self, query: Option<&'a str>) -> Self {
    self.search_query = query;
    self
}

pub fn search_info(mut self, info: Option<(usize, usize, bool, bool)>) -> Self {
    self.search_info = info;
    self
}

pub fn search_valid(mut self, valid: bool) -> Self {
    self.search_valid = valid;
    self
}
```

Initialize all as `None`/`true` in `new()`.

- [ ] **Step 2: Render search bar in EnterSearch mode**

In `StatusBar::render`, after the mode badge + separator, add a branch for search modes. When `self.mode == Mode::EnterSearch`:

```rust
if self.mode == Mode::EnterSearch {
    if let Some(query) = self.search_query {
        let prefix = " / ";
        let cursor = "\u{2588}"; // █
        let invalid_msg = if !self.search_valid { "  [invalid regex]" } else { "" };
        let display = format!("{}{}{}{}", prefix, query, cursor, invalid_msg);
        for (i, ch) in display.chars().enumerate() {
            if x + i as u16 >= area.x + area.width { break; }
            let style = if !self.search_valid && i >= prefix.len() + query.len() + cursor.len() {
                Style::default().fg(self.palette.accent_red).bg(self.palette.bg_secondary)
            } else {
                Style::default().fg(self.palette.fg_primary).bg(self.palette.bg_secondary)
            };
            buf[(x + i as u16, area.y)].set_char(ch).set_style(style);
        }
    }
    return;
}
```

- [ ] **Step 3: Render search info in Search mode**

When `self.mode == Mode::Search`:

```rust
if self.mode == Mode::Search {
    if let Some(query) = self.search_query {
        let prefix = format!(" / {} ", query);
        for (i, ch) in prefix.chars().enumerate() {
            if x + i as u16 >= area.x + area.width { break; }
            buf[(x + i as u16, area.y)].set_char(ch).set_style(
                Style::default().fg(self.palette.fg_primary).bg(self.palette.bg_secondary),
            );
        }
        x += prefix.chars().count() as u16;
    }
    // Render hint segments and match counter using existing Powerline hint style.
    // Hints: n NEXT, N PREV, c CASE, w WRAP
    // Then match count: active/total
    // (reuse the existing hint rendering loop with mode_hints returning search hints)
    // ... render key hints using the same Powerline segment code ...
    if let Some((active, total, case_sensitive, wrap)) = self.search_info {
        let case_label = if case_sensitive { "CASE" } else { "case" };
        let wrap_label = if wrap { "WRAP" } else { "wrap" };
        let hints = vec![
            ("n", "NEXT"), ("N", "PREV"),
            ("c", case_label), ("w", wrap_label),
        ];
        // ... render using the existing Powerline hint loop ...
        // Then append match count
        let counter = format!(" {}/{} ", active, total);
        // ... render counter ...
    }
    return;
}
```

(Use the same Powerline segment rendering code as the existing `mode_hints` loop.)

- [ ] **Step 4: Add search_matches field to TerminalWidget**

In `src/tui/widgets/terminal_widget.rs`:

```rust
use crate::tui::search::SearchMatch;

pub struct TerminalWidget<'a> {
    // ... existing fields ...
    search_matches: &'a [SearchMatch],
    active_match_index: Option<usize>,
    scrollback_base_row: usize, // absolute row of the top of the viewport
}
```

Add builder methods:

```rust
pub fn search(mut self, matches: &'a [SearchMatch], active: Option<usize>, base_row: usize) -> Self {
    self.search_matches = matches;
    self.active_match_index = active;
    self.scrollback_base_row = base_row;
    self
}
```

- [ ] **Step 5: Overlay search highlights in TerminalWidget::render**

After the normal cell rendering loop, add a second pass for search highlights:

```rust
// Search match highlighting
for (match_idx, search_match) in self.search_matches.iter().enumerate() {
    // Convert absolute row to viewport-relative row.
    if search_match.row < self.scrollback_base_row {
        continue;
    }
    let viewport_row = search_match.row - self.scrollback_base_row;
    if viewport_row >= rows as usize {
        continue;
    }
    let is_active = self.active_match_index == Some(match_idx);
    let highlight_bg = if is_active {
        self.palette.accent_yellow
    } else {
        Color::Rgb(113, 89, 32) // dimmer yellow
    };
    let highlight_fg = Color::Rgb(13, 17, 23); // black

    for col in search_match.col_start..search_match.col_end {
        if col >= cols as usize {
            break;
        }
        let x = inner.x + col as u16;
        let y = inner.y + viewport_row as u16;
        if x < inner.right() && y < inner.bottom() {
            let cell = &mut buf[(x, y)];
            if is_active {
                cell.set_style(Style::default().fg(highlight_fg).bg(highlight_bg));
            } else {
                cell.set_style(cell.style().bg(highlight_bg));
            }
        }
    }
}
```

- [ ] **Step 6: Pass search state to widgets in App render methods**

In `src/app.rs`, when creating `StatusBar`:

```rust
let mut status = StatusBar::new(self.mode, &self.palette, &self.ui_config)
    .error(self.last_error.as_deref());
if let Some(ref state) = self.search_state {
    status = status
        .search_query(Some(&state.query))
        .search_valid(state.is_valid_regex() || state.query.is_empty());
    if self.mode == Mode::Search {
        let active = state.active_index.map(|i| i + 1).unwrap_or(0);
        let total = state.matches.len();
        status = status.search_info(Some((active, total, state.case_sensitive, state.wrap)));
    }
}
```

When creating `TerminalWidget`, pass search matches:

```rust
let widget = TerminalWidget::new(&screen, preset_name, &self.palette, &self.ui_config)
    .focus(is_focused)
    .exited(exit_code)
    .pane_count(pane_count);
// Add search highlights if in search mode and this is the focused pane.
let widget = if is_focused && self.search_state.is_some() {
    let state = self.search_state.as_ref().unwrap();
    let base_row = /* compute from max_offset and current scrollback */ 0;
    widget.search(&state.matches, state.active_index, base_row)
} else {
    widget
};
```

- [ ] **Step 7: Build and test**

Run: `cargo build && cargo test`
Expected: compiles and all tests pass

- [ ] **Step 8: Commit**

```bash
git add src/tui/widgets/status_bar.rs src/tui/widgets/terminal_widget.rs src/app.rs
git commit -m "feat(tui): render search bar, match highlights, and navigation"
```

### Task 7: Update docs

**Files:**
- Modify: `docs/PRDs/003-tui-layout.md`

- [ ] **Step 1: Add EnterSearch and Search mode keybinding sections**

Add after the Room Mode section:

```markdown
### EnterSearch Mode

`Ctrl+f` from Terminal mode enters search. Type a regex query with live highlighting.

| Key | Action |
| --- | ------ |
| Any char | Append to query |
| `Backspace` | Delete last char |
| `Enter` | Confirm (enter Search mode) |
| `Esc` | Cancel search |

### Search Mode

Navigate search results in the focused pane.

| Key | Action |
| --- | ------ |
| `n` | Next match |
| `N` | Previous match |
| `c` | Toggle case sensitivity |
| `w` | Toggle wrap-around |
| `↑/↓` | Scroll up/down |
| `PageUp/PageDown` | Page scroll |
| `Esc` | Exit search |
```

- [ ] **Step 2: Update Status Bar Mode Colors table**

Add:

```markdown
| SEARCH | cyan (#56d4dd) |
```

- [ ] **Step 3: Commit**

```bash
git add docs/PRDs/003-tui-layout.md
git commit -m "docs: add search mode keybindings and status bar docs"
```
