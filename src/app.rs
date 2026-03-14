use humu::config::{humu_dir, HumuConfig, HumuState, RoomLayout, SplitDirection as CfgDir, SplitNode, TabLayout};
use humu::git::room::RoomManager;
use humu::git::workspace::WorkspaceManager;
use humu::hook::server::{HookEvent, HookServer};
use humu::pty::pane::PtyPane;
use humu::tui::completion::complete_path;
use humu::tui::input::{handle_key, Action, Direction as NavDirection, Mode};
use humu::tui::layout::{PaneId, SplitDirection, SplitTree, TabContainer};
use humu::tui::widgets::dialog::{Dialog, DialogField};
use humu::tui::widgets::preset_selector::PresetSelector;
use humu::tui::widgets::room_panel::{RoomItem, RoomPanel};
use humu::tui::widgets::status_bar::StatusBar;
use humu::tui::widgets::terminal_area::TabBar;
use humu::tui::widgets::terminal_widget::TerminalWidget;
use humu::tui::widgets::workspace_panel::{WorkspaceItem, WorkspacePanel};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::Terminal;
use std::collections::HashMap;
use std::io::stdout;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedPanel {
    Workspace,
    Room,
    Terminal,
}

/// Tracks the last-rendered rects for each major panel so mouse clicks can be
/// hit-tested without re-running the layout computation.
#[derive(Debug, Clone, Copy, Default)]
pub struct PanelRects {
    pub workspace: Rect,
    pub room: Rect,
    pub terminal: Rect,
    pub tab_bar: Rect,
}

/// Which panel border is currently being dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragTarget {
    /// The border between the workspace panel and the room panel.
    WorkspaceRoom,
    /// The border between the room panel and the terminal area.
    RoomTerminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetAction {
    NewTab,
    SplitDown,
    SplitRight,
}

pub enum PopupState {
    None,
    PresetSelector {
        presets: Vec<String>,
        selected: usize,
        action: PresetAction,
    },
    WorkspaceCreate {
        /// field indices: 0=Mode, 1=Path, 2=URL
        fields: Vec<DialogField>,
        focused_field: usize,
        completions: Vec<String>,
        completion_selected: Option<usize>,
    },
    RoomCreate {
        /// field indices: 0=Branch name, 1=Base branch
        fields: Vec<DialogField>,
        focused_field: usize,
    },
    WorkspaceDelete {
        /// field index 0=Confirm (yes/no)
        fields: Vec<DialogField>,
        focused_field: usize,
        workspace_name: String,
    },
    RoomDelete {
        /// field index 0=Confirm (yes/no)
        fields: Vec<DialogField>,
        focused_field: usize,
        branch: String,
    },
}

#[allow(dead_code)]
pub struct App {
    pub config: HumuConfig,
    pub state: HumuState,
    pub mode: Mode,
    pub focus: FocusedPanel,
    pub workspace_selected: Option<usize>,
    pub room_selected: Option<usize>,
    pub running: bool,
    pub panes: HashMap<PaneId, PtyPane>,
    pub tabs: TabContainer,
    pub next_pane_id: PaneId,
    pub focused_pane: Option<PaneId>,
    /// Tracks which preset name was used to spawn each pane.
    pub pane_presets: HashMap<PaneId, String>,
    /// Active popup (None when no popup is showing).
    pub popup: PopupState,
    /// Last error message to display (cleared on next action).
    pub last_error: Option<String>,
    /// Spinner state: (workspace, room) → last event time.
    pub spinner_state: HashMap<(String, String), Instant>,
    /// Receiver for hook events forwarded from the background tokio thread.
    pub hook_rx: Option<mpsc::Receiver<HookEvent>>,
    /// Last-rendered panel rects used for mouse hit-testing.
    pub panel_rects: PanelRects,
    /// Panel widths: [workspace, room]. Used in the layout constraints.
    pub panel_widths: [u16; 2],
    /// Active drag target when resizing a panel border via mouse drag.
    pub dragging: Option<DragTarget>,
    /// When Some(id), only that pane is rendered filling the full terminal area.
    pub fullscreen_pane: Option<PaneId>,
    pub palette: humu::tui::theme::Palette,
    pub ui_config: humu::tui::theme::UiConfig,
}

impl App {
    pub fn new() -> Result<Self> {
        let config_path = humu_dir().join("config.toml");
        let state_path = humu_dir().join("state.toml");

        let config = if config_path.exists() {
            HumuConfig::load(&config_path)?
        } else {
            HumuConfig::default()
        };

        let state = if state_path.exists() {
            HumuState::load(&state_path)?
        } else {
            HumuState::default()
        };

        let mut tabs = TabContainer::new();
        let mut panes = HashMap::new();
        let mut pane_presets = HashMap::new();
        let pane_id: PaneId = 0;

        // Spawn a default shell pane
        let shell_cmd = config
            .presets
            .get("shell")
            .map(|p| p.command.as_str())
            .unwrap_or("sh");
        let shell_args: Vec<&str> = config
            .presets
            .get("shell")
            .map(|p| p.args.iter().map(String::as_str).collect())
            .unwrap_or_default();
        let (cmd, args) = humu::preset::resolve_preset(shell_cmd, &shell_args);
        let args_refs: Vec<String> = args;
        let pane = PtyPane::spawn(&cmd, &args_refs, None, 80, 24)?;
        panes.insert(pane_id, pane);
        pane_presets.insert(pane_id, "shell".to_string());
        tabs.add_tab("shell".into(), SplitTree::leaf(pane_id));

        // Start hook server in a background tokio runtime and forward events over
        // a std mpsc channel so the synchronous event loop can call try_recv().
        let sock_path = humu_dir().join("humu.sock");
        let (hook_tx, hook_rx) = mpsc::channel::<HookEvent>();
        thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                match HookServer::new(&sock_path).await {
                    Ok(server) => {
                        let mut rx = server.subscribe();
                        loop {
                            if let Ok(event) = rx.recv().await {
                                let _ = hook_tx.send(event);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("hook server error: {e}");
                    }
                }
            });
        });

        let ui_config = humu::tui::theme::UiConfig {
            simplified_ui: config.ui.simplified_ui,
            rounded_corners: config.ui.rounded_corners,
        };

        Ok(Self {
            config,
            state,
            mode: Mode::Normal,
            focus: FocusedPanel::Terminal,
            workspace_selected: None,
            room_selected: None,
            running: true,
            panes,
            tabs,
            next_pane_id: 1,
            focused_pane: Some(pane_id),
            pane_presets,
            popup: PopupState::None,
            last_error: None,
            spinner_state: HashMap::new(),
            hook_rx: Some(hook_rx),
            panel_rects: PanelRects::default(),
            panel_widths: [20, 18],
            dragging: None,
            fullscreen_pane: None,
            palette: humu::tui::theme::Palette::GITHUB_DARK,
            ui_config,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        stdout().execute(EnterAlternateScreen)?;
        crossterm::execute!(stdout(), crossterm::event::EnableMouseCapture)?;

        let backend = ratatui::backend::CrosstermBackend::new(stdout());
        let mut terminal = Terminal::new(backend)?;

        self.restore_selection();

        while self.running {
            terminal.draw(|frame| self.render(frame))?;

            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        // Clear any previous error on new keypress.
                        self.last_error = None;
                        // Popup intercepts all key handling when active.
                        if self.handle_popup_key(key) {
                            // key was consumed by popup
                        } else {
                            self.handle_action(handle_key(self.mode, key));
                        }
                    }
                    Event::Mouse(mouse) => {
                        match mouse.kind {
                            MouseEventKind::Down(MouseButton::Left) => {
                                self.dragging = None;
                                self.handle_click(mouse.column, mouse.row);
                            }
                            MouseEventKind::Drag(MouseButton::Left) => {
                                self.handle_drag(mouse.column);
                            }
                            MouseEventKind::Up(MouseButton::Left) => {
                                self.dragging = None;
                            }
                            _ => {}
                        }
                    }
                    // Pane resizing is handled in render_terminal_area on the
                    // next draw cycle, so we only need to consume the event.
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }

            // Process hook events each tick.
            self.process_hook_events();

            // Process PTY output each tick
            for pane in self.panes.values_mut() {
                let _ = pane.process_output();
            }
        }

        crossterm::execute!(stdout(), crossterm::event::DisableMouseCapture)?;
        stdout().execute(LeaveAlternateScreen)?;
        disable_raw_mode()?;

        // Graceful shutdown: persist layout, then drop all PTY children.
        self.persist_layout();
        self.panes.clear(); // Drop all PTY panes, killing child processes.

        // Remove the socket file explicitly — the hook server thread runs
        // forever so its Drop impl never fires during normal exit.
        let sock_path = humu_dir().join("humu.sock");
        let _ = std::fs::remove_file(&sock_path);

        let state_path = humu_dir().join("state.toml");
        self.state.save(&state_path)?;

        Ok(())
    }

    /// Handle a key event when a popup is active.
    /// Returns `true` if the key was consumed (popup was active), `false` otherwise.
    fn handle_popup_key(&mut self, key: KeyEvent) -> bool {
        match &self.popup {
            PopupState::None => false,

            PopupState::PresetSelector { .. } => {
                self.handle_preset_selector_key(key);
                true
            }
            PopupState::WorkspaceCreate { .. }
            | PopupState::RoomCreate { .. }
            | PopupState::WorkspaceDelete { .. }
            | PopupState::RoomDelete { .. } => {
                self.handle_dialog_key(key);
                true
            }
        }
    }

    fn handle_preset_selector_key(&mut self, key: KeyEvent) {
        let PopupState::PresetSelector { presets, selected, action } = &self.popup else {
            return;
        };
        let presets = presets.clone();
        let mut selected = *selected;
        let action = *action;

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if selected + 1 < presets.len() {
                    selected += 1;
                }
                self.popup = PopupState::PresetSelector { presets, selected, action };
            }
            KeyCode::Char('k') | KeyCode::Up => {
                selected = selected.saturating_sub(1);
                self.popup = PopupState::PresetSelector { presets, selected, action };
            }
            KeyCode::Enter => {
                let chosen = presets[selected].clone();
                self.popup = PopupState::None;
                match action {
                    PresetAction::NewTab => self.new_tab_with_preset(&chosen),
                    PresetAction::SplitDown => self.split_pane_with_preset(&chosen, false),
                    PresetAction::SplitRight => self.split_pane_with_preset(&chosen, true),
                }
            }
            KeyCode::Esc => {
                self.popup = PopupState::None;
            }
            _ => {}
        }
    }

    fn handle_dialog_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                if !self.try_dismiss_completions() {
                    self.popup = PopupState::None;
                }
            }
            KeyCode::Enter => {
                if !self.try_confirm_completion() {
                    self.confirm_dialog();
                }
            }
            KeyCode::Tab | KeyCode::BackTab => {
                if !self.try_accept_completion() {
                    self.dialog_move_focus(key.code == KeyCode::Tab);
                }
            }
            KeyCode::Up => {
                if !self.try_navigate_completion(false) {
                    self.dialog_move_focus(false);
                }
            }
            KeyCode::Down => {
                if !self.try_navigate_completion(true) {
                    self.dialog_move_focus(true);
                }
            }
            KeyCode::Left => {
                self.dialog_field_left();
            }
            KeyCode::Right => {
                self.dialog_field_right();
            }
            KeyCode::Backspace => {
                self.dialog_field_backspace();
                self.refresh_completions();
            }
            KeyCode::Char(c) => {
                self.dialog_field_insert(c);
                self.refresh_completions();
            }
            _ => {}
        }
    }

    /// If the workspace-create Path field is focused and has completions,
    /// accept the selected (or first) completion.  Returns `true` if consumed.
    fn try_accept_completion(&mut self) -> bool {
        let PopupState::WorkspaceCreate {
            fields,
            focused_field,
            completions,
            completion_selected,
        } = &mut self.popup
        else {
            return false;
        };

        // Only act when focused on the Path field (index 1) and completions exist.
        if *focused_field != 1 || completions.is_empty() {
            return false;
        }

        // Accept the current selection, or pick the first item if none highlighted.
        let next = completion_selected.unwrap_or(0);

        // Write the selected completion into the Path field value.
        if let Some(DialogField::TextInput { value, .. }) = fields.get_mut(1) {
            *value = completions[next].clone();
        }
        // Recompute completions for the new value (important for directories).
        // Always clear selection so Enter submits the dialog next time
        // unless the user explicitly navigates with Up/Down.
        if let Some(DialogField::TextInput { value, .. }) = fields.get(1) {
            *completions = complete_path(value);
        }
        *completion_selected = None;

        true
    }

    /// Clear completions if they are visible.  Returns `true` if consumed
    /// (so Esc dismisses suggestions first, closes dialog on second Esc).
    fn try_dismiss_completions(&mut self) -> bool {
        let PopupState::WorkspaceCreate {
            focused_field,
            completions,
            completion_selected,
            ..
        } = &mut self.popup
        else {
            return false;
        };

        if *focused_field != 1 || completions.is_empty() {
            return false;
        }

        completions.clear();
        *completion_selected = None;
        true
    }

    /// Accept the highlighted completion into the Path field on Enter.
    /// Returns `true` if a completion was selected (dialog NOT submitted).
    fn try_confirm_completion(&mut self) -> bool {
        let PopupState::WorkspaceCreate {
            fields,
            focused_field,
            completions,
            completion_selected,
        } = &mut self.popup
        else {
            return false;
        };

        if *focused_field != 1 {
            return false;
        }

        let Some(sel) = *completion_selected else {
            return false;
        };

        if sel >= completions.len() {
            return false;
        }

        if let Some(DialogField::TextInput { value, .. }) = fields.get_mut(1) {
            *value = completions[sel].clone();
        }
        *completions = if let Some(DialogField::TextInput { value, .. }) = fields.get(1) {
            complete_path(value)
        } else {
            vec![]
        };
        *completion_selected = None;
        true
    }

    /// Move the completion highlight up/down.  Returns `true` if consumed.
    fn try_navigate_completion(&mut self, down: bool) -> bool {
        let PopupState::WorkspaceCreate {
            focused_field,
            completions,
            completion_selected,
            ..
        } = &mut self.popup
        else {
            return false;
        };

        if *focused_field != 1 || completions.is_empty() {
            return false;
        }

        let next = match *completion_selected {
            Some(idx) => {
                if down {
                    if idx + 1 < completions.len() { idx + 1 } else { 0 }
                } else {
                    if idx == 0 { completions.len() - 1 } else { idx - 1 }
                }
            }
            None => if down { 0 } else { completions.len() - 1 },
        };
        *completion_selected = Some(next);
        true
    }

    /// Recompute path completions when the Path text field changes.
    fn refresh_completions(&mut self) {
        let PopupState::WorkspaceCreate {
            fields,
            focused_field,
            completions,
            completion_selected,
        } = &mut self.popup
        else {
            return;
        };

        if *focused_field != 1 {
            return;
        }

        if let Some(DialogField::TextInput { value, .. }) = fields.get(1) {
            *completions = complete_path(value);
            *completion_selected = None;
        }
    }

    fn dialog_field_count(&self) -> usize {
        match &self.popup {
            PopupState::WorkspaceCreate { fields, .. }
            | PopupState::RoomCreate { fields, .. }
            | PopupState::WorkspaceDelete { fields, .. }
            | PopupState::RoomDelete { fields, .. } => fields.len(),
            _ => 0,
        }
    }

    fn dialog_move_focus(&mut self, forward: bool) {
        let count = self.dialog_field_count();
        if count == 0 {
            return;
        }
        match &mut self.popup {
            PopupState::WorkspaceCreate { focused_field, .. }
            | PopupState::RoomCreate { focused_field, .. }
            | PopupState::WorkspaceDelete { focused_field, .. }
            | PopupState::RoomDelete { focused_field, .. } => {
                if forward {
                    *focused_field = (*focused_field + 1) % count;
                } else {
                    *focused_field = focused_field.checked_sub(1).unwrap_or(count - 1);
                }
            }
            _ => {}
        }
    }

    /// Left arrow: for Select fields cycle backwards; for Confirm toggle yes/no.
    fn dialog_field_left(&mut self) {
        match &mut self.popup {
            PopupState::WorkspaceCreate { fields, focused_field, .. }
            | PopupState::RoomCreate { fields, focused_field, .. }
            | PopupState::WorkspaceDelete { fields, focused_field, .. }
            | PopupState::RoomDelete { fields, focused_field, .. } => {
                let idx = *focused_field;
                if idx < fields.len() {
                    match &mut fields[idx] {
                        DialogField::Select { options, selected, .. } => {
                            if *selected > 0 {
                                *selected -= 1;
                            } else {
                                *selected = options.len().saturating_sub(1);
                            }
                        }
                        DialogField::Confirm { yes, .. } => {
                            *yes = !*yes;
                        }
                        DialogField::TextInput { .. } => {}
                    }
                }
            }
            _ => {}
        }
    }

    /// Right arrow: for Select fields cycle forwards; for Confirm toggle yes/no.
    fn dialog_field_right(&mut self) {
        match &mut self.popup {
            PopupState::WorkspaceCreate { fields, focused_field, .. }
            | PopupState::RoomCreate { fields, focused_field, .. }
            | PopupState::WorkspaceDelete { fields, focused_field, .. }
            | PopupState::RoomDelete { fields, focused_field, .. } => {
                let idx = *focused_field;
                if idx < fields.len() {
                    match &mut fields[idx] {
                        DialogField::Select { options, selected, .. } => {
                            *selected = (*selected + 1) % options.len().max(1);
                        }
                        DialogField::Confirm { yes, .. } => {
                            *yes = !*yes;
                        }
                        DialogField::TextInput { .. } => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn dialog_field_backspace(&mut self) {
        match &mut self.popup {
            PopupState::WorkspaceCreate { fields, focused_field, .. }
            | PopupState::RoomCreate { fields, focused_field, .. }
            | PopupState::WorkspaceDelete { fields, focused_field, .. }
            | PopupState::RoomDelete { fields, focused_field, .. } => {
                let idx = *focused_field;
                if idx < fields.len()
                    && let DialogField::TextInput { value, .. } = &mut fields[idx]
                {
                    value.pop();
                }
            }
            _ => {}
        }
    }

    fn dialog_field_insert(&mut self, c: char) {
        match &mut self.popup {
            PopupState::WorkspaceCreate { fields, focused_field, .. }
            | PopupState::RoomCreate { fields, focused_field, .. }
            | PopupState::WorkspaceDelete { fields, focused_field, .. }
            | PopupState::RoomDelete { fields, focused_field, .. } => {
                let idx = *focused_field;
                if idx < fields.len()
                    && let DialogField::TextInput { value, .. } = &mut fields[idx]
                {
                    value.push(c);
                }
            }
            _ => {}
        }
    }

    fn confirm_dialog(&mut self) {
        // Swap popup to None so we can consume the data.
        let popup = std::mem::replace(&mut self.popup, PopupState::None);
        match popup {
            PopupState::WorkspaceCreate { fields, .. } => {
                self.execute_workspace_create(fields);
            }
            PopupState::RoomCreate { fields, .. } => {
                self.execute_room_create(fields);
            }
            PopupState::WorkspaceDelete { fields, workspace_name, .. } => {
                self.execute_workspace_delete(fields, workspace_name);
            }
            PopupState::RoomDelete { fields, branch, .. } => {
                self.execute_room_delete(fields, branch);
            }
            other => {
                // Restore if we didn't handle it.
                self.popup = other;
            }
        }
    }

    fn execute_workspace_create(&mut self, fields: Vec<DialogField>) {
        // Field 0: Mode (Clone/Existing/New)
        // Field 1: Path
        // Field 2: URL (only used for Clone)
        let mode_idx = match &fields[0] {
            DialogField::Select { selected, .. } => *selected,
            _ => 0,
        };
        let path_str = match &fields[1] {
            DialogField::TextInput { value, .. } => value.clone(),
            _ => String::new(),
        };
        if path_str.is_empty() {
            self.last_error = Some("Path is required".to_string());
            return;
        }
        // Expand ~ to the user's home directory (Rust's Path doesn't do this).
        let expanded = if path_str.starts_with("~/") || path_str == "~" {
            if let Some(home) = dirs::home_dir() {
                format!("{}{}", home.display(), &path_str[1..])
            } else {
                path_str.clone()
            }
        } else {
            path_str.clone()
        };
        // Strip trailing slash so Path resolves correctly.
        let trimmed = expanded.trim_end_matches('/');
        let path = std::path::Path::new(trimmed);
        let mgr = WorkspaceManager::new();
        let result = match mode_idx {
            0 => {
                // Clone
                let url = match &fields[2] {
                    DialogField::TextInput { value, .. } => value.clone(),
                    _ => String::new(),
                };
                if url.is_empty() {
                    self.last_error = Some("URL is required for Clone".to_string());
                    return;
                }
                mgr.clone_remote(&mut self.state, &url, path)
            }
            1 => {
                // Existing
                mgr.register(&mut self.state, path)
            }
            _ => {
                // New
                mgr.init(&mut self.state, path)
            }
        };
        match result {
            Ok(_name) => {
                self.last_error = None;
            }
            Err(e) => {
                self.last_error = Some(e.to_string());
            }
        }
    }

    fn execute_room_create(&mut self, fields: Vec<DialogField>) {
        // Field 0: Branch name
        // Field 1: Base branch
        let branch = match &fields[0] {
            DialogField::TextInput { value, .. } => value.clone(),
            _ => String::new(),
        };
        let base_branch = match &fields[1] {
            DialogField::TextInput { value, .. } => value.clone(),
            _ => String::new(),
        };
        if branch.is_empty() {
            self.last_error = Some("Branch name is required".to_string());
            return;
        }

        let ws_name = match &self.state.active_workspace {
            Some(w) => w.clone(),
            None => {
                self.last_error = Some("No active workspace".to_string());
                return;
            }
        };
        let ws_path = match self.state.workspaces.get(&ws_name) {
            Some(e) => e.path.clone(),
            None => {
                self.last_error = Some("Workspace not found".to_string());
                return;
            }
        };
        let base = if base_branch.is_empty() { "HEAD" } else { &base_branch };
        let worktree_path = humu_dir()
            .join("worktrees")
            .join(&ws_name)
            .join(&branch);
        let mgr = RoomManager::new();
        match mgr.create(&ws_path, &branch, base, &worktree_path) {
            Ok(()) => {
                self.last_error = None;
            }
            Err(e) => {
                self.last_error = Some(e.to_string());
            }
        }
    }

    fn execute_workspace_delete(&mut self, fields: Vec<DialogField>, workspace_name: String) {
        // Field 0: Confirm — also used as "remove from disk?" prompt
        let remove_from_disk = match &fields[0] {
            DialogField::Confirm { yes, .. } => *yes,
            _ => false,
        };
        let mgr = WorkspaceManager::new();
        match mgr.delete(&mut self.state, &workspace_name, remove_from_disk) {
            Ok(()) => {
                // Adjust selection if needed.
                let count = self.state.workspaces.len();
                if count == 0 {
                    self.workspace_selected = None;
                } else if let Some(sel) = self.workspace_selected
                    && sel >= count
                {
                    self.workspace_selected = Some(count - 1);
                }
                self.last_error = None;
            }
            Err(e) => {
                self.last_error = Some(e.to_string());
            }
        }
    }

    fn execute_room_delete(&mut self, fields: Vec<DialogField>, branch: String) {
        let confirmed = match &fields[0] {
            DialogField::Confirm { yes, .. } => *yes,
            _ => false,
        };
        if !confirmed {
            return;
        }
        let ws_name = match &self.state.active_workspace {
            Some(w) => w.clone(),
            None => {
                self.last_error = Some("No active workspace".to_string());
                return;
            }
        };
        let ws_path = match self.state.workspaces.get(&ws_name) {
            Some(e) => e.path.clone(),
            None => {
                self.last_error = Some("Workspace not found".to_string());
                return;
            }
        };
        let worktree_path = humu_dir()
            .join("worktrees")
            .join(&ws_name)
            .join(&branch);
        let mgr = RoomManager::new();
        match mgr.delete(&ws_path, &branch, &worktree_path) {
            Ok(()) => {
                self.last_error = None;
            }
            Err(e) => {
                self.last_error = Some(e.to_string());
            }
        }
    }

    fn render(&mut self, frame: &mut ratatui::Frame) {
        let size = frame.area();

        // Main layout: [workspace | room | terminal] + status bar
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(size);

        let panel_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(self.panel_widths[0]),
                Constraint::Length(self.panel_widths[1]),
                Constraint::Min(1),
            ])
            .split(main_chunks[0]);

        // Store rects for mouse hit-testing.
        let tab_bar_rect = Rect::new(panel_chunks[2].x, panel_chunks[2].y, panel_chunks[2].width, 1);
        self.panel_rects = PanelRects {
            workspace: panel_chunks[0],
            room: panel_chunks[1],
            terminal: panel_chunks[2],
            tab_bar: tab_bar_rect,
        };

        // Workspace panel
        let workspaces = self.workspace_items();
        let ws_widget = WorkspacePanel::new(&workspaces, &self.palette, &self.ui_config)
            .selected(self.workspace_selected)
            .focus(self.focus == FocusedPanel::Workspace);
        frame.render_widget(ws_widget, panel_chunks[0]);

        // Room panel
        let rooms = self.room_items();
        let room_widget = RoomPanel::new(&rooms, &self.palette, &self.ui_config)
            .selected(self.room_selected)
            .focus(self.focus == FocusedPanel::Room);
        frame.render_widget(room_widget, panel_chunks[1]);

        // Terminal area: tab bar (1 line) + pane area
        self.render_terminal_area(frame, panel_chunks[2]);

        // Status bar
        let status = StatusBar::new(self.mode).error(self.last_error.as_deref());
        frame.render_widget(status, main_chunks[1]);

        // Render popup on top of everything when active.
        self.render_popup(frame, size);
    }

    fn render_popup(&self, frame: &mut ratatui::Frame, area: Rect) {
        match &self.popup {
            PopupState::None => {}
            PopupState::PresetSelector { presets, selected, .. } => {
                frame.render_widget(PresetSelector::new(presets, *selected), area);
            }
            PopupState::WorkspaceCreate {
                fields,
                focused_field,
                completions,
                completion_selected,
            } => {
                let mut dialog = Dialog::new("Create Workspace", fields, *focused_field);
                dialog.completions = completions;
                dialog.completion_selected = *completion_selected;
                dialog.completion_field = Some(1); // Path field
                frame.render_widget(dialog, area);
            }
            PopupState::RoomCreate { fields, focused_field } => {
                frame.render_widget(Dialog::new("Create Room", fields, *focused_field), area);
            }
            PopupState::WorkspaceDelete { fields, focused_field, .. } => {
                frame.render_widget(Dialog::new("Delete Workspace", fields, *focused_field), area);
            }
            PopupState::RoomDelete { fields, focused_field, .. } => {
                frame.render_widget(Dialog::new("Delete Room", fields, *focused_field), area);
            }
        }
    }

    fn render_terminal_area(&mut self, frame: &mut ratatui::Frame, area: Rect) {
        if area.height == 0 {
            return;
        }

        // Split into tab bar (1 row) and pane content area
        let tab_bar_area = Rect::new(area.x, area.y, area.width, 1);
        let pane_area = if area.height > 1 {
            Rect::new(area.x, area.y + 1, area.width, area.height - 1)
        } else {
            Rect::new(area.x, area.y, area.width, 0)
        };

        // Render tab bar — a tab is active if any of its panes is a "claude"
        // preset and the current workspace/room has an active spinner entry.
        let tab_names: Vec<&str> = self.tabs.tab_names();
        let active_indicators: Vec<bool> = (0..tab_names.len())
            .map(|i| {
                let tree = match self.tabs.tree_at(i) {
                    Some(t) => t,
                    None => return false,
                };
                tree.pane_ids().iter().any(|pid| {
                    if self.pane_presets.get(pid).map(String::as_str) != Some("claude") {
                        return false;
                    }
                    // Check if any spinner entry matches the current workspace/room.
                    self.spinner_state.keys().any(|(ws, room)| {
                        self.state.active_workspace.as_deref() == Some(ws.as_str())
                            && self.state.active_room.as_deref() == Some(room.as_str())
                    })
                })
            })
            .collect();
        let tab_bar = TabBar::new(&tab_names, self.tabs.active_index(), &active_indicators, &self.palette, &self.ui_config);
        frame.render_widget(tab_bar, tab_bar_area);

        // Render panes from active tab's split tree
        if pane_area.height == 0 {
            return;
        }

        // Fullscreen mode: render only the fullscreen pane filling the whole area.
        if let Some(fs_id) = self.fullscreen_pane {
            if let Some(pane) = self.panes.get_mut(&fs_id) {
                if pane.cols() != pane_area.width || pane.rows() != pane_area.height {
                    let _ = pane.resize(pane_area.width, pane_area.height);
                }
            }
            if let Some(pane) = self.panes.get(&fs_id) {
                let screen = pane.screen();
                let widget = TerminalWidget::new(&screen).focus(true).exited(None);
                frame.render_widget(widget, pane_area);
            }
            return;
        }

        if let Some(tree) = self.tabs.active_tree() {
            let rects = tree.compute_rects(pane_area);
            for (pane_id, rect) in &rects {
                // Resize pane if its dimensions have changed since last render.
                if let Some(pane) = self.panes.get_mut(pane_id)
                    && (pane.cols() != rect.width || pane.rows() != rect.height)
                {
                    let _ = pane.resize(rect.width, rect.height);
                }
            }
            for (pane_id, rect) in rects {
                if let Some(pane) = self.panes.get(&pane_id) {
                    let screen = pane.screen();
                    let is_focused = self.focused_pane == Some(pane_id)
                        && self.focus == FocusedPanel::Terminal;
                    // exit_status() requires &mut self; exit overlay wired in a later task
                    let widget = TerminalWidget::new(&screen).focus(is_focused).exited(None);
                    frame.render_widget(widget, rect);
                }
            }
        }
    }

    fn handle_action(&mut self, action: Action) {
        match action {
            Action::EnterMode(mode) => {
                self.mode = mode;
                // Auto-focus the relevant panel when entering Workspace/Room mode.
                if mode == Mode::Workspace {
                    self.focus = FocusedPanel::Workspace;
                } else if mode == Mode::Room {
                    self.focus = FocusedPanel::Room;
                }
            }
            Action::ExitToNormal => self.mode = Mode::Normal,
            Action::Quit => self.running = false,

            Action::FocusWorkspacePanel => self.focus = FocusedPanel::Workspace,
            Action::FocusRoomPanel => self.focus = FocusedPanel::Room,

            Action::NavigateUp => self.navigate(-1),
            Action::NavigateDown => self.navigate(1),
            Action::Select => self.select_current(),

            Action::PassThrough(key) => self.handle_passthrough(key),

            // Tab actions
            Action::NewTab => self.show_preset_selector(PresetAction::NewTab),
            Action::CloseTab => self.close_tab(),
            Action::PrevTab => {
                let active = self.tabs.active_index();
                if active > 0 {
                    self.tabs.set_active(active - 1);
                    self.sync_focused_pane();
                }
            }
            Action::NextTab => {
                let active = self.tabs.active_index();
                if active + 1 < self.tabs.len() {
                    self.tabs.set_active(active + 1);
                    self.sync_focused_pane();
                }
            }
            Action::GoToTab(n) => {
                if n < self.tabs.len() {
                    self.tabs.set_active(n);
                    self.sync_focused_pane();
                }
            }

            // Pane actions
            Action::NewPane => self.show_preset_selector(PresetAction::SplitDown),
            Action::SplitDown => self.show_preset_selector(PresetAction::SplitDown),
            Action::SplitRight => self.show_preset_selector(PresetAction::SplitRight),
            Action::ClosePane => self.close_pane(),
            Action::MoveFocus(dir) => self.move_focus(dir),
            Action::ToggleFullscreen => self.toggle_fullscreen(),
            Action::RenameTab => {} // stub: inline rename popup deferred

            // Workspace/room actions
            Action::Create => self.show_create_dialog(),
            Action::Delete => self.show_delete_dialog(),

            // Resize actions
            Action::Resize(dir) => self.handle_resize_action(dir, false),
            Action::ResizeReverse(dir) => self.handle_resize_action(dir, true),

            _ => {}
        }
    }

    /// Handle a left-button mouse click at terminal coordinates (x, y).
    fn handle_click(&mut self, x: u16, y: u16) {
        let pos = Position::new(x, y);
        if self.panel_rects.workspace.contains(pos) {
            self.focus = FocusedPanel::Workspace;
            let row = y.saturating_sub(self.panel_rects.workspace.y + 1); // +1 for border
            self.workspace_selected = Some(row as usize);
        } else if self.panel_rects.room.contains(pos) {
            self.focus = FocusedPanel::Room;
            let row = y.saturating_sub(self.panel_rects.room.y + 1);
            self.room_selected = Some(row as usize);
        } else if self.panel_rects.tab_bar.contains(pos) {
            // Determine which tab or "+" was clicked.
            self.handle_tab_bar_click(x);
        } else if self.panel_rects.terminal.contains(pos) {
            self.focus = FocusedPanel::Terminal;
            let pane_area = {
                let r = self.panel_rects.terminal;
                if r.height > 1 {
                    Rect::new(r.x, r.y + 1, r.width, r.height - 1)
                } else {
                    Rect::new(r.x, r.y, r.width, 0)
                }
            };
            if let Some(tree) = self.tabs.active_tree() {
                let rects = tree.compute_rects(pane_area);
                for (pane_id, rect) in rects {
                    if rect.contains(pos) {
                        self.focused_pane = Some(pane_id);
                        break;
                    }
                }
            }
        } else {
            // Click on a panel border — detect which border and start dragging.
            let ws_right = self.panel_rects.workspace.x + self.panel_rects.workspace.width;
            let room_right = self.panel_rects.room.x + self.panel_rects.room.width;
            if x.abs_diff(ws_right) <= 1 {
                self.dragging = Some(DragTarget::WorkspaceRoom);
            } else if x.abs_diff(room_right) <= 1 {
                self.dragging = Some(DragTarget::RoomTerminal);
            }
        }
    }

    /// Handle a left-button drag at column `x` — resize the panel being dragged.
    fn handle_drag(&mut self, x: u16) {
        match self.dragging {
            Some(DragTarget::WorkspaceRoom) => {
                let origin = self.panel_rects.workspace.x;
                let new_width = x.saturating_sub(origin).clamp(5, 60);
                self.panel_widths[0] = new_width;
            }
            Some(DragTarget::RoomTerminal) => {
                let origin = self.panel_rects.room.x;
                let new_width = x.saturating_sub(origin).clamp(5, 60);
                let ws_width = self.panel_widths[0];
                if ws_width + new_width < 120 {
                    self.panel_widths[1] = new_width;
                }
            }
            None => {
                // No active drag — try to start one if the cursor is near a border.
                let ws_right = self.panel_rects.workspace.x + self.panel_rects.workspace.width;
                let room_right = self.panel_rects.room.x + self.panel_rects.room.width;
                if x.abs_diff(ws_right) <= 1 {
                    self.dragging = Some(DragTarget::WorkspaceRoom);
                    self.handle_drag(x);
                } else if x.abs_diff(room_right) <= 1 {
                    self.dragging = Some(DragTarget::RoomTerminal);
                    self.handle_drag(x);
                }
            }
        }
    }

    /// Handle keyboard resize actions (Resize / ResizeReverse).
    fn handle_resize_action(&mut self, dir: NavDirection, reverse: bool) {
        match self.focus {
            FocusedPanel::Terminal => {
                if let Some(pane_id) = self.focused_pane
                    && let Some(tree) = self.tabs.active_tree_mut()
                {
                    // Positive delta grows the first child; direction determines sign.
                    let sign: f64 = match dir {
                        NavDirection::Left | NavDirection::Up => {
                            if reverse { 0.05 } else { -0.05 }
                        }
                        NavDirection::Right | NavDirection::Down => {
                            if reverse { -0.05 } else { 0.05 }
                        }
                    };
                    tree.resize(pane_id, sign);
                }
            }
            FocusedPanel::Workspace => {
                let delta: i16 = match dir {
                    NavDirection::Right => if reverse { -1 } else { 1 },
                    NavDirection::Left => if reverse { 1 } else { -1 },
                    _ => 0,
                };
                self.panel_widths[0] =
                    (self.panel_widths[0] as i16 + delta).clamp(5, 60) as u16;
            }
            FocusedPanel::Room => {
                let delta: i16 = match dir {
                    NavDirection::Right => if reverse { -1 } else { 1 },
                    NavDirection::Left => if reverse { 1 } else { -1 },
                    _ => 0,
                };
                self.panel_widths[1] =
                    (self.panel_widths[1] as i16 + delta).clamp(5, 60) as u16;
            }
        }
    }

    /// Handle tab bar clicks: switch tabs or open new tab via "+".
    fn handle_tab_bar_click(&mut self, x: u16) {
        let tab_names = self.tabs.tab_names().iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let mut cursor = self.panel_rects.tab_bar.x;
        for (i, name) in tab_names.iter().enumerate() {
            // Tab text is " {name} " — 2 extra chars (leading + trailing space).
            let tab_width = (name.len() as u16) + 2;
            if x >= cursor && x < cursor + tab_width {
                self.tabs.set_active(i);
                self.sync_focused_pane();
                self.focus = FocusedPanel::Terminal;
                return;
            }
            cursor += tab_width;
        }
        // Clicked on "+" — open new tab.
        self.show_preset_selector(PresetAction::NewTab);
    }

    /// Build and display the preset selector popup.
    fn show_preset_selector(&mut self, action: PresetAction) {
        let mut presets: Vec<String> = self.config.presets.keys().cloned().collect();
        presets.sort();
        if presets.is_empty() {
            // No presets configured — fall back to direct action with "shell".
            match action {
                PresetAction::NewTab => self.new_tab_with_preset("shell"),
                PresetAction::SplitDown => self.split_pane_with_preset("shell", false),
                PresetAction::SplitRight => self.split_pane_with_preset("shell", true),
            }
            return;
        }
        self.popup = PopupState::PresetSelector { presets, selected: 0, action };
    }

    /// Show the appropriate create dialog based on focused panel.
    fn show_create_dialog(&mut self) {
        match self.focus {
            FocusedPanel::Workspace => {
                let fields = vec![
                    DialogField::Select {
                        label: "Mode".to_string(),
                        options: vec![
                            "Clone".to_string(),
                            "Existing".to_string(),
                            "New".to_string(),
                        ],
                        selected: 0,
                    },
                    DialogField::TextInput {
                        label: "Path".to_string(),
                        value: String::new(),
                    },
                    DialogField::TextInput {
                        label: "URL (Clone only)".to_string(),
                        value: String::new(),
                    },
                ];
                self.popup = PopupState::WorkspaceCreate {
                    fields,
                    focused_field: 0,
                    completions: vec![],
                    completion_selected: None,
                };
            }
            FocusedPanel::Room => {
                let fields = vec![
                    DialogField::TextInput {
                        label: "Branch name".to_string(),
                        value: String::new(),
                    },
                    DialogField::TextInput {
                        label: "Base branch".to_string(),
                        value: String::new(),
                    },
                ];
                self.popup = PopupState::RoomCreate { fields, focused_field: 0 };
            }
            FocusedPanel::Terminal => {}
        }
    }

    /// Show the appropriate delete dialog based on focused panel.
    fn show_delete_dialog(&mut self) {
        match self.focus {
            FocusedPanel::Workspace => {
                let ws_name = {
                    let mut names: Vec<_> = self.state.workspaces.keys().cloned().collect();
                    names.sort();
                    match self.workspace_selected {
                        Some(i) => names.into_iter().nth(i),
                        None => None,
                    }
                };
                let ws_name = match ws_name {
                    Some(n) => n,
                    None => return, // nothing selected
                };
                let fields = vec![DialogField::Confirm {
                    message: format!("Delete workspace '{ws_name}'? Also remove from disk?"),
                    yes: false,
                }];
                self.popup = PopupState::WorkspaceDelete {
                    fields,
                    focused_field: 0,
                    workspace_name: ws_name,
                };
            }
            FocusedPanel::Room => {
                // Use active room as the target.
                let branch = match &self.state.active_room {
                    Some(r) => r.clone(),
                    None => return,
                };
                let fields = vec![DialogField::Confirm {
                    message: "Delete room? This removes worktree and branch.".to_string(),
                    yes: false,
                }];
                self.popup = PopupState::RoomDelete {
                    fields,
                    focused_field: 0,
                    branch,
                };
            }
            FocusedPanel::Terminal => {}
        }
    }

    /// Compute the working directory for the currently active workspace/room.
    ///
    /// Returns `None` when no workspace or room is active.  The default room
    /// (the workspace repo itself) maps to the workspace path; worktree rooms
    /// map to `~/.humu/worktrees/<workspace>/<room>`.
    fn current_room_path(&self) -> Option<PathBuf> {
        let ws_name = self.state.active_workspace.as_ref()?;
        let ws_entry = self.state.workspaces.get(ws_name.as_str())?;
        let room = self.state.active_room.as_ref()?;

        let worktree_path = humu_dir()
            .join("worktrees")
            .join(ws_name)
            .join(room);

        if worktree_path.exists() {
            Some(worktree_path)
        } else {
            // Default room: the workspace repo directory itself.
            Some(ws_entry.path.clone())
        }
    }

    /// Spawn a new pane from the named preset and register it.
    /// Returns the new `PaneId` on success.
    fn spawn_pane(&mut self, preset_name: &str) -> Option<PaneId> {
        let shell_cmd = self
            .config
            .presets
            .get(preset_name)
            .map(|p| p.command.as_str())
            .unwrap_or("sh")
            .to_string();
        let shell_args: Vec<String> = self
            .config
            .presets
            .get(preset_name)
            .map(|p| p.args.clone())
            .unwrap_or_default();
        let arg_refs: Vec<&str> = shell_args.iter().map(String::as_str).collect();
        let (cmd, args) = humu::preset::resolve_preset(&shell_cmd, &arg_refs);

        // Set HUMU_* env vars when spawning the "claude" preset.
        let envs: Vec<(String, String)> = if preset_name == "claude" {
            let sock = humu_dir().join("humu.sock");
            let workspace = self
                .state
                .active_workspace
                .clone()
                .unwrap_or_default();
            let room = self
                .state
                .active_room
                .clone()
                .unwrap_or_default();
            vec![
                ("HUMU_SOCKET".to_string(), sock.to_string_lossy().into_owned()),
                ("HUMU_WORKSPACE".to_string(), workspace),
                ("HUMU_ROOM".to_string(), room),
            ]
        } else {
            vec![]
        };

        let cwd = self.current_room_path();
        let pane = PtyPane::spawn_with_envs(&cmd, &args, cwd.as_deref(), 80, 24, &envs).ok()?;
        let id = self.next_pane_id;
        self.panes.insert(id, pane);
        self.pane_presets.insert(id, preset_name.to_string());
        self.next_pane_id += 1;
        Some(id)
    }

    fn new_tab_with_preset(&mut self, preset_name: &str) {
        if let Some(new_id) = self.spawn_pane(preset_name) {
            self.tabs.add_tab(preset_name.to_string(), SplitTree::leaf(new_id));
            let last = self.tabs.len() - 1;
            self.tabs.set_active(last);
            self.focused_pane = Some(new_id);
        }
    }

    fn close_tab(&mut self) {
        if self.tabs.len() <= 1 {
            return;
        }
        let active = self.tabs.active_index();
        if let Some(tree) = self.tabs.remove_tab(active) {
            for id in tree.pane_ids() {
                self.panes.remove(&id);
            }
            self.sync_focused_pane();
        }
    }

    /// After changing the active tab, set `focused_pane` to the first pane in that tab.
    fn sync_focused_pane(&mut self) {
        self.focused_pane = self
            .tabs
            .active_tree()
            .and_then(|t| t.pane_ids().into_iter().next());
    }

    fn split_pane_with_preset(&mut self, preset_name: &str, horizontal: bool) {
        let focused = match self.focused_pane {
            Some(id) => id,
            None => return,
        };
        let new_id = match self.spawn_pane(preset_name) {
            Some(id) => id,
            None => return,
        };
        if let Some(tree) = self.tabs.active_tree_mut() {
            if horizontal {
                tree.split_horizontal(focused, new_id);
            } else {
                tree.split_vertical(focused, new_id);
            }
            self.focused_pane = Some(new_id);
        } else {
            // No active tree — clean up the pane we just spawned.
            self.panes.remove(&new_id);
        }
    }

    fn toggle_fullscreen(&mut self) {
        if self.fullscreen_pane.is_some() {
            // Turn off fullscreen — restore normal split rendering.
            self.fullscreen_pane = None;
        } else {
            // Enter fullscreen with the currently focused pane.
            self.fullscreen_pane = self.focused_pane;
        }
    }

    fn close_pane(&mut self) {
        let focused = match self.focused_pane {
            Some(id) => id,
            None => return,
        };

        // Check if this is the only pane in the active tree.
        let only_pane = self
            .tabs
            .active_tree()
            .map(|t| t.pane_ids().len() == 1)
            .unwrap_or(false);

        if only_pane {
            // Close the tab (unless it's the last one).
            self.close_tab();
            return;
        }

        // Remove from the split tree first.
        if let Some(tree) = self.tabs.active_tree_mut() {
            tree.remove_pane(focused);
        }
        self.panes.remove(&focused);

        // Pick a new focused pane from remaining panes in the active tree.
        self.focused_pane = self
            .tabs
            .active_tree()
            .and_then(|t| t.pane_ids().into_iter().next());
    }

    fn move_focus(&mut self, dir: NavDirection) {
        let focused = match self.focused_pane {
            Some(id) => id,
            None => return,
        };

        // We need a temporary area to compute rects; use a fixed reference area.
        // The actual area is not known here, so we use a large fixed rect so
        // relative adjacency is still computed correctly.
        let area = Rect::new(0, 0, 1000, 1000);

        let rects = match self.tabs.active_tree() {
            Some(tree) => tree.compute_rects(area),
            None => return,
        };

        let focused_rect = match rects.iter().find(|(id, _)| *id == focused) {
            Some((_, r)) => *r,
            None => return,
        };

        let candidate = match dir {
            NavDirection::Left => rects
                .iter()
                .filter(|(id, r)| {
                    *id != focused
                        && r.x + r.width <= focused_rect.x
                        && ranges_overlap(r.y, r.y + r.height, focused_rect.y, focused_rect.y + focused_rect.height)
                })
                .max_by_key(|(_, r)| r.x + r.width),

            NavDirection::Right => rects
                .iter()
                .filter(|(id, r)| {
                    *id != focused
                        && r.x >= focused_rect.x + focused_rect.width
                        && ranges_overlap(r.y, r.y + r.height, focused_rect.y, focused_rect.y + focused_rect.height)
                })
                .min_by_key(|(_, r)| r.x),

            NavDirection::Up => rects
                .iter()
                .filter(|(id, r)| {
                    *id != focused
                        && r.y + r.height <= focused_rect.y
                        && ranges_overlap(r.x, r.x + r.width, focused_rect.x, focused_rect.x + focused_rect.width)
                })
                .max_by_key(|(_, r)| r.y + r.height),

            NavDirection::Down => rects
                .iter()
                .filter(|(id, r)| {
                    *id != focused
                        && r.y >= focused_rect.y + focused_rect.height
                        && ranges_overlap(r.x, r.x + r.width, focused_rect.x, focused_rect.x + focused_rect.width)
                })
                .min_by_key(|(_, r)| r.y),
        };

        if let Some((new_id, _)) = candidate {
            self.focused_pane = Some(*new_id);
            self.focus = FocusedPanel::Terminal;
        }
    }

    fn handle_passthrough(&mut self, key: KeyEvent) {
        if self.focus == FocusedPanel::Terminal
            && let Some(pane_id) = self.focused_pane
            && let Some(pane) = self.panes.get_mut(&pane_id)
        {
            let bytes = key_event_to_bytes(&key);
            if !bytes.is_empty() {
                let _ = pane.write_input(&bytes);
            }
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
                let count = self.room_items().len();
                if count > 0 {
                    let current = self.room_selected.unwrap_or(0) as i32;
                    let next = (current + delta).clamp(0, count as i32 - 1) as usize;
                    self.room_selected = Some(next);
                }
            }
            FocusedPanel::Terminal => {}
        }
    }

    fn select_current(&mut self) {
        match self.focus {
            FocusedPanel::Workspace | FocusedPanel::Room => {
                self.switch_to_selected_room();
            }
            FocusedPanel::Terminal => {}
        }
    }

    fn restore_selection(&mut self) {
        if let Some(ws_name) = self.state.active_workspace.clone() {
            let names: Vec<_> = {
                let mut n: Vec<_> = self.state.workspaces.keys().cloned().collect();
                n.sort();
                n
            };
            if let Some(idx) = names.iter().position(|n| *n == ws_name) {
                self.workspace_selected = Some(idx);
            }
        }

        // Find the active room index
        if let Some(room_name) = self.state.active_room.clone() {
            let rooms = self.room_items();
            if let Some(idx) = rooms.iter().position(|r| r.name == room_name) {
                self.room_selected = Some(idx);
            }
        }

        // Restore layout if saved
        if let (Some(ws), Some(room)) = (
            self.state.active_workspace.clone(),
            self.state.active_room.clone(),
        ) {
            if let Some(layout) = self
                .state
                .layout
                .get(&ws)
                .and_then(|m| m.get(&room))
                .cloned()
            {
                self.restore_layout(&layout);
            }
        }
    }

    /// Drain the hook event channel and update spinner_state.
    fn process_hook_events(&mut self) {
        if let Some(rx) = &self.hook_rx {
            while let Ok(event) = rx.try_recv() {
                let key = (event.workspace.clone(), event.room.clone());
                if event.hook_type == "Stop" {
                    self.spinner_state.remove(&key);
                } else {
                    self.spinner_state.insert(key, Instant::now());
                }
            }
        }
        // Timeout: remove entries older than 10 seconds.
        let now = Instant::now();
        self.spinner_state
            .retain(|_, time| now.duration_since(*time) < Duration::from_secs(10));
    }

    fn workspace_items(&self) -> Vec<WorkspaceItem> {
        let mut names: Vec<_> = self.state.workspaces.keys().cloned().collect();
        names.sort();
        names
            .into_iter()
            .map(|name| {
                let active = self
                    .spinner_state
                    .keys()
                    .any(|(ws, _)| ws == &name);
                WorkspaceItem { name, active }
            })
            .collect()
    }

    fn room_items(&self) -> Vec<RoomItem> {
        let ws_name = match &self.state.active_workspace {
            Some(name) => name,
            None => return vec![],
        };
        let ws = match self.state.workspaces.get(ws_name) {
            Some(ws) => ws,
            None => return vec![],
        };
        let mgr = RoomManager::new();
        match mgr.list(&ws.path) {
            Ok(rooms) => rooms
                .into_iter()
                .map(|r| {
                    let active = self
                        .spinner_state
                        .contains_key(&(ws_name.clone(), r.branch.clone()));
                    RoomItem {
                        name: r.branch,
                        is_default: r.is_default,
                        active,
                    }
                })
                .collect(),
            Err(_) => vec![],
        }
    }

    /// Convert the current runtime TabContainer into a `RoomLayout` for persistence.
    /// Returns `None` if there are no tabs.
    fn save_layout(&self) -> Option<RoomLayout> {
        if self.tabs.is_empty() {
            return None;
        }
        let tabs: Vec<TabLayout> = self
            .tabs
            .tab_names()
            .into_iter()
            .enumerate()
            .filter_map(|(i, name)| {
                // Temporarily obtain the tree via index by iterating — we use
                // a helper that borrows self immutably.
                let tree = self.tabs.tree_at(i)?;
                let split = Self::split_tree_to_node(tree, &self.pane_presets)?;
                Some(TabLayout {
                    name: name.to_string(),
                    split,
                })
            })
            .collect();

        if tabs.is_empty() {
            return None;
        }

        Some(RoomLayout {
            active_tab: self.tabs.active_index(),
            tabs,
        })
    }

    /// Recursively convert a runtime `SplitTree` to the serializable `SplitNode`.
    fn split_tree_to_node(
        tree: &SplitTree,
        pane_presets: &HashMap<PaneId, String>,
    ) -> Option<SplitNode> {
        match tree {
            SplitTree::Leaf(id) => {
                let preset = pane_presets.get(id)?.clone();
                Some(SplitNode::Leaf { preset })
            }
            SplitTree::Split {
                direction,
                ratio,
                children,
            } => {
                let left = Self::split_tree_to_node(&children.0, pane_presets)?;
                let right = Self::split_tree_to_node(&children.1, pane_presets)?;
                let dir = match direction {
                    SplitDirection::Vertical => CfgDir::Vertical,
                    SplitDirection::Horizontal => CfgDir::Horizontal,
                };
                Some(SplitNode::Split {
                    direction: dir,
                    ratio: *ratio,
                    children: vec![left, right],
                })
            }
        }
    }

    /// Persist the current layout for the active workspace/room into `self.state`.
    fn persist_layout(&mut self) {
        let ws = match self.state.active_workspace.clone() {
            Some(w) => w,
            None => return,
        };
        let room = match self.state.active_room.clone() {
            Some(r) => r,
            None => return,
        };
        if let Some(layout) = self.save_layout() {
            self.state
                .layout
                .entry(ws)
                .or_default()
                .insert(room, layout);
        }
    }

    /// Close all existing panes and rebuild the TabContainer from a saved `RoomLayout`.
    fn restore_layout(&mut self, layout: &RoomLayout) {
        // Drop all existing panes.
        self.panes.clear();
        self.pane_presets.clear();
        self.tabs = TabContainer::new();
        self.focused_pane = None;

        let active_tab = layout.active_tab;

        for tab_layout in &layout.tabs {
            match Self::node_to_split_tree(
                &tab_layout.split,
                &self.config,
                &mut self.panes,
                &mut self.pane_presets,
                &mut self.next_pane_id,
            ) {
                Some(tree) => {
                    self.tabs.add_tab(tab_layout.name.clone(), tree);
                }
                None => {
                    // Fallback: spawn a plain shell tab if restore fails for this tab.
                    if let Some(id) = self.spawn_pane("shell") {
                        self.tabs
                            .add_tab(tab_layout.name.clone(), SplitTree::leaf(id));
                    }
                }
            }
        }

        // If nothing was restored, create a default shell tab.
        if self.tabs.is_empty()
            && let Some(id) = self.spawn_pane("shell")
        {
            self.tabs.add_tab("shell".into(), SplitTree::leaf(id));
        }

        // Restore active tab.
        let last = self.tabs.len().saturating_sub(1);
        self.tabs.set_active(active_tab.min(last));
        self.sync_focused_pane();
    }

    /// Recursively convert a `SplitNode` (config) into a runtime `SplitTree`,
    /// spawning PTY panes as needed.
    fn node_to_split_tree(
        node: &SplitNode,
        config: &humu::config::HumuConfig,
        panes: &mut HashMap<PaneId, PtyPane>,
        pane_presets: &mut HashMap<PaneId, String>,
        next_id: &mut PaneId,
    ) -> Option<SplitTree> {
        match node {
            SplitNode::Leaf { preset } => {
                let shell_cmd = config
                    .presets
                    .get(preset.as_str())
                    .map(|p| p.command.as_str())
                    .unwrap_or("sh")
                    .to_string();
                let shell_args: Vec<String> = config
                    .presets
                    .get(preset.as_str())
                    .map(|p| p.args.clone())
                    .unwrap_or_default();
                let arg_refs: Vec<&str> = shell_args.iter().map(String::as_str).collect();
                let (cmd, args) = humu::preset::resolve_preset(&shell_cmd, &arg_refs);
                let pane = PtyPane::spawn(&cmd, &args, None, 80, 24).ok()?;
                let id = *next_id;
                panes.insert(id, pane);
                pane_presets.insert(id, preset.clone());
                *next_id += 1;
                Some(SplitTree::Leaf(id))
            }
            SplitNode::Split {
                direction,
                ratio,
                children,
            } => {
                // Config always stores exactly 2 children for a binary split.
                if children.len() < 2 {
                    return None;
                }
                let left = Self::node_to_split_tree(
                    &children[0],
                    config,
                    panes,
                    pane_presets,
                    next_id,
                )?;
                let right = Self::node_to_split_tree(
                    &children[1],
                    config,
                    panes,
                    pane_presets,
                    next_id,
                )?;
                let dir = match direction {
                    CfgDir::Vertical => SplitDirection::Vertical,
                    CfgDir::Horizontal => SplitDirection::Horizontal,
                };
                Some(SplitTree::Split {
                    direction: dir,
                    ratio: *ratio,
                    children: Box::new((left, right)),
                })
            }
        }
    }

    /// Switch to the room identified by the current workspace/room selection,
    /// saving the current layout first and restoring the new room's layout.
    fn switch_to_selected_room(&mut self) {
        // Resolve workspace name from index.
        let ws_name = {
            let mut names: Vec<_> = self.state.workspaces.keys().cloned().collect();
            names.sort();
            match self.workspace_selected {
                Some(i) => names.into_iter().nth(i),
                None => None,
            }
        };
        let ws_name = match ws_name {
            Some(w) => w,
            None => return,
        };

        // Resolve room name from the room_selected index.
        let room_name = {
            let rooms = self.room_items();
            match self.room_selected {
                Some(i) => rooms.into_iter().nth(i).map(|r| r.name),
                None => None,
            }
        };
        let room_name = match room_name {
            Some(r) => r,
            None => match &self.state.active_room {
                Some(r) => r.clone(),
                None => return,
            },
        };

        // Save current layout before switching.
        self.persist_layout();

        // Update active workspace/room in state.
        self.state.active_workspace = Some(ws_name.clone());
        self.state.active_room = Some(room_name.clone());

        // Restore layout for the new room, if any.
        let layout = self
            .state
            .layout
            .get(&ws_name)
            .and_then(|rooms| rooms.get(&room_name))
            .cloned();

        if let Some(layout) = layout {
            self.restore_layout(&layout);
        } else {
            // No saved layout — create a default shell tab.
            self.panes.clear();
            self.pane_presets.clear();
            self.tabs = TabContainer::new();
            self.focused_pane = None;
            if let Some(id) = self.spawn_pane("shell") {
                self.tabs.add_tab("shell".into(), SplitTree::leaf(id));
                self.focused_pane = Some(id);
            }
        }
    }
}

/// Returns true if the ranges [a_start, a_end) and [b_start, b_end) overlap.
fn ranges_overlap(a_start: u16, a_end: u16, b_start: u16, b_end: u16) -> bool {
    a_start < b_end && b_start < a_end
}

fn key_event_to_bytes(key: &KeyEvent) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char(c) if ctrl => vec![(c as u8) & 0x1f],
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
