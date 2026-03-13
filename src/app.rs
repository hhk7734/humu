use humu::config::{humu_dir, HumuConfig, HumuState};
use humu::tui::input::{handle_key, Action, Mode};
use humu::tui::widgets::room_panel::{RoomItem, RoomPanel};
use humu::tui::widgets::status_bar::StatusBar;
use humu::tui::widgets::workspace_panel::{WorkspaceItem, WorkspacePanel};
use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::layout::{Constraint, Direction, Layout};
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

        self.restore_selection();

        while self.running {
            terminal.draw(|frame| self.render(frame))?;

            if event::poll(Duration::from_millis(50))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                self.handle_action(handle_key(self.mode, key));
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

        // Terminal area placeholder
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
            Action::Quit => self.running = false,

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
