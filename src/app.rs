use humu::config::{humu_dir, HumuConfig, HumuState};
use humu::pty::pane::PtyPane;
use humu::tui::input::{handle_key, Action, Mode};
use humu::tui::layout::{PaneId, SplitTree, TabContainer};
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

            // TODO: implement remaining actions in later tasks
            _ => {}
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
