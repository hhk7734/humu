use humu::config::{humu_dir, HumuConfig, HumuState, RoomLayout, SplitDirection as CfgDir, SplitNode, TabLayout};
use humu::pty::pane::PtyPane;
use humu::tui::input::{handle_key, Action, Direction as NavDirection, Mode};
use humu::tui::layout::{PaneId, SplitDirection, SplitTree, TabContainer};
use humu::tui::widgets::room_panel::{RoomItem, RoomPanel};
use humu::tui::widgets::status_bar::StatusBar;
use humu::tui::widgets::terminal_area::TabBar;
use humu::tui::widgets::terminal_widget::TerminalWidget;
use humu::tui::widgets::workspace_panel::{WorkspaceItem, WorkspacePanel};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::Terminal;
use std::collections::HashMap;
use std::io::stdout;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedPanel {
    Workspace,
    Room,
    Terminal,
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

            if event::poll(Duration::from_millis(50))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                self.handle_action(handle_key(self.mode, key));
            }

            // Process PTY output each tick
            for pane in self.panes.values_mut() {
                let _ = pane.process_output();
            }
        }

        crossterm::execute!(stdout(), crossterm::event::DisableMouseCapture)?;
        stdout().execute(LeaveAlternateScreen)?;
        disable_raw_mode()?;

        // Save layout then persist state on exit.
        self.persist_layout();
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

        // Terminal area: tab bar (1 line) + pane area
        self.render_terminal_area(frame, panel_chunks[2]);

        // Status bar
        frame.render_widget(StatusBar::new(self.mode), main_chunks[1]);
    }

    fn render_terminal_area(&self, frame: &mut ratatui::Frame, area: Rect) {
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

        // Render tab bar
        let tab_names: Vec<&str> = self.tabs.tab_names();
        let active_indicators: Vec<bool> = vec![false; tab_names.len()];
        let tab_bar = TabBar::new(&tab_names, self.tabs.active_index(), &active_indicators);
        frame.render_widget(tab_bar, tab_bar_area);

        // Render panes from active tab's split tree
        if pane_area.height == 0 {
            return;
        }

        if let Some(tree) = self.tabs.active_tree() {
            let rects = tree.compute_rects(pane_area);
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
            Action::EnterMode(mode) => self.mode = mode,
            Action::ExitToNormal => self.mode = Mode::Normal,
            Action::Quit => self.running = false,

            Action::FocusWorkspacePanel => self.focus = FocusedPanel::Workspace,
            Action::FocusRoomPanel => self.focus = FocusedPanel::Room,

            Action::NavigateUp => self.navigate(-1),
            Action::NavigateDown => self.navigate(1),
            Action::Select => self.select_current(),

            Action::PassThrough(key) => self.handle_passthrough(key),

            // Tab actions
            Action::NewTab => self.new_tab(),
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

            // Split actions
            Action::SplitDown => self.split_pane(false),
            Action::SplitRight => self.split_pane(true),
            Action::ClosePane => self.close_pane(),
            Action::MoveFocus(dir) => self.move_focus(dir),

            _ => {}
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

        let pane = PtyPane::spawn(&cmd, &args, None, 80, 24).ok()?;
        let id = self.next_pane_id;
        self.panes.insert(id, pane);
        self.pane_presets.insert(id, preset_name.to_string());
        self.next_pane_id += 1;
        Some(id)
    }

    fn new_tab(&mut self) {
        if let Some(new_id) = self.spawn_pane("shell") {
            self.tabs.add_tab("shell".into(), SplitTree::leaf(new_id));
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

    fn split_pane(&mut self, horizontal: bool) {
        let focused = match self.focused_pane {
            Some(id) => id,
            None => return,
        };
        let new_id = match self.spawn_pane("shell") {
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
                // TODO: navigate rooms
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

        // For now room_selected is always None (room panel not yet wired);
        // we still handle the workspace-only case.
        let room_name = match &self.state.active_room {
            Some(r) => r.clone(),
            None => "default".to_string(),
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
