use humu::config::{humu_dir, HumuConfig, HumuState, SplitDirection as CfgDir, SplitNode, TabLayout};
use humu::id::{RoomId, TabId, WorkspaceId};
use humu::git::room::RoomManager;
use humu::git::workspace::WorkspaceManager;
use humu::hook::http::{generate_hook_files, AgentState, HookEvent, HookServer};
use humu::pty::pane::PtyPane;
use humu::tui::completion::complete_path;
use humu::tui::search::SearchState;
use humu::tui::input::{handle_key, hint_click_action, hint_click_action_right, Action, Direction as NavDirection, Mode};
use humu::tui::layout::{PaneId, SplitDirection, SplitTree, TabContainer};
use humu::tui::widgets::dialog::{Dialog, DialogField};
use humu::tui::widgets::preset_selector::PresetSelector;
use humu::tui::widgets::room_panel::{RoomItem, RoomPanel};
use humu::tui::widgets::status_bar::{self, StatusBar};
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
use std::time::Duration;
use tokio::runtime::Runtime;

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedPanel {
    Workspace,
    Room,
    Terminal,
    Explorer,
}

/// Cached room info with git stats, refreshed periodically.
#[allow(dead_code)]
struct CachedRoomInfo {
    branch: String,
    path: std::path::PathBuf,
    is_default: bool,
    diff_stat: Option<(usize, usize)>,
    ahead_behind: Option<(usize, usize)>,
}

/// Tracks the last-rendered rects for each major panel so mouse clicks can be
/// hit-tested without re-running the layout computation.
#[derive(Debug, Clone, Copy, Default)]
pub struct PanelRects {
    pub workspace: Rect,
    pub room: Rect,
    pub terminal: Rect,
    pub explorer: Rect,
    pub tab_bar: Rect,
    pub status_bar: Rect,
}


/// Active text selection state for mouse drag in terminal panes.
#[derive(Debug, Clone)]
pub struct TextSelection {
    pub pane_id: PaneId,
    pub pane_rect: Rect,
    /// Start position in vt100 screen coordinates (row, col).
    pub start: (u16, u16),
    /// Current end position in vt100 screen coordinates (row, col).
    pub end: (u16, u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetAction {
    NewTab,
    SplitDown,
    SplitRight,
}

/// Holds the live runtime state for a room so it can be suspended and restored
/// without killing PTY processes.
pub struct RoomState {
    pub panes: HashMap<PaneId, PtyPane>,
    pub tabs: TabContainer,
    pub pane_presets: HashMap<PaneId, String>,
    pub focused_pane: Option<PaneId>,
    pub fullscreen_pane: Option<PaneId>,
}

pub struct AgentStateEntry {
    pub state: AgentState,
    pub session_id: Option<String>,
}

pub enum PopupState {
    None,
    Settings {
        selected: usize,
    },
    SplitDirection,
    LogViewer {
        lines: Vec<String>,
        scroll: usize,
        h_scroll: usize,
        file_len: u64,
    },
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
    NotificationSettings {
        selected: usize,
    },
    NotificationTokenInput {
        field: NotificationField,
        value: String,
    },
    FloatingPane {
        pane_id: PaneId,
        title: String,
    },
    ErrorDialog {
        message: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum NotificationField {
    BotToken,
    ChatId,
}

#[allow(dead_code)]
pub struct App {
    pub config: HumuConfig,
    pub state: HumuState,
    pub mode: Mode,
    pub focus: FocusedPanel,
    pub workspace_selected: Option<WorkspaceId>,
    pub room_selected: Option<RoomId>,
    pub running: bool,
    pub panes: HashMap<PaneId, PtyPane>,
    pub tabs: TabContainer,
    pub focused_pane: Option<PaneId>,
    /// Tracks which preset name was used to spawn each pane.
    pub pane_presets: HashMap<PaneId, String>,
    /// Active popup (None when no popup is showing).
    pub popup: PopupState,
    /// Per-pane agent state from the HTTP hook server.
    pub agent_states: HashMap<PaneId, AgentStateEntry>,
    /// Receiver for hook events forwarded from the background tokio thread.
    pub hook_rx: Option<mpsc::Receiver<HookEvent>>,
    /// Port the HTTP hook server is listening on.
    pub hook_port: Option<u16>,
    /// Last-rendered panel rects used for mouse hit-testing.
    pub panel_rects: PanelRects,
    /// Panel widths: [workspace, room, explorer]. Used in the layout constraints.
    pub panel_widths: [u16; 3],
    /// True when a mouse-down was forwarded to the focused PTY (not humu UI).
    pub pty_mouse_active: bool,
    /// Whether the terminal window is focused (for focus-aware notifications).
    pub is_focused: bool,
    /// Active text selection in a terminal pane (when child has no mouse tracking).
    pub selection: Option<TextSelection>,
    /// When Some(id), only that pane is rendered filling the full terminal area.
    pub fullscreen_pane: Option<PaneId>,
    pub palette: humu::tui::theme::Palette,
    pub ui_config: humu::tui::theme::UiConfig,
    /// Counter incremented each event-loop tick for animating spinners.
    pub spin_tick: usize,
    /// Suspended room states keyed by (workspace_id, room_id).
    /// Holds live PTY panes so they survive room/workspace switches.
    pub suspended_rooms: HashMap<(WorkspaceId, RoomId), RoomState>,
    /// Active search state (None when not searching).
    pub search_state: Option<SearchState>,
    /// File explorer state for the right-side panel.
    pub explorer_state: humu::explorer::ExplorerState,
    /// Cached room list + git stats, refreshed periodically (~3s).
    room_cache: Vec<CachedRoomInfo>,
    /// Cached path to state.yaml.
    state_path: std::path::PathBuf,
    /// Notification manager for OS/Telegram alerts.
    notification_manager: humu::notification::NotificationManager,
    /// Path to config.yaml for persisting changes.
    config_path: std::path::PathBuf,
}

impl App {
    pub fn new() -> Result<Self> {
        humu::log::init();

        if let Err(e) = generate_hook_files(&humu_dir()) {
            humu::humu_log!("failed to generate hook files: {e}");
        }

        let config_path = humu_dir().join("config.yaml");
        let config_toml_path = humu_dir().join("config.toml");
        let state_path = humu_dir().join("state.yaml");

        let config = if config_path.exists() {
            HumuConfig::load(&config_path)?
        } else if config_toml_path.exists() {
            let cfg = HumuConfig::load_toml(&config_toml_path)?;
            cfg.save(&config_path)?;
            humu::humu_log!("Migrated config.toml → config.yaml");
            cfg
        } else {
            HumuConfig::default()
        };

        let notification_manager =
            humu::notification::NotificationManager::from_config(&config.notifications);

        let state = if state_path.exists() {
            HumuState::load(&state_path)?
        } else {
            HumuState::default()
        };

        let tabs = TabContainer::new();
        let panes = HashMap::new();
        let pane_presets = HashMap::new();

        // Start HTTP hook server in a background tokio runtime and forward events
        // over a std mpsc channel so the synchronous event loop can call try_recv().
        let (hook_tx, hook_rx) = mpsc::channel::<HookEvent>();
        let (port_tx, port_rx) = mpsc::channel::<u16>();
        thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                match HookServer::start().await {
                    Ok(server) => {
                        let _ = port_tx.send(server.port());
                        let mut rx = server.subscribe();
                        loop {
                            match rx.recv().await {
                                Ok(event) => {
                                    let _ = hook_tx.send(event);
                                }
                                Err(_) => break,
                            }
                        }
                    }
                    Err(e) => humu::humu_log!("hook server error: {e}"),
                }
            });
        });
        let hook_port = port_rx.recv().ok();

        // Write port file so external tools can discover the hook server.
        if let Some(port) = hook_port {
            let port_path = humu_dir().join("port");
            let _ = std::fs::write(&port_path, port.to_string());
        }

        let ui_config = humu::tui::theme::UiConfig {
            simplified_ui: config.ui.simplified_ui,
            rounded_corners: config.ui.rounded_corners,
        };

        let saved_panel_widths = state.panel_widths.unwrap_or([20, 18, 25]);

        Ok(Self {
            config,
            state,
            mode: Mode::Terminal,
            focus: FocusedPanel::Terminal,
            workspace_selected: None,
            room_selected: None,
            running: true,
            panes,
            tabs,
            focused_pane: None,
            pane_presets,
            popup: PopupState::None,
            agent_states: HashMap::new(),
            hook_rx: Some(hook_rx),
            hook_port,
            panel_rects: PanelRects::default(),
            panel_widths: saved_panel_widths,
            pty_mouse_active: false,
            is_focused: true,
            selection: None,
            fullscreen_pane: None,
            palette: humu::tui::theme::Palette::GITHUB_DARK,
            ui_config,
            spin_tick: 0,
            suspended_rooms: HashMap::new(),
            search_state: None,
            explorer_state: humu::explorer::ExplorerState::new(std::path::PathBuf::new()),
            room_cache: Vec::new(),
            state_path: humu_dir().join("state.yaml"),
            notification_manager,
            config_path,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        stdout().execute(EnterAlternateScreen)?;
        crossterm::execute!(
            stdout(),
            crossterm::event::EnableMouseCapture,
            crossterm::event::EnableBracketedPaste,
            crossterm::event::EnableFocusChange,
        )?;
        // Enable Kitty keyboard protocol for modifier-aware keys (Shift+Enter, etc.).
        // Silently ignored on terminals that don't support it.
        let keyboard_enhanced = crossterm::execute!(
            stdout(),
            crossterm::event::PushKeyboardEnhancementFlags(
                crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES,
            ),
        )
        .is_ok();

        let backend = ratatui::backend::CrosstermBackend::new(stdout());
        let mut terminal = Terminal::new(backend)?;

        self.restore_selection();

        // Initialize explorer with the active room's directory.
        if let Some(path) = self.current_room_path() {
            self.explorer_state = humu::explorer::ExplorerState::new(path);
            self.explorer_state.scan();
        }

        if self.state.active_room_id.is_none() {
            self.mode = Mode::Workspace;
            self.focus = FocusedPanel::Workspace;
        }

        while self.running {
            // Process PTY output before render so cursor position is up-to-date.
            for pane in self.panes.values_mut() {
                let _ = pane.process_output();
            }

            // Auto-close exited panes.
            self.cleanup_exited_panes();

            // Floating pane: resize PTY to match popup area, auto-close on exit.
            if let PopupState::FloatingPane { pane_id, .. } = &self.popup {
                let pane_id = *pane_id;
                let fp_area = self.floating_pane_area();
                let inner_w = fp_area.width.saturating_sub(2);
                let inner_h = fp_area.height.saturating_sub(2);
                if let Some(pane) = self.panes.get_mut(&pane_id) {
                    if pane.cols() != inner_w || pane.rows() != inner_h {
                        let _ = pane.resize(inner_w, inner_h);
                    }
                }
                if self.panes.get_mut(&pane_id).and_then(|p| p.exit_status()).is_some() {
                    self.panes.remove(&pane_id);
                    self.pane_presets.remove(&pane_id);
                    self.popup = PopupState::None;
                }
            }

            // Periodic rescan (~3s) to pick up git status changes.
            if self.spin_tick % 60 == 0 {
                if !self.explorer_state.root.as_os_str().is_empty() {
                    self.explorer_state.scan();
                }
                self.refresh_room_cache();
            }

            terminal.draw(|frame| self.render(frame))?;
            self.spin_tick = self.spin_tick.wrapping_add(1);

            // Drain all pending events before rendering to avoid
            // per-event renders when mouse moves queue up.
            while event::poll(Duration::from_millis(0))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        // Ctrl+Q is a global quit — bypass popups.
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && key.code == KeyCode::Char('q')
                            && !matches!(self.popup, PopupState::FloatingPane { .. })
                        {
                            self.handle_action(Action::Quit);
                        } else if self.handle_popup_key(key) {
                        } else {
                            self.handle_action(handle_key(self.mode, key));
                        }
                    }
                    Event::Mouse(mouse) => self.handle_mouse(mouse),
                    Event::Paste(text) => self.handle_paste_event(&text),
                    Event::FocusGained => { self.is_focused = true; }
                    Event::FocusLost => { self.is_focused = false; }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }
            // If no events were pending, wait up to 50ms for the next one.
            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        // Ctrl+Q is a global quit — bypass popups.
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && key.code == KeyCode::Char('q')
                            && !matches!(self.popup, PopupState::FloatingPane { .. })
                        {
                            self.handle_action(Action::Quit);
                        } else if self.handle_popup_key(key) {
                        } else {
                            self.handle_action(handle_key(self.mode, key));
                        }
                    }
                    Event::Mouse(mouse) => self.handle_mouse(mouse),
                    Event::Paste(text) => self.handle_paste_event(&text),
                    Event::FocusGained => { self.is_focused = true; }
                    Event::FocusLost => { self.is_focused = false; }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }

            // Process hook events each tick.
            self.process_hook_events();

            // Refresh log viewer if open.
            self.refresh_log_viewer();

        }

        if keyboard_enhanced {
            let _ = crossterm::execute!(stdout(), crossterm::event::PopKeyboardEnhancementFlags);
        }
        crossterm::execute!(
            stdout(),
            crossterm::event::DisableBracketedPaste,
            crossterm::event::DisableMouseCapture,
            crossterm::event::DisableFocusChange,
        )?;
        stdout().execute(LeaveAlternateScreen)?;
        disable_raw_mode()?;

        // Graceful shutdown: sync layout for current room and all suspended rooms,
        // then write once to disk.
        self.sync_layout();
        self.panes.clear();

        // Sync suspended rooms into state.
        let suspended: Vec<_> = self.suspended_rooms.drain().collect();
        for ((ws_id, room_id), room_state) in suspended {
            // Temporarily swap in the suspended state to reuse persist helpers.
            self.panes = room_state.panes;
            self.tabs = room_state.tabs;
            self.pane_presets = room_state.pane_presets;
            let layout = self.save_layout();
            if let Some(ws) = self.state.ws_by_id_mut(ws_id) {
                if let Some(room) = ws.room_by_id_mut(room_id) {
                    if let Some((active_tab, tabs)) = layout {
                        room.active_tab = Some(active_tab);
                        room.tabs = tabs;
                    } else {
                        room.active_tab = None;
                        room.tabs.clear();
                    }
                }
            }
            self.panes.clear();
        }

        self.save_state();

        Ok(())
    }

    /// Handle a key event when a popup is active.
    /// Returns `true` if the key was consumed (popup was active), `false` otherwise.
    fn handle_popup_key(&mut self, key: KeyEvent) -> bool {
        match &self.popup {
            PopupState::None => false,

            PopupState::Settings { .. } => {
                self.handle_settings_key(key);
                true
            }
            PopupState::SplitDirection => {
                self.handle_split_direction_key(key);
                true
            }
            PopupState::LogViewer { .. } => {
                self.handle_log_viewer_key(key);
                true
            }
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
            PopupState::NotificationSettings { .. } => {
                self.handle_notification_settings_key(key);
                true
            }
            PopupState::NotificationTokenInput { .. } => {
                self.handle_notification_token_input_key(key);
                true
            }
            PopupState::FloatingPane { pane_id, .. } => {
                let pane_id = *pane_id;
                self.handle_floating_pane_key(pane_id, key);
                true
            }
            PopupState::ErrorDialog { .. } => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char(' ')) {
                    self.popup = PopupState::None;
                }
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
            KeyCode::Down => {
                if selected + 1 < presets.len() {
                    selected += 1;
                }
                self.popup = PopupState::PresetSelector { presets, selected, action };
            }
            KeyCode::Up => {
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

    const SETTINGS_ITEMS: &'static [&'static str] = &["Notifications", "View Logs"];

    fn handle_settings_key(&mut self, key: KeyEvent) {
        let PopupState::Settings { selected } = &self.popup else {
            return;
        };
        let mut selected = *selected;

        match key.code {
            KeyCode::Down => {
                if selected + 1 < Self::SETTINGS_ITEMS.len() {
                    selected += 1;
                }
                self.popup = PopupState::Settings { selected };
            }
            KeyCode::Up => {
                selected = selected.saturating_sub(1);
                self.popup = PopupState::Settings { selected };
            }
            KeyCode::Enter => {
                match selected {
                    0 => {
                        self.popup = PopupState::NotificationSettings { selected: 0 };
                    }
                    1 => {
                        self.popup = PopupState::None;
                        self.open_log_viewer();
                    }
                    _ => {}
                }
            }
            KeyCode::Esc => {
                self.popup = PopupState::None;
            }
            _ => {}
        }
    }

    fn notification_settings_items(&self) -> Vec<String> {
        let cfg = &self.config.notifications;
        let on_off = |b: bool| if b { "ON" } else { "OFF" };
        vec![
            format!("OS Notifications: {}", on_off(cfg.os.enabled)),
            format!("OS Only Unfocused: {}", on_off(cfg.os.only_unfocused)),
            format!("Sound: {}", on_off(cfg.sound.enabled)),
            format!("Sound Only Unfocused: {}", on_off(cfg.sound.only_unfocused)),
            format!("Telegram: {}", on_off(cfg.telegram.enabled)),
            format!("Telegram Only Unfocused: {}", on_off(cfg.telegram.only_unfocused)),
            format!("Telegram Bot Token: {}", if cfg.telegram.bot_token_encrypted.is_empty() { "(not set)" } else { "****" }),
            format!("Telegram Chat ID: {}", if cfg.telegram.chat_id_encrypted.is_empty() { "(not set)" } else { "****" }),
        ]
    }

    fn handle_notification_settings_key(&mut self, key: KeyEvent) {
        let PopupState::NotificationSettings { selected } = &self.popup else {
            return;
        };
        let mut selected = *selected;
        let item_count = 8;

        match key.code {
            KeyCode::Down => {
                if selected + 1 < item_count {
                    selected += 1;
                }
                self.popup = PopupState::NotificationSettings { selected };
            }
            KeyCode::Up => {
                selected = selected.saturating_sub(1);
                self.popup = PopupState::NotificationSettings { selected };
            }
            KeyCode::Enter | KeyCode::Char(' ') => match selected {
                0 => {
                    self.config.notifications.os.enabled =
                        !self.config.notifications.os.enabled;
                    self.rebuild_notification_manager();
                }
                1 => {
                    self.config.notifications.os.only_unfocused =
                        !self.config.notifications.os.only_unfocused;
                    self.rebuild_notification_manager();
                }
                2 => {
                    self.config.notifications.sound.enabled =
                        !self.config.notifications.sound.enabled;
                    self.rebuild_notification_manager();
                }
                3 => {
                    self.config.notifications.sound.only_unfocused =
                        !self.config.notifications.sound.only_unfocused;
                    self.rebuild_notification_manager();
                }
                4 => {
                    self.config.notifications.telegram.enabled =
                        !self.config.notifications.telegram.enabled;
                    self.rebuild_notification_manager();
                }
                5 => {
                    self.config.notifications.telegram.only_unfocused =
                        !self.config.notifications.telegram.only_unfocused;
                    self.rebuild_notification_manager();
                }
                6 => {
                    self.popup = PopupState::NotificationTokenInput {
                        field: NotificationField::BotToken,
                        value: String::new(),
                    };
                    return;
                }
                7 => {
                    self.popup = PopupState::NotificationTokenInput {
                        field: NotificationField::ChatId,
                        value: String::new(),
                    };
                    return;
                }
                _ => {}
            },
            KeyCode::Esc => {
                self.popup = PopupState::Settings { selected: 0 };
            }
            _ => {}
        }
    }

    fn handle_notification_token_input_key(&mut self, key: KeyEvent) {
        let PopupState::NotificationTokenInput { field, value } = &self.popup else {
            return;
        };
        let field = *field;
        let mut value = value.clone();

        match key.code {
            KeyCode::Enter => {
                let encrypted =
                    humu::notification::crypto::encrypt(&value).unwrap_or_default();
                match field {
                    NotificationField::BotToken => {
                        self.config.notifications.telegram.bot_token_encrypted =
                            encrypted;
                    }
                    NotificationField::ChatId => {
                        self.config.notifications.telegram.chat_id_encrypted =
                            encrypted;
                    }
                }
                self.rebuild_notification_manager();
                self.popup = PopupState::NotificationSettings { selected: 0 };
                return;
            }
            KeyCode::Esc => {
                self.popup = PopupState::NotificationSettings { selected: 0 };
                return;
            }
            KeyCode::Backspace => {
                value.pop();
            }
            KeyCode::Char(c) => {
                value.push(c);
            }
            _ => {}
        }
        self.popup = PopupState::NotificationTokenInput { field, value };
    }

    fn handle_floating_pane_key(&mut self, pane_id: PaneId, key: KeyEvent) {
        // Ctrl+Q or Ctrl+G closes the floating pane.
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('q') | KeyCode::Char('g'))
        {
            self.panes.remove(&pane_id);
            self.pane_presets.remove(&pane_id);
            self.popup = PopupState::None;
            return;
        }
        // Forward all keys to the PTY
        if let Some(pane) = self.panes.get_mut(&pane_id) {
            let bytes = key_event_to_bytes(&key);
            if !bytes.is_empty() {
                let _ = pane.write_input(&bytes);
            }
        }
    }

    /// Forward a mouse event to the floating pane's PTY. Returns true if handled.
    fn forward_mouse_to_floating_pane(&mut self, pane_id: PaneId, mouse: &crossterm::event::MouseEvent) -> bool {
        let popup_area = self.floating_pane_area();

        let pos = Position::new(mouse.column, mouse.row);
        if !popup_area.contains(pos) {
            return false;
        }

        let pane = match self.panes.get_mut(&pane_id) {
            Some(p) => p,
            None => return false,
        };

        // If child has no mouse reporting, convert scroll to arrow keys
        // and consume other mouse events.
        if pane.mouse_protocol_mode() == vt100::MouseProtocolMode::None {
            let lines_per_tick = 3;
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    for _ in 0..lines_per_tick {
                        let _ = pane.write_input(b"k");
                    }
                }
                MouseEventKind::ScrollDown => {
                    for _ in 0..lines_per_tick {
                        let _ = pane.write_input(b"j");
                    }
                }
                _ => {}
            }
            return true;
        }

        // Pane-relative coordinates (inside border)
        let col = mouse.column.saturating_sub(popup_area.x + 1) as u32;
        let row = mouse.row.saturating_sub(popup_area.y + 1) as u32;

        let (button, press) = match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => (0u32, true),
            MouseEventKind::Down(MouseButton::Right) => (2, true),
            MouseEventKind::Down(MouseButton::Middle) => (1, true),
            MouseEventKind::Up(MouseButton::Left) => (0, false),
            MouseEventKind::Up(MouseButton::Right) => (2, false),
            MouseEventKind::Up(MouseButton::Middle) => (1, false),
            MouseEventKind::Drag(MouseButton::Left) => (32, true),
            MouseEventKind::Drag(MouseButton::Right) => (34, true),
            MouseEventKind::Drag(MouseButton::Middle) => (33, true),
            MouseEventKind::ScrollUp => (64, true),
            MouseEventKind::ScrollDown => (65, true),
            MouseEventKind::Moved => (35, true),
            _ => return false,
        };

        let encoding = pane.mouse_protocol_encoding();
        let seq = match encoding {
            vt100::MouseProtocolEncoding::Sgr => {
                let suffix = if press { 'M' } else { 'm' };
                format!("\x1b[<{};{};{}{}", button, col + 1, row + 1, suffix)
            }
            _ => {
                let b = (button + 32) as u8;
                let c = ((col + 33).min(255)) as u8;
                let r = ((row + 33).min(255)) as u8;
                format!("\x1b[M{}{}{}", b as char, c as char, r as char)
            }
        };
        let _ = pane.write_input(seq.as_bytes());
        true
    }

    fn show_error(&mut self, message: impl Into<String>) {
        self.popup = PopupState::ErrorDialog { message: message.into() };
    }

    fn rebuild_notification_manager(&mut self) {
        self.notification_manager =
            humu::notification::NotificationManager::from_config(&self.config.notifications);
        if let Err(e) = self.config.save(&self.config_path) {
            humu::humu_log!("failed to save config: {e}");
        }
    }

    fn handle_split_direction_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Down => {
                self.popup = PopupState::None;
                self.show_preset_selector(PresetAction::SplitDown);
            }
            KeyCode::Right => {
                self.popup = PopupState::None;
                self.show_preset_selector(PresetAction::SplitRight);
            }
            KeyCode::Esc => {
                self.popup = PopupState::None;
            }
            _ => {}
        }
    }

    fn read_log_lines() -> (Vec<String>, u64) {
        let path = humu::log::log_path();
        let file_len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let lines = match std::fs::read_to_string(&path) {
            Ok(content) => content.lines().map(String::from).collect(),
            Err(_) => vec![],
        };
        (lines, file_len)
    }

    fn refresh_log_viewer(&mut self) {
        if let PopupState::LogViewer { lines, scroll, file_len, .. } = &mut self.popup {
            let current_len = std::fs::metadata(humu::log::log_path())
                .map(|m| m.len())
                .unwrap_or(0);
            if current_len == *file_len {
                return;
            }
            let was_at_end = *scroll + 1 >= lines.len();
            let (new_lines, new_len) = Self::read_log_lines();
            *lines = new_lines;
            *file_len = new_len;
            if was_at_end {
                *scroll = lines.len().saturating_sub(1);
            }
        }
    }

    fn open_log_viewer(&mut self) {
        let (lines, file_len) = Self::read_log_lines();
        let scroll = lines.len().saturating_sub(1);
        self.popup = PopupState::LogViewer { lines, scroll, h_scroll: 0, file_len };
    }

    fn handle_log_viewer_key(&mut self, key: KeyEvent) {
        let PopupState::LogViewer { lines, scroll, h_scroll, .. } = &self.popup else {
            return;
        };
        let total = lines.len();
        let mut scroll = *scroll;
        let mut h_scroll = *h_scroll;

        match key.code {
            KeyCode::Esc => {
                self.popup = PopupState::None;
                return;
            }
            KeyCode::Up => scroll = scroll.saturating_sub(1),
            KeyCode::Down => {
                if scroll + 1 < total {
                    scroll += 1;
                }
            }
            KeyCode::Left => h_scroll = h_scroll.saturating_sub(10),
            KeyCode::Right => h_scroll += 10,
            KeyCode::PageUp => scroll = scroll.saturating_sub(20),
            KeyCode::PageDown => scroll = (scroll + 20).min(total.saturating_sub(1)),
            KeyCode::Home => {
                scroll = 0;
                h_scroll = 0;
            }
            KeyCode::End => scroll = total.saturating_sub(1),
            _ => {}
        }

        if let PopupState::LogViewer { scroll: ref mut s, h_scroll: ref mut hs, .. } = self.popup {
            *s = scroll;
            *hs = h_scroll;
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
            self.show_error("Path is required");
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
                    self.show_error("URL is required for Clone");
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
            Ok(name) => {
                // Auto-select the new workspace and its default room.
                if let Some(ws) = self.state.ws_by_name(&name) {
                    self.workspace_selected = Some(ws.id);
                    self.room_selected = None;
                    self.switch_to_selected_room();
                } else {
                    self.save_state();
                }
            }
            Err(e) => {
                self.show_error(e.to_string());
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
            self.show_error("Branch name is required");
            return;
        }

        let ws_name = match self.active_workspace_name() {
            Some(w) => w,
            None => {
                self.show_error("No active workspace");
                return;
            }
        };
        let ws_path = match self.state.ws_by_name(&ws_name) {
            Some(e) => e.path.clone(),
            None => {
                self.show_error("Workspace not found");
                return;
            }
        };
        let base = if base_branch.is_empty() { "HEAD" } else { &base_branch };
        let worktree_path = humu_dir()
            .join("worktrees")
            .join(&ws_name)
            .join(&branch);
        let mgr = RoomManager::new();
        if let Err(e) = mgr.create(&ws_path, &branch, base, &worktree_path) {
            self.show_error(e.to_string());
        }
    }

    fn execute_workspace_delete(&mut self, fields: Vec<DialogField>, workspace_name: String) {
        // Field 0: Confirm — also used as "remove from disk?" prompt
        let remove_from_disk = match &fields[0] {
            DialogField::Confirm { yes, .. } => *yes,
            _ => false,
        };

        // Capture the workspace ID before deletion so we can clean up
        // suspended rooms and detect if the active workspace is being deleted.
        let ws_id = self
            .state
            .ws_by_name(&workspace_name)
            .map(|e| e.id);

        let was_active = ws_id == self.state.active_workspace_id;

        // If the active workspace is being deleted and its panes are live,
        // clear them first (they'll be invalid after deletion).
        if was_active {
            self.panes.clear();
            self.pane_presets.clear();
            self.tabs = TabContainer::new();
            self.focused_pane = None;
            self.fullscreen_pane = None;
        }

        let mgr = WorkspaceManager::new();
        match mgr.delete(&mut self.state, &workspace_name, remove_from_disk) {
            Ok(()) => {
                // Remove all suspended rooms belonging to the deleted workspace.
                if let Some(ws_id) = ws_id {
                    self.suspended_rooms
                        .retain(|(wid, _), _| *wid != ws_id);
                }

                // Adjust selection if needed.
                if self.state.workspaces.is_empty() {
                    self.workspace_selected = None;
                    self.room_selected = None;
                    self.save_state();
                } else if was_active {
                    // Select first available workspace.
                    let items = self.workspace_items();
                    if let Some(first) = items.first() {
                        self.workspace_selected = Some(first.id);
                    }
                    self.room_selected = None;
                    self.switch_to_selected_room();
                } else {
                    self.save_state();
                }
            }
            Err(e) => {
                self.show_error(e.to_string());
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
        let ws_name = match self.active_workspace_name() {
            Some(w) => w,
            None => {
                self.show_error("No active workspace");
                return;
            }
        };
        let ws_path = match self.state.ws_by_name(&ws_name) {
            Some(e) => e.path.clone(),
            None => {
                self.show_error("Workspace not found");
                return;
            }
        };
        let worktree_path = humu_dir()
            .join("worktrees")
            .join(&ws_name)
            .join(&branch);
        let mgr = RoomManager::new();
        if let Err(e) = mgr.delete(&ws_path, &branch, &worktree_path) {
            self.show_error(e.to_string());
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
                Constraint::Length(self.panel_widths[2]),
            ])
            .split(main_chunks[0]);

        // Store rects for mouse hit-testing.
        let tab_bar_rect = Rect::new(panel_chunks[2].x, panel_chunks[2].y, panel_chunks[2].width, 1);
        self.panel_rects = PanelRects {
            workspace: panel_chunks[0],
            room: panel_chunks[1],
            terminal: panel_chunks[2],
            explorer: panel_chunks[3],
            tab_bar: tab_bar_rect,
            status_bar: main_chunks[1],
        };

        // Compute animated spinner frame (~100ms per frame at 50ms tick).
        let spinner_frame = SPINNER_FRAMES[self.spin_tick / 2 % SPINNER_FRAMES.len()];

        // Workspace panel
        let workspaces = self.workspace_items();
        let ws_selected_idx = self.workspace_selected.and_then(|id| {
            workspaces.iter().position(|w| w.id == id)
        });
        let ws_widget = WorkspacePanel::new(&workspaces, &self.palette, &self.ui_config)
            .selected(ws_selected_idx)
            .focus(self.focus == FocusedPanel::Workspace)
            .spinner(spinner_frame);
        frame.render_widget(ws_widget, panel_chunks[0]);

        // Room panel
        let rooms = self.room_items();
        let room_selected_idx = self.room_selected.and_then(|id| {
            rooms.iter().position(|r| r.id == Some(id))
        });
        let room_widget = RoomPanel::new(&rooms, &self.palette, &self.ui_config)
            .selected(room_selected_idx)
            .focus(self.focus == FocusedPanel::Room)
            .spinner(spinner_frame);
        frame.render_widget(room_widget, panel_chunks[1]);

        // Terminal area: tab bar (1 line) + pane area
        self.render_terminal_area(frame, panel_chunks[2]);

        // Explorer panel
        let explorer_widget = humu::tui::widgets::explorer_panel::ExplorerPanel::new(
            &self.explorer_state,
            &self.palette,
            &self.ui_config,
        ).focus(self.focus == FocusedPanel::Explorer);
        frame.render_widget(explorer_widget, panel_chunks[3]);

        // Status bar
        let mut status = StatusBar::new(self.mode, &self.palette, &self.ui_config);
        if let Some(ref state) = self.search_state {
            status = status
                .search_query(Some(&state.query))
                .search_valid(state.is_valid_regex());
            if self.mode == Mode::Search {
                let active = state.active_index.map(|i| i + 1).unwrap_or(0);
                let total = state.matches.len();
                status = status.search_info(Some((active, total, state.case_sensitive, state.wrap)));
            }
        }
        frame.render_widget(status, main_chunks[1]);

        // Render popup on top of everything when active.
        self.render_popup(frame, size);
    }

    fn render_log_viewer(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
        lines: &[String],
        scroll: usize,
        h_scroll: usize,
    ) {
        use ratatui::style::{Modifier, Style};
        use ratatui::widgets::{Block, BorderType, Borders, Clear, Widget};

        let width = (area.width - 4).min(100);
        let height = (area.height - 2).min(40);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup = Rect::new(x, y, width, height);

        Clear.render(popup, frame.buffer_mut());

        let border_type = if self.ui_config.rounded_corners {
            BorderType::Rounded
        } else {
            BorderType::Plain
        };
        let title = format!(" Logs ({}/{}) ", scroll + 1, lines.len().max(1));
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.palette.accent_blue))
            .border_type(border_type);
        let inner = block.inner(popup);
        block.render(popup, frame.buffer_mut());

        let visible_height = inner.height as usize;
        let max_width = inner.width as usize;
        let start = scroll.saturating_sub(visible_height.saturating_sub(1));
        let text_style = Style::default().fg(self.palette.fg_primary);
        let muted_style = Style::default().fg(self.palette.fg_muted);
        let ellipsis_style = Style::default().fg(self.palette.fg_muted);

        for (i, line_idx) in (start..lines.len()).enumerate() {
            if i >= visible_height {
                break;
            }
            let line = &lines[line_idx];
            let style = if line_idx == scroll {
                text_style.add_modifier(Modifier::BOLD)
            } else {
                muted_style
            };

            let chars: Vec<char> = line.chars().collect();
            let total_chars = chars.len();
            let visible: String = chars.iter().skip(h_scroll).take(max_width).collect();
            let truncated_left = h_scroll > 0;
            let truncated_right = h_scroll + max_width < total_chars;

            if truncated_left && max_width > 3 {
                frame.buffer_mut().set_string(inner.x, inner.y + i as u16, "...", ellipsis_style);
                let rest: String = chars.iter().skip(h_scroll + 3).take(max_width - 3).collect();
                frame.buffer_mut().set_string(inner.x + 3, inner.y + i as u16, &rest, style);
            } else {
                frame.buffer_mut().set_string(inner.x, inner.y + i as u16, &visible, style);
            }
            if truncated_right && max_width > 3 {
                let col = inner.x + max_width as u16 - 3;
                frame.buffer_mut().set_string(col, inner.y + i as u16, "...", ellipsis_style);
            }
        }
    }

    fn render_popup(&self, frame: &mut ratatui::Frame, area: Rect) {
        match &self.popup {
            PopupState::None => {}
            PopupState::Settings { selected } => {
                let items: Vec<String> = Self::SETTINGS_ITEMS.iter().map(|s| s.to_string()).collect();
                frame.render_widget(
                    PresetSelector::new(&items, *selected, &self.palette, &self.ui_config)
                        .title(" Settings "),
                    area,
                );
            }
            PopupState::SplitDirection => {
                let items = vec![
                    "\u{2193} Down".to_string(),
                    "\u{2192} Right".to_string(),
                ];
                frame.render_widget(
                    PresetSelector::new(&items, usize::MAX, &self.palette, &self.ui_config)
                        .title(" Press \u{2193} or \u{2192} "),
                    area,
                );
            }
            PopupState::LogViewer { lines, scroll, h_scroll, .. } => {
                self.render_log_viewer(frame, area, lines, *scroll, *h_scroll);
            }
            PopupState::PresetSelector { presets, selected, .. } => {
                frame.render_widget(
                    PresetSelector::new(presets, *selected, &self.palette, &self.ui_config),
                    area,
                );
            }
            PopupState::WorkspaceCreate {
                fields,
                focused_field,
                completions,
                completion_selected,
            } => {
                let mut dialog = Dialog::new(
                    "Create Workspace",
                    fields,
                    *focused_field,
                    &self.palette,
                    &self.ui_config,
                );
                dialog.completions = completions;
                dialog.completion_selected = *completion_selected;
                dialog.completion_field = Some(1); // Path field
                frame.render_widget(dialog, area);
            }
            PopupState::RoomCreate { fields, focused_field } => {
                frame.render_widget(
                    Dialog::new("Create Room", fields, *focused_field, &self.palette, &self.ui_config),
                    area,
                );
            }
            PopupState::WorkspaceDelete { fields, focused_field, .. } => {
                frame.render_widget(
                    Dialog::new("Delete Workspace", fields, *focused_field, &self.palette, &self.ui_config),
                    area,
                );
            }
            PopupState::RoomDelete { fields, focused_field, .. } => {
                frame.render_widget(
                    Dialog::new("Delete Room", fields, *focused_field, &self.palette, &self.ui_config),
                    area,
                );
            }
            PopupState::NotificationSettings { selected } => {
                let items = self.notification_settings_items();
                let selector =
                    PresetSelector::new(&items, *selected, &self.palette, &self.ui_config)
                        .title(" Notifications ");
                frame.render_widget(selector, area);
            }
            PopupState::NotificationTokenInput { field, value } => {
                let title = match field {
                    NotificationField::BotToken => " Bot Token ",
                    NotificationField::ChatId => " Chat ID ",
                };
                let fields = vec![DialogField::TextInput {
                    label: title.trim().to_string(),
                    value: value.clone(),
                }];
                let dialog =
                    Dialog::new(title.trim(), &fields, 0, &self.palette, &self.ui_config);
                frame.render_widget(dialog, area);
            }
            PopupState::FloatingPane { pane_id, title } => {
                use ratatui::widgets::Clear;
                use humu::tui::widgets::terminal_widget::TerminalWidget;

                let popup_area = self.floating_pane_area();

                frame.render_widget(Clear, popup_area);

                if let Some(pane) = self.panes.get(pane_id) {
                    let parser = pane.parser_ref().lock().unwrap();
                    let screen = parser.screen();
                    let tw = TerminalWidget::new(screen, title, &self.palette, &self.ui_config)
                        .focus(true)
                        .pane_count(1);
                    frame.render_widget(tw, popup_area);

                    // Show cursor inside the floating pane.
                    if !screen.hide_cursor() {
                        let (crow, ccol) = screen.cursor_position();
                        let cx = popup_area.x + 1 + ccol;
                        let cy = popup_area.y + 1 + crow;
                        if cx < popup_area.x + popup_area.width && cy < popup_area.y + popup_area.height {
                            frame.set_cursor_position(Position::new(cx, cy));
                        }
                    }
                }
            }
            PopupState::ErrorDialog { message } => {
                use ratatui::style::{Modifier, Style};
                use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

                let lines: Vec<&str> = message.lines().collect();
                let max_line_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(10);
                let width = (max_line_len as u16 + 6).min(area.width - 4).max(20);
                let height = (lines.len() as u16 + 4).min(area.height - 2).max(5);
                let x = area.x + (area.width.saturating_sub(width)) / 2;
                let y = area.y + (area.height.saturating_sub(height)) / 2;
                let popup_area = Rect::new(x, y, width, height);

                frame.render_widget(Clear, popup_area);

                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(self.palette.accent_red))
                    .title(" Error ")
                    .title_style(Style::default().fg(self.palette.accent_red).add_modifier(Modifier::BOLD));

                let paragraph = Paragraph::new(message.as_str())
                    .style(Style::default().fg(self.palette.fg_primary))
                    .block(block)
                    .wrap(Wrap { trim: false });

                frame.render_widget(paragraph, popup_area);
            }
        }
    }

    fn render_terminal_area(&mut self, frame: &mut ratatui::Frame, area: Rect) {
        if area.height == 0 {
            return;
        }

        // Split into tab bar (1 row) and pane content area
        let tab_bar_area = Rect::new(area.x, area.y, area.width, 1);
        let pane_area = self.terminal_pane_area();

        // Pre-compute search highlight data for the focused pane.
        // Search matches use viewport-relative rows (0 = top of visible area).
        let search_base_row = 0;
        let (search_matches, search_active) = match &self.search_state {
            Some(state) if matches!(self.mode, Mode::EnterSearch | Mode::Search) => {
                (state.matches.as_slice(), state.active_index)
            }
            _ => ([].as_slice(), None),
        };

        // Render tab bar — a tab is active if any of its panes has a non-Idle
        // agent state in agent_states.
        let tab_names: Vec<&str> = self.tabs.tab_names();
        let active_indicators: Vec<bool> = (0..tab_names.len())
            .map(|i| {
                let tree = match self.tabs.tree_at(i) {
                    Some(t) => t,
                    None => return false,
                };
                tree.pane_ids().iter().any(|pid| {
                    self.agent_states
                        .get(pid)
                        .map(|e| matches!(e.state, AgentState::Working | AgentState::NeedsInput))
                        .unwrap_or(false)
                })
            })
            .collect();
        let spinner_frame = SPINNER_FRAMES[self.spin_tick / 2 % SPINNER_FRAMES.len()];
        let tab_bar = TabBar::new(&tab_names, self.tabs.active_index(), &active_indicators, &self.palette, &self.ui_config)
            .spinner(spinner_frame);
        frame.render_widget(tab_bar, tab_bar_area);

        // Render panes from active tab's split tree
        if pane_area.height == 0 {
            return;
        }

        // Fullscreen mode: render only the fullscreen pane filling the whole area.
        if let Some(fs_id) = self.fullscreen_pane {
            if let Some(pane) = self.panes.get_mut(&fs_id) {
                let inner_w = pane_area.width.saturating_sub(2);
                let inner_h = pane_area.height.saturating_sub(2);
                if pane.cols() != inner_w || pane.rows() != inner_h {
                    let _ = pane.resize(inner_w, inner_h);
                }
            }
            let fs_exit_code = self.panes.get_mut(&fs_id).and_then(|p| p.exit_status());
            let fs_pane_count = self
                .tabs
                .active_tree()
                .map(|t| t.pane_ids().len())
                .unwrap_or(1);
            if let Some(pane) = self.panes.get(&fs_id) {
                let screen = pane.screen();
                let preset_name = self
                    .pane_presets
                    .get(&fs_id)
                    .map(|s| s.as_str())
                    .unwrap_or("shell");
                let sel = self.selection_for_pane(fs_id);
                let widget =
                    TerminalWidget::new(&screen, preset_name, &self.palette, &self.ui_config)
                        .focus(true)
                        .exited(fs_exit_code)
                        .pane_count(fs_pane_count)
                        .search(search_matches, search_active, search_base_row)
                        .selection(sel);
                frame.render_widget(widget, pane_area);
                if fs_exit_code.is_none() && screen.scrollback() == 0 {
                    let (crow, ccol) = screen.cursor_position();
                    let cx = pane_area.x + 1 + ccol;
                    let cy = pane_area.y + 1 + crow;
                    if cx < pane_area.right() - 1 && cy < pane_area.bottom() - 1 {
                        frame.set_cursor_position(Position::new(cx, cy));
                    }
                }
            }
            return;
        }

        if let Some(tree) = self.tabs.active_tree() {
            let rects = tree.compute_rects(pane_area);
            let pane_count = rects.len();
            for (pane_id, rect) in &rects {
                // Resize pane if its dimensions have changed since last render.
                let inner_w = rect.width.saturating_sub(2);
                let inner_h = rect.height.saturating_sub(2);
                if let Some(pane) = self.panes.get_mut(pane_id)
                    && (pane.cols() != inner_w || pane.rows() != inner_h)
                {
                    let _ = pane.resize(inner_w, inner_h);
                }
            }
            // Collect exit codes while we still have mutable access.
            let exit_codes: HashMap<PaneId, Option<i32>> = rects
                .iter()
                .filter_map(|(pid, _)| {
                    self.panes.get_mut(pid).map(|p| (*pid, p.exit_status()))
                })
                .collect();
            for (pane_id, rect) in rects {
                if let Some(pane) = self.panes.get(&pane_id) {
                    let screen = pane.screen();
                    let is_focused = self.focused_pane == Some(pane_id)
                        && self.focus == FocusedPanel::Terminal;
                    let preset_name = self
                        .pane_presets
                        .get(&pane_id)
                        .map(|s| s.as_str())
                        .unwrap_or("shell");
                    let exit_code = exit_codes.get(&pane_id).copied().flatten();
                    let sel = self.selection_for_pane(pane_id);
                    let widget = TerminalWidget::new(
                        &screen,
                        preset_name,
                        &self.palette,
                        &self.ui_config,
                    )
                    .focus(is_focused)
                    .exited(exit_code)
                    .pane_count(pane_count)
                    .search(
                        if is_focused { search_matches } else { &[] },
                        if is_focused { search_active } else { None },
                        search_base_row,
                    )
                    .selection(sel);
                    frame.render_widget(widget, rect);
                    if is_focused && exit_code.is_none() && !screen.hide_cursor() && screen.scrollback() == 0 {
                        let (crow, ccol) = screen.cursor_position();
                        let cx = rect.x + 1 + ccol;
                        let cy = rect.y + 1 + crow;
                        if cx < rect.right() - 1 && cy < rect.bottom() - 1 {
                            frame.set_cursor_position(Position::new(cx, cy));
                        }
                    }
                }
            }
        }
    }

    fn handle_action(&mut self, action: Action) {
        match action {
            Action::EnterMode(mode) => {
                // Clear search state when switching away from search modes.
                if matches!(self.mode, Mode::EnterSearch | Mode::Search)
                    && !matches!(mode, Mode::EnterSearch | Mode::Search)
                {
                    self.search_state = None;
                }
                // Initialize search state when entering EnterSearch.
                if mode == Mode::EnterSearch && self.search_state.is_none() {
                    self.search_state = Some(SearchState::new());
                }
                self.mode = mode;
                match mode {
                    Mode::Workspace => self.focus = FocusedPanel::Workspace,
                    Mode::Room => self.focus = FocusedPanel::Room,
                    Mode::Explorer => {
                        self.focus = FocusedPanel::Explorer;
                        if let Some(path) = self.current_room_path() {
                            if self.explorer_state.root != path {
                                self.explorer_state = humu::explorer::ExplorerState::new(path);
                            }
                            self.explorer_state.scan();
                        }
                    }
                    _ => self.focus = FocusedPanel::Terminal,
                }
            }
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
            Action::NewPane => {
                self.popup = PopupState::SplitDirection;
            }
            Action::SplitDown => self.show_preset_selector(PresetAction::SplitDown),
            Action::SplitRight => self.show_preset_selector(PresetAction::SplitRight),
            Action::ClosePane => self.close_pane(),
            Action::MoveFocus(dir) => self.move_focus(dir),
            Action::ToggleFullscreen => self.toggle_fullscreen(),

            // Workspace/room actions
            Action::Create => self.show_create_dialog(),
            Action::Delete => self.show_delete_dialog(),

            // Settings
            Action::OpenSettings => {
                self.popup = PopupState::Settings { selected: 0 };
            }

            // Resize actions
            Action::Resize(dir) => self.handle_resize_action(dir),

            // Search actions
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
                if self.search_state.is_some() {
                    self.run_search();
                }
            }
            Action::ScrollDown => {
                if let Some(pane_id) = self.focused_pane {
                    if let Some(pane) = self.panes.get(&pane_id) {
                        let current = pane.scrollback();
                        pane.set_scrollback(current.saturating_sub(1));
                    }
                }
                if self.search_state.is_some() {
                    self.run_search();
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
                if self.search_state.is_some() {
                    self.run_search();
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
                if self.search_state.is_some() {
                    self.run_search();
                }
            }

            Action::DiffFile => {
                self.explorer_diff_file();
            }
            Action::ToggleIgnored => {
                self.explorer_state.show_ignored = !self.explorer_state.show_ignored;
                self.explorer_state.scan();
            }

            Action::CopyPath => {
                if let Some(entry) = self.explorer_state.selected_entry() {
                    let abs_path = entry.path.to_string_lossy();
                    use std::io::Write;
                    let encoded = base64_encode(abs_path.as_bytes());
                    let osc = format!("\x1b]52;c;{}\x07", encoded);
                    let _ = stdout().write_all(osc.as_bytes());
                    let _ = stdout().flush();
                }
            }

            _ => {}
        }
    }

    fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent) {
        // Forward mouse events to floating pane if active and within its area.
        if let PopupState::FloatingPane { pane_id, .. } = &self.popup {
            let pane_id = *pane_id;
            if self.forward_mouse_to_floating_pane(pane_id, &mouse) {
                return;
            }
        }

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.pty_mouse_active = false;
                self.selection = None;
                self.handle_click(mouse.column, mouse.row);
                if self.try_forward_mouse(&mouse) {
                    self.pty_mouse_active = true;
                } else {
                    // Start text selection if click is on a terminal pane.
                    let pos = Position::new(mouse.column, mouse.row);
                    if self.panel_rects.terminal.contains(pos) {
                        if let Some((pane_id, pane_rect)) = self.pane_at(pos) {
                            let col = mouse.column.saturating_sub(pane_rect.x + 1);
                            let row = mouse.row.saturating_sub(pane_rect.y + 1);
                            self.selection = Some(TextSelection {
                                pane_id,
                                pane_rect,
                                start: (row, col),
                                end: (row, col),
                            });
                        }
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.pty_mouse_active {
                    self.try_forward_mouse(&mouse);
                } else if let Some(ref mut sel) = self.selection {
                    let col = mouse.column.saturating_sub(sel.pane_rect.x + 1);
                    let row = mouse.row.saturating_sub(sel.pane_rect.y + 1);
                    sel.end = (row, col);
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if self.pty_mouse_active {
                    self.try_forward_mouse(&mouse);
                    self.pty_mouse_active = false;
                } else if let Some(ref sel) = self.selection {
                    // Copy selected text to clipboard via OSC 52.
                    if sel.start != sel.end {
                        self.copy_selection_to_clipboard();
                    }
                    self.selection = None;
                }
            }
            MouseEventKind::Down(_) | MouseEventKind::Up(_) => {
                self.try_forward_mouse(&mouse);
            }
            MouseEventKind::Drag(_) => {
                if self.pty_mouse_active {
                    self.try_forward_mouse(&mouse);
                }
            }
            MouseEventKind::ScrollUp => {
                if !self.try_forward_mouse(&mouse) {
                    self.handle_scroll(mouse.column, mouse.row, true);
                }
            }
            MouseEventKind::ScrollDown => {
                if !self.try_forward_mouse(&mouse) {
                    self.handle_scroll(mouse.column, mouse.row, false);
                }
            }
            _ => {}
        }
    }

    fn selection_for_pane(&self, pane_id: PaneId) -> Option<(u16, u16, u16, u16)> {
        let sel = self.selection.as_ref()?;
        if sel.pane_id != pane_id {
            return None;
        }
        let (sr, sc, er, ec) = if sel.start <= sel.end {
            (sel.start.0, sel.start.1, sel.end.0, sel.end.1)
        } else {
            (sel.end.0, sel.end.1, sel.start.0, sel.start.1)
        };
        Some((sr, sc, er, ec))
    }

    fn copy_selection_to_clipboard(&self) {
        let sel = match &self.selection {
            Some(s) => s,
            None => return,
        };
        let pane = match self.panes.get(&sel.pane_id) {
            Some(p) => p,
            None => return,
        };
        let screen = pane.screen();
        let (start_row, start_col, end_row, end_col) = if sel.start <= sel.end {
            (sel.start.0, sel.start.1, sel.end.0, sel.end.1)
        } else {
            (sel.end.0, sel.end.1, sel.start.0, sel.start.1)
        };

        let mut text = String::new();
        let cols = screen.size().1;
        for row in start_row..=end_row {
            let from = if row == start_row { start_col } else { 0 };
            let to = if row == end_row { end_col } else { cols.saturating_sub(1) };
            for col in from..=to {
                if let Some(cell) = screen.cell(row, col) {
                    // Skip continuation cells of wide characters (e.g., CJK)
                    if cell.is_wide_continuation() {
                        continue;
                    }
                    let contents = cell.contents();
                    if contents.is_empty() {
                        text.push(' ');
                    } else {
                        text.push_str(&contents);
                    }
                }
            }
            if row < end_row {
                // Trim trailing spaces from each line.
                let trimmed = text.trim_end_matches(' ');
                text.truncate(trimmed.len());
                text.push('\n');
            }
        }
        let trimmed = text.trim_end();
        if trimmed.is_empty() {
            return;
        }

        // OSC 52 clipboard: \x1b]52;c;{base64}\x07
        use std::io::Write;
        let encoded = base64_encode(trimmed.as_bytes());
        let osc = format!("\x1b]52;c;{}\x07", encoded);
        let _ = stdout().write_all(osc.as_bytes());
        let _ = stdout().flush();
    }

    /// Handle a left-button mouse click at terminal coordinates (x, y).
    fn handle_click(&mut self, x: u16, y: u16) {
        let pos = Position::new(x, y);
        if self.panel_rects.workspace.contains(pos) {
            let row = y.saturating_sub(self.panel_rects.workspace.y + 1) as usize;
            let items = self.workspace_items();
            if row < items.len() {
                self.workspace_selected = Some(items[row].id);
                self.switch_to_selected_room();
                self.mode = Mode::Terminal;
                self.focus = FocusedPanel::Terminal;
            } else {
                self.mode = Mode::Workspace;
                self.focus = FocusedPanel::Workspace;
            }
        } else if self.panel_rects.room.contains(pos) {
            let row = y.saturating_sub(self.panel_rects.room.y + 1) as usize;
            let items = self.room_items();
            if row < items.len() {
                if let Some(id) = items[row].id {
                    self.room_selected = Some(id);
                }
                self.select_current();
                self.mode = Mode::Terminal;
                self.focus = FocusedPanel::Terminal;
            } else {
                self.mode = Mode::Room;
                self.focus = FocusedPanel::Room;
            }
        } else if self.panel_rects.tab_bar.contains(pos) {
            // Determine which tab or "+" was clicked.
            self.handle_tab_bar_click(x);
        } else if self.panel_rects.explorer.contains(pos) {
            let was_focused = self.focus == FocusedPanel::Explorer;
            self.mode = Mode::Explorer;
            self.focus = FocusedPanel::Explorer;
            // Map click y to tree entry (accounting for border + scroll offset)
            let inner_y = y.saturating_sub(self.panel_rects.explorer.y + 1);
            let clicked_index = self.explorer_state.scroll_offset + inner_y as usize;
            if clicked_index < self.explorer_state.entries.len() {
                if was_focused && clicked_index == self.explorer_state.selected {
                    // Focused + same item → open (same as Enter)
                    self.explorer_select();
                } else {
                    // Not focused or different item → just select
                    self.explorer_state.selected = clicked_index;
                }
            }
        } else if self.panel_rects.terminal.contains(pos) {
            self.mode = Mode::Terminal;
            self.focus = FocusedPanel::Terminal;
            if let Some((pane_id, _)) = self.pane_at(pos) {
                self.focused_pane = Some(pane_id);
            }
        } else if self.panel_rects.status_bar.contains(pos) {
            self.handle_status_bar_click(x);
        }
    }

    /// Handle a click on a status bar hint segment.
    fn handle_status_bar_click(&mut self, click_x: u16) {
        // Modes without clickable hints
        if matches!(self.mode, Mode::EnterSearch | Mode::Locked) {
            return;
        }

        let area = self.panel_rects.status_bar;
        let mut x = area.x;

        // Mode badge: " MODE " + separator
        let label = status_bar::mode_label(self.mode);
        x += label.chars().count() as u16 + 2 + 1;

        // Terminal mode has a "Ctrl+" prefix block
        if self.mode == Mode::Terminal {
            x += 1 + 8 + 1; // sep + " Ctrl + " + sep
        }

        // Search mode: skip over query prefix, then test search-specific hints
        if self.mode == Mode::Search {
            if let Some(ref state) = self.search_state {
                // " / {query} " rendered before hints
                x += 3 + state.query.chars().count() as u16 + 1;

                let hints: [(&str, &str); 4] = [
                    ("n", "NEXT"),
                    ("N", "PREV"),
                    ("c", "CASE"), // "CASE"/"case" both 4 chars
                    ("w", "WRAP"), // "WRAP"/"wrap" both 4 chars
                ];
                for (i, (key, lbl)) in hints.iter().enumerate() {
                    let seg = status_bar::hint_segment_width(key, lbl);
                    if click_x >= x && click_x < x + seg {
                        if let Some(action) = hint_click_action(self.mode, i) {
                            self.handle_action(action);
                        }
                        return;
                    }
                    x += seg;
                }
            }
            return;
        }

        // Normal modes: iterate mode_hints
        let hints = status_bar::mode_hints(self.mode);
        for (i, (key, lbl)) in hints.iter().enumerate() {
            let seg = status_bar::hint_segment_width(key, lbl);
            if click_x >= x && click_x < x + seg {
                if let Some(action) = hint_click_action(self.mode, i) {
                    self.handle_action(action);
                }
                return;
            }
            x += seg;
        }

        // Right-aligned hints (e.g. Alt+n in Terminal mode)
        let right_hints = status_bar::mode_hints_right(self.mode);
        if !right_hints.is_empty() {
            let alt_prefix_width: u16 = 1 + 7 + 1; // sep + " Alt + " + sep
            let hints_width: u16 = right_hints.iter().map(|(k, l)| {
                status_bar::hint_segment_width(k, l)
            }).sum();
            let total_right = hints_width + alt_prefix_width;
            let mut rx = area.x + area.width.saturating_sub(total_right);
            for (i, (key, lbl)) in right_hints.iter().enumerate() {
                let seg = status_bar::hint_segment_width(key, lbl);
                if click_x >= rx && click_x < rx + seg {
                    if let Some(action) = hint_click_action_right(self.mode, i) {
                        self.handle_action(action);
                    }
                    return;
                }
                rx += seg;
            }
        }
    }


    /// Returns the pane content area (terminal area minus the tab bar row).
    fn terminal_pane_area(&self) -> Rect {
        let r = self.panel_rects.terminal;
        if r.height > 1 {
            Rect::new(r.x, r.y + 1, r.width, r.height - 1)
        } else {
            Rect::new(r.x, r.y, r.width, 0)
        }
    }

    /// Find the pane whose rendered rect contains the given position.
    fn pane_at(&self, pos: Position) -> Option<(PaneId, Rect)> {
        let pane_area = self.terminal_pane_area();
        self.tabs
            .active_tree()?
            .compute_rects(pane_area)
            .into_iter()
            .find(|(_, rect)| rect.contains(pos))
    }

    /// Try to forward a mouse event to the focused pane's PTY.
    /// Returns true if the event was forwarded (child has mouse reporting enabled).
    fn try_forward_mouse(&mut self, mouse: &crossterm::event::MouseEvent) -> bool {
        let pane_id = match self.focused_pane {
            Some(id) => id,
            None => return false,
        };

        // Find the focused pane's rect for coordinate translation.
        let pane_rect = self
            .pane_at(Position::new(mouse.column, mouse.row))
            .map(|(_, rect)| rect)
            .or_else(|| {
                // Pane not at mouse position (drag outside) — find focused pane's rect.
                let pane_area = self.terminal_pane_area();
                self.tabs
                    .active_tree()
                    .and_then(|t| {
                        t.compute_rects(pane_area)
                            .into_iter()
                            .find(|(id, _)| *id == pane_id)
                            .map(|(_, rect)| rect)
                    })
            })
            .unwrap_or_else(|| self.terminal_pane_area());

        let pane = match self.panes.get_mut(&pane_id) {
            Some(p) => p,
            None => return false,
        };

        if pane.mouse_protocol_mode() == vt100::MouseProtocolMode::None {
            return false;
        }

        let encoding = pane.mouse_protocol_encoding();
        let col = mouse.column.saturating_sub(pane_rect.x + 1) as u32;
        let row = mouse.row.saturating_sub(pane_rect.y + 1) as u32;

        let (button, press) = match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => (0u32, true),
            MouseEventKind::Down(MouseButton::Right) => (2, true),
            MouseEventKind::Down(MouseButton::Middle) => (1, true),
            MouseEventKind::Up(MouseButton::Left) => (0, false),
            MouseEventKind::Up(MouseButton::Right) => (2, false),
            MouseEventKind::Up(MouseButton::Middle) => (1, false),
            MouseEventKind::Drag(MouseButton::Left) => (32, true),
            MouseEventKind::Drag(MouseButton::Right) => (34, true),
            MouseEventKind::Drag(MouseButton::Middle) => (33, true),
            MouseEventKind::ScrollUp => (64, true),
            MouseEventKind::ScrollDown => (65, true),
            MouseEventKind::Moved => (35, true),
            _ => return false,
        };

        let seq = match encoding {
            vt100::MouseProtocolEncoding::Sgr => {
                let suffix = if press { 'M' } else { 'm' };
                format!("\x1b[<{};{};{}{}", button, col + 1, row + 1, suffix)
            }
            _ => {
                let b = (button + 32) as u8;
                let c = ((col + 33).min(255)) as u8;
                let r = ((row + 33).min(255)) as u8;
                format!("\x1b[M{}{}{}", b as char, c as char, r as char)
            }
        };
        let _ = pane.write_input(seq.as_bytes());
        true
    }

    /// Handle mouse scroll within the terminal area.
    ///
    /// If the child process has enabled mouse reporting, forward the scroll
    /// as a proper mouse escape sequence. Otherwise, send arrow key sequences
    /// (3 lines per scroll tick) for programs like plain shells.
    fn handle_scroll(&mut self, x: u16, y: u16, up: bool) {
        let pos = Position::new(x, y);
        if !self.panel_rects.terminal.contains(pos) {
            return;
        }

        let (pane_id, pane_rect) = match self.pane_at(pos) {
            Some(v) => v,
            None => return,
        };

        let pane = match self.panes.get_mut(&pane_id) {
            Some(p) => p,
            None => return,
        };

        // Read mouse protocol state via thin accessors (avoids cloning full Screen).
        let mouse_mode = pane.mouse_protocol_mode();

        if mouse_mode != vt100::MouseProtocolMode::None {
            // Child process wants mouse events — send proper mouse escape sequences.
            let encoding = pane.mouse_protocol_encoding();
            // Translate terminal-absolute coordinates to pane-relative.
            let col = x.saturating_sub(pane_rect.x + 1) as u32; // inside border
            let row = y.saturating_sub(pane_rect.y + 1) as u32;
            let button: u32 = if up { 64 } else { 65 }; // 64 = scroll up, 65 = scroll down

            let seq = match encoding {
                vt100::MouseProtocolEncoding::Sgr => {
                    format!("\x1b[<{};{};{}M", button, col + 1, row + 1)
                }
                _ => {
                    // Default/UTF-8 encoding: \x1b[M + (button+32) + (col+33) + (row+33)
                    let b = (button + 32) as u8;
                    let c = ((col + 33).min(255)) as u8;
                    let r = ((row + 33).min(255)) as u8;
                    format!("\x1b[M{}{}{}", b as char, c as char, r as char)
                }
            };
            let _ = pane.write_input(seq.as_bytes());
        } else {
            // No mouse reporting — adjust scrollback offset.
            let lines_per_tick: usize = 3;
            let current = pane.scrollback();
            if up {
                pane.set_scrollback(current.saturating_add(lines_per_tick));
            } else {
                pane.set_scrollback(current.saturating_sub(lines_per_tick));
            }
        }
        // Re-run search so highlights track the new viewport.
        if self.search_state.is_some() {
            self.run_search();
        }
    }

    /// Handle keyboard resize actions.
    fn handle_resize_action(&mut self, dir: NavDirection) {
        match self.focus {
            FocusedPanel::Terminal => {
                if let Some(pane_id) = self.focused_pane
                    && let Some(tree) = self.tabs.active_tree_mut()
                {
                    let sign: f64 = match dir {
                        NavDirection::Left | NavDirection::Up => -0.05,
                        NavDirection::Right | NavDirection::Down => 0.05,
                    };
                    tree.resize(pane_id, sign);
                }
            }
            FocusedPanel::Workspace => {
                let delta: i16 = match dir {
                    NavDirection::Right => 1,
                    NavDirection::Left => -1,
                    _ => 0,
                };
                self.panel_widths[0] =
                    (self.panel_widths[0] as i16 + delta).clamp(5, 60) as u16;
            }
            FocusedPanel::Room => {
                let delta: i16 = match dir {
                    NavDirection::Right => 1,
                    NavDirection::Left => -1,
                    _ => 0,
                };
                self.panel_widths[1] =
                    (self.panel_widths[1] as i16 + delta).clamp(5, 60) as u16;
            }
            FocusedPanel::Explorer => {
                let delta: i16 = match dir {
                    NavDirection::Right => 1,
                    NavDirection::Left => -1,
                    _ => 0,
                };
                self.panel_widths[2] =
                    (self.panel_widths[2] as i16 + delta).clamp(5, 60) as u16;
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
        if self.state.active_room_id.is_none() {
            self.show_error("Select a room first");
            return;
        }
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
            FocusedPanel::Explorer => {}
        }
    }

    /// Show the appropriate delete dialog based on focused panel.
    fn show_delete_dialog(&mut self) {
        match self.focus {
            FocusedPanel::Workspace => {
                let ws_name = match self.workspace_selected
                    .and_then(|id| self.state.ws_by_id(id))
                    .map(|w| w.name.clone())
                {
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
                let branch = match self.active_room_name() {
                    Some(r) => r,
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
            FocusedPanel::Explorer => {}
        }
    }

    /// Compute the working directory for the currently active workspace/room.
    ///
    /// Returns `None` when no workspace or room is active.  The default room
    /// (the workspace repo itself) maps to the workspace path; worktree rooms
    /// map to `~/.humu/worktrees/<workspace>/<room>`.
    fn current_room_path(&self) -> Option<PathBuf> {
        let ws_id = self.state.active_workspace_id?;
        let room_id = self.state.active_room_id?;
        let ws = self.state.ws_by_id(ws_id)?;
        let room = ws.room_by_id(room_id)?;

        let worktree_path = humu_dir()
            .join("worktrees")
            .join(&ws.name)
            .join(&room.name);

        if worktree_path.exists() {
            Some(worktree_path)
        } else {
            // Default room: the workspace repo directory itself.
            Some(ws.path.clone())
        }
    }

    /// Spawn a new pane from the named preset and register it.
    /// Returns the new `PaneId` on success.
    fn spawn_pane(&mut self, preset_name: &str, session_id: Option<String>) -> Option<PaneId> {
        // Preserve session_id for agent_states before it's consumed by args.
        let restored_session_id = session_id.clone();

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
        let mut extra_args: Vec<String> = vec![];
        let id = PaneId::new();
        let envs: Vec<(String, String)> = if preset_name == "claude" {
            let settings_path = humu_dir().join("hooks/claude-settings.json");
            extra_args.push("--settings".to_string());
            extra_args.push(settings_path.to_string_lossy().into_owned());

            if let Some(sid) = session_id {
                extra_args.push("--resume".to_string());
                extra_args.push(sid);
            }

            let mut envs = vec![];
            if let Some(port) = self.hook_port {
                envs.push(("HUMU_PORT".to_string(), port.to_string()));
            }
            if let Some(ws_id) = self.state.active_workspace_id {
                envs.push(("HUMU_WORKSPACE_ID".to_string(), ws_id.to_string()));
            }
            if let Some(room_id) = self.state.active_room_id {
                envs.push(("HUMU_ROOM_ID".to_string(), room_id.to_string()));
            }
            envs.push(("HUMU_TAB_ID".to_string(), TabId::new().to_string()));
            envs.push(("HUMU_PANE_ID".to_string(), id.to_string()));
            envs
        } else {
            vec![]
        };

        let cwd = self.current_room_path();
        let mut all_args = args;
        all_args.extend(extra_args);
        let pane = PtyPane::spawn_with_envs(&cmd, &all_args, cwd.as_deref(), 80, 24, &envs).ok()?;
        self.panes.insert(id, pane);
        self.pane_presets.insert(id, preset_name.to_string());
        // Seed agent_states so session_id survives restart even if no hook
        // event arrives before the next shutdown.
        if restored_session_id.is_some() {
            self.agent_states.insert(
                id,
                AgentStateEntry {
                    state: AgentState::Idle,
                    session_id: restored_session_id,

                },
            );
        }
        Some(id)
    }

    fn new_tab_with_preset(&mut self, preset_name: &str) {
        if let Some(new_id) = self.spawn_pane(preset_name, None) {
            self.tabs.add_tab(preset_name.to_string(), SplitTree::leaf(new_id));
            let last = self.tabs.len() - 1;
            self.tabs.set_active(last);
            self.focused_pane = Some(new_id);
            self.persist_layout();
        }
    }

    fn cleanup_exited_panes(&mut self) {
        // Don't clean up floating pane — it has its own auto-close logic.
        let floating_id = if let PopupState::FloatingPane { pane_id, .. } = &self.popup {
            Some(*pane_id)
        } else {
            None
        };
        let exited: Vec<PaneId> = self
            .panes
            .iter_mut()
            .filter_map(|(id, p)| p.exit_status().map(|_| *id))
            .filter(|id| Some(*id) != floating_id)
            .collect();
        if !exited.is_empty() {
            self.remove_panes(&exited);
        }
    }

    fn close_tab(&mut self) {
        let ids: Vec<PaneId> = self
            .tabs
            .active_tree()
            .map(|t| t.pane_ids())
            .unwrap_or_default();
        if !ids.is_empty() {
            self.remove_panes(&ids);
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
        let new_id = match self.spawn_pane(preset_name, None) {
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
            self.persist_layout();
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
        if let Some(id) = self.focused_pane {
            self.remove_panes(&[id]);
        }
    }

    /// Remove panes by ID: clean up state, update split trees, remove empty tabs.
    fn remove_panes(&mut self, ids: &[PaneId]) {
        for id in ids {
            self.panes.remove(id);
            self.pane_presets.remove(id);
            self.agent_states.remove(id);
        }
        // Remove dead panes from trees and remove empty tabs.
        let mut i = self.tabs.len();
        while i > 0 {
            i -= 1;
            if let Some(tree) = self.tabs.tree_at_mut(i) {
                for id in ids {
                    tree.remove_pane(*id);
                }
                let alive = tree.pane_ids().iter().any(|id| self.panes.contains_key(id));
                if !alive {
                    self.tabs.remove_tab(i);
                }
            }
        }
        self.fullscreen_pane = None;
        self.sync_focused_pane();
        self.persist_layout();
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
        if self.focus != FocusedPanel::Terminal {
            return;
        }
        let Some(pane_id) = self.focused_pane else {
            return;
        };

        // If focused pane has exited, intercept p/t to close pane/tab.
        let exited = self
            .panes
            .get_mut(&pane_id)
            .and_then(|p| p.exit_status())
            .is_some();
        if exited {
            return;
        }

        if let Some(pane) = self.panes.get_mut(&pane_id) {
            // Page Up/Down: scroll scrollback buffer when no mouse reporting,
            // otherwise forward to PTY.
            if matches!(key.code, KeyCode::PageUp | KeyCode::PageDown)
                && pane.mouse_protocol_mode() == vt100::MouseProtocolMode::None
            {
                let page = pane.rows() as usize;
                let current = pane.scrollback();
                if key.code == KeyCode::PageUp {
                    pane.set_scrollback(current.saturating_add(page));
                } else {
                    pane.set_scrollback(current.saturating_sub(page));
                }
                return;
            }

            // Reset scrollback to live view when the user types.
            if pane.scrollback() > 0 {
                pane.set_scrollback(0);
            }
            let bytes = key_event_to_bytes(&key);
            if !bytes.is_empty() {
                let _ = pane.write_input(&bytes);
            }
        }
    }

    /// Route paste events: popups get priority, otherwise forward to PTY.
    fn handle_paste_event(&mut self, text: &str) {
        if let PopupState::NotificationTokenInput { field, value } = &self.popup {
            let field = *field;
            let mut value = value.clone();
            value.push_str(text);
            self.popup = PopupState::NotificationTokenInput { field, value };
            return;
        }
        if let PopupState::FloatingPane { pane_id, .. } = &self.popup {
            let pane_id = *pane_id;
            if let Some(pane) = self.panes.get_mut(&pane_id) {
                let _ = pane.write_input(text.as_bytes());
            }
            return;
        }
        self.handle_paste(text);
    }

    /// Forward pasted text to the focused PTY pane.
    /// Wraps in bracketed paste sequences if the child process has requested it.
    /// In EnterSearch mode, appends the text to the search query instead.
    fn handle_paste(&mut self, text: &str) {
        if self.mode == Mode::EnterSearch {
            if let Some(ref mut state) = self.search_state {
                state.query.push_str(text);
                self.run_search();
            }
            return;
        }
        if self.focus != FocusedPanel::Terminal {
            return;
        }
        let Some(pane_id) = self.focused_pane else {
            return;
        };
        let Some(pane) = self.panes.get_mut(&pane_id) else {
            return;
        };
        if pane.exit_status().is_some() {
            return;
        }
        if pane.scrollback() > 0 {
            pane.set_scrollback(0);
        }
        if pane.bracketed_paste() {
            let mut buf = Vec::with_capacity(12 + text.len());
            buf.extend_from_slice(b"\x1b[200~");
            buf.extend_from_slice(text.as_bytes());
            buf.extend_from_slice(b"\x1b[201~");
            let _ = pane.write_input(&buf);
        } else {
            let _ = pane.write_input(text.as_bytes());
        }
    }

    fn navigate(&mut self, delta: i32) {
        match self.focus {
            FocusedPanel::Workspace => {
                let items = self.workspace_items();
                if items.is_empty() { return; }
                let current = self.workspace_selected
                    .and_then(|id| items.iter().position(|w| w.id == id))
                    .unwrap_or(0) as i32;
                let next = (current + delta).clamp(0, items.len() as i32 - 1) as usize;
                self.workspace_selected = Some(items[next].id);
            }
            FocusedPanel::Room => {
                let items = self.room_items();
                if items.is_empty() { return; }
                let current = self.room_selected
                    .and_then(|id| items.iter().position(|r| r.id == Some(id)))
                    .unwrap_or(0) as i32;
                let next = (current + delta).clamp(0, items.len() as i32 - 1) as usize;
                if let Some(id) = items[next].id {
                    self.room_selected = Some(id);
                }
            }
            FocusedPanel::Terminal => {}
            FocusedPanel::Explorer => {
                if delta < 0 {
                    self.explorer_state.move_up();
                } else {
                    self.explorer_state.move_down();
                }
            }
        }
    }

    fn select_current(&mut self) {
        match self.focus {
            FocusedPanel::Workspace => {
                self.switch_to_selected_room();
                self.mode = Mode::Terminal;
                self.focus = FocusedPanel::Terminal;
            }
            FocusedPanel::Room => {
                self.switch_to_selected_room();
                self.mode = Mode::Terminal;
                self.focus = FocusedPanel::Terminal;
            }
            FocusedPanel::Terminal => {}
            FocusedPanel::Explorer => {
                self.explorer_select();
            }
        }
    }

    fn restore_selection(&mut self) {
        // Prune stale room entries for every workspace before restoring selection.
        // Discover actual rooms from git and remove any persisted entries that no
        // longer correspond to a live worktree.
        let ws_info: Vec<(String, PathBuf)> = self
            .state
            .workspaces
            .iter()
            .map(|w| (w.name.clone(), w.path.clone()))
            .collect();
        for (ws_name, ws_path) in ws_info {
            let mgr = RoomManager::new();
            if let Ok(rooms) = mgr.list(&ws_path) {
                let discovered: std::collections::HashSet<String> =
                    rooms.into_iter().map(|r| r.branch).collect();
                humu::config::prune_stale_rooms_for_workspace(
                    &mut self.state,
                    &ws_name,
                    &discovered,
                );
            }
        }

        self.workspace_selected = self.state.active_workspace_id;
        self.room_selected = self.state.active_room_id;

        // Restore layout if saved
        if let (Some(ws_id), Some(room_id)) = (self.state.active_workspace_id, self.state.active_room_id) {
            if let Some(ws) = self.state.ws_by_id(ws_id) {
                if let Some(room) = ws.room_by_id(room_id) {
                    if !room.tabs.is_empty() {
                        let active_tab = room.active_tab.unwrap_or(0);
                        let tabs = room.tabs.clone();
                        self.restore_layout(active_tab, tabs);
                    }
                }
            }
        }
    }

    /// Drain the hook event channel, update agent_states, and fire
    /// notifications on Working→NeedsInput / Working→Idle transitions.
    fn process_hook_events(&mut self) {
        let events: Vec<HookEvent> = self
            .hook_rx
            .as_ref()
            .map(|rx| std::iter::from_fn(|| rx.try_recv().ok()).collect())
            .unwrap_or_default();

        for event in events {
            humu::humu_log!(
                "hook: ws={:?} room={:?} tab={:?} pane={} state={:?} session={:?}",
                event.workspace_id,
                event.room_id,
                event.tab_id,
                event.pane_id,
                event.event_type,
                event.session_id,
            );

            let pane_id = event.pane_id;
            let prev_state = self.agent_states.get(&pane_id).map(|e| e.state.clone());

            let new_session_id = event.session_id.clone();
            let existing_session_id = self
                .agent_states
                .get(&pane_id)
                .and_then(|e| e.session_id.clone());
            let session_id = new_session_id.or(existing_session_id);

            // Detect Working → NeedsInput or Working → Idle transitions.
            let should_notify = matches!(
                (&prev_state, &event.event_type),
                (Some(AgentState::Working), AgentState::NeedsInput)
                    | (Some(AgentState::Working), AgentState::Idle)
            );

            self.agent_states.insert(
                pane_id,
                AgentStateEntry {
                    state: event.event_type.clone(),
                    session_id,
                },
            );

            if should_notify {
                let ws_name = event
                    .workspace_id
                    .as_deref()
                    .and_then(|s| uuid::Uuid::parse_str(s).ok())
                    .map(WorkspaceId)
                    .and_then(|id| self.state.ws_by_id(id))
                    .map(|ws| ws.name.clone())
                    .unwrap_or_else(|| "unknown".to_string());

                let room_name = event
                    .workspace_id
                    .as_deref()
                    .and_then(|s| uuid::Uuid::parse_str(s).ok())
                    .map(WorkspaceId)
                    .and_then(|ws_id| {
                        let ws = self.state.ws_by_id(ws_id)?;
                        let room_uuid = event.room_id.as_deref()
                            .and_then(|s| uuid::Uuid::parse_str(s).ok())?;
                        ws.room_by_id(RoomId(room_uuid)).map(|r| r.name.clone())
                    })
                    .unwrap_or_else(|| "unknown".to_string());

                let notification_event = match event.event_type {
                    AgentState::NeedsInput => {
                        humu::notification::NotificationEvent::AgentNeedsInput {
                            workspace: ws_name,
                            room: room_name,
                        }
                    }
                    AgentState::Idle => {
                        humu::notification::NotificationEvent::AgentFinished {
                            workspace: ws_name,
                            room: room_name,
                        }
                    }
                    _ => continue,
                };
                self.notification_manager.notify(notification_event, self.is_focused);
            }
        }
    }

    /// Collect pane IDs belonging to a workspace (live + suspended).
    fn pane_ids_for_workspace(&self, ws_id: WorkspaceId) -> Vec<PaneId> {
        let mut ids = Vec::new();
        // Current room's panes if this is the active workspace.
        if self.state.active_workspace_id == Some(ws_id) {
            ids.extend(self.panes.keys());
        }
        // Suspended rooms for this workspace.
        for ((wid, _), room_state) in &self.suspended_rooms {
            if *wid == ws_id {
                ids.extend(room_state.panes.keys());
            }
        }
        ids
    }

    /// Check if any pane in the given set has an active agent state.
    fn has_active_agent(&self, pane_ids: &[PaneId]) -> bool {
        pane_ids.iter().any(|id| {
            self.agent_states
                .get(id)
                .is_some_and(|e| matches!(e.state, AgentState::Working | AgentState::NeedsInput))
        })
    }

    fn workspace_items(&self) -> Vec<WorkspaceItem> {
        let mut items: Vec<_> = self
            .state
            .workspaces
            .iter()
            .map(|ws| {
                let active = self.has_active_agent(&self.pane_ids_for_workspace(ws.id));
                WorkspaceItem { id: ws.id, name: ws.name.clone(), active }
            })
            .collect();
        items.sort_by(|a, b| a.name.cmp(&b.name));
        items
    }

    fn room_items(&self) -> Vec<RoomItem> {
        let ws_id = match self.state.active_workspace_id {
            Some(id) => id,
            None => return vec![],
        };
        self.room_items_for_workspace(ws_id)
    }

    /// Collect pane IDs belonging to a specific room (live + suspended).
    fn pane_ids_for_room(&self, ws_id: WorkspaceId, room_id: RoomId) -> Vec<PaneId> {
        let mut ids = Vec::new();
        // Current room's panes if this is the active workspace+room.
        if self.state.active_workspace_id == Some(ws_id)
            && self.state.active_room_id == Some(room_id)
        {
            ids.extend(self.panes.keys());
        }
        // Suspended room.
        if let Some(room_state) = self.suspended_rooms.get(&(ws_id, room_id)) {
            ids.extend(room_state.panes.keys());
        }
        ids
    }

    /// Refresh cached room list + git stats for the active workspace.
    fn refresh_room_cache(&mut self) {
        let ws_id = match self.state.active_workspace_id {
            Some(id) => id,
            None => return,
        };
        let ws = match self.state.ws_by_id(ws_id) {
            Some(ws) => ws,
            None => return,
        };
        let mgr = RoomManager::new();
        if let Ok(rooms) = mgr.list(&ws.path) {
            self.room_cache = rooms
                .into_iter()
                .map(|r| {
                    let diff = mgr.diff_stat(&r.path);
                    let ab = mgr.ahead_behind(&r.path);
                    CachedRoomInfo {
                        branch: r.branch,
                        path: r.path,
                        is_default: r.is_default,
                        diff_stat: diff,
                        ahead_behind: ab,
                    }
                })
                .collect();
        }
    }

    /// List rooms for a specific workspace by ID, with agent activity flags.
    /// Uses the cached room list — no git subprocesses.
    fn room_items_for_workspace(&self, ws_id: WorkspaceId) -> Vec<RoomItem> {
        let ws = match self.state.ws_by_id(ws_id) {
            Some(ws) => ws,
            None => return vec![],
        };
        self.room_cache
            .iter()
            .map(|r| {
                let room_id = ws.room_by_name(&r.branch).map(|e| e.id);
                let active = room_id
                    .map(|rid| self.has_active_agent(&self.pane_ids_for_room(ws_id, rid)))
                    .unwrap_or(false);
                RoomItem {
                    id: room_id,
                    name: r.branch.clone(),
                    is_default: r.is_default,
                    active,
                    diff_stat: r.diff_stat,
                    ahead_behind: r.ahead_behind,
                }
            })
            .collect()
    }

    /// Convert the current runtime TabContainer into layout data for persistence.
    /// Returns `None` if there are no tabs.
    fn save_layout(&self) -> Option<(usize, Vec<TabLayout>)> {
        if self.tabs.is_empty() {
            return None;
        }
        let tabs: Vec<TabLayout> = self
            .tabs
            .tab_names()
            .into_iter()
            .enumerate()
            .filter_map(|(i, name)| {
                let tree = self.tabs.tree_at(i)?;
                let split = self.split_tree_to_node(tree)?;
                Some(TabLayout {
                    name: name.to_string(),
                    split,
                })
            })
            .collect();

        if tabs.is_empty() {
            return None;
        }

        Some((self.tabs.active_index(), tabs))
    }

    /// Recursively convert a runtime `SplitTree` to the serializable `SplitNode`.
    fn split_tree_to_node(&self, tree: &SplitTree) -> Option<SplitNode> {
        match tree {
            SplitTree::Leaf(pane_id) => {
                let preset = self.pane_presets.get(pane_id)?.clone();
                let session_id = self.agent_states.get(pane_id).and_then(|e| e.session_id.clone());
                Some(SplitNode::Leaf { preset, session_id })
            }
            SplitTree::Split {
                direction,
                ratio,
                children,
            } => {
                let left = self.split_tree_to_node(&children.0)?;
                let right = self.split_tree_to_node(&children.1)?;
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

    /// Sync the current layout for the active workspace/room into `self.state`
    /// (memory only, no disk write).
    fn sync_layout(&mut self) {
        let ws_id = match self.state.active_workspace_id {
            Some(id) => id,
            None => return,
        };
        let room_id = match self.state.active_room_id {
            Some(id) => id,
            None => return,
        };
        let layout = self.save_layout();
        if let Some(ws_entry) = self.state.ws_by_id_mut(ws_id) {
            if let Some(room_entry) = ws_entry.room_by_id_mut(room_id) {
                if let Some((active_tab, tabs)) = layout {
                    room_entry.active_tab = Some(active_tab);
                    room_entry.tabs = tabs;
                } else {
                    room_entry.active_tab = None;
                    room_entry.tabs.clear();
                }
            }
        }
    }

    /// Sync layout to state and flush to disk.
    fn persist_layout(&mut self) {
        self.sync_layout();
        self.save_state();
    }

    /// Flush `self.state` to disk (`~/.humu/state.yaml`).
    /// Also syncs `panel_widths` so it's never stale on disk.
    fn save_state(&mut self) {
        self.state.panel_widths = Some(self.panel_widths);
        if let Err(e) = self.state.save(&self.state_path) {
            humu::humu_log!("failed to save state: {e}");
        }
    }

    /// Close all existing panes and rebuild the TabContainer from a saved room's layout.
    fn restore_layout(&mut self, active_tab: usize, tabs: Vec<TabLayout>) {
        // Drop all existing panes.
        self.panes.clear();
        self.pane_presets.clear();
        self.tabs = TabContainer::new();
        self.focused_pane = None;
        for tab_layout in &tabs {
            match self.node_to_split_tree(&tab_layout.split) {
                Some(tree) => {
                    self.tabs.add_tab(tab_layout.name.clone(), tree);
                }
                None => {
                    // Fallback: spawn a plain shell tab if restore fails for this tab.
                    if let Some(id) = self.spawn_pane("shell", None) {
                        self.tabs
                            .add_tab(tab_layout.name.clone(), SplitTree::leaf(id));
                    }
                }
            }
        }

        // If nothing was restored, create a default shell tab.
        if self.tabs.is_empty()
            && let Some(id) = self.spawn_pane("shell", None)
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
    fn node_to_split_tree(&mut self, node: &SplitNode) -> Option<SplitTree> {
        match node {
            SplitNode::Leaf { preset, session_id } => {
                let id = self.spawn_pane(preset, session_id.clone())?;
                Some(SplitTree::Leaf(id))
            }
            SplitNode::Split {
                direction,
                ratio,
                children,
            } => {
                if children.len() < 2 {
                    return None;
                }
                let left = self.node_to_split_tree(&children[0])?;
                let right = self.node_to_split_tree(&children[1])?;
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

    // ── Search helpers ─────────────────────────────────────────────────────────

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
            self.scroll_to_active_match();
        }
    }

    fn scroll_to_active_match(&mut self) {
        // Search covers the current viewport, so matches are already visible.
        // No scrollback adjustment needed.
    }

    /// Suspend the current room's live state (panes, tabs, etc.) into the
    /// `suspended_rooms` map without killing PTY processes.
    fn suspend_current_room(&mut self) {
        let ws_id = match self.state.active_workspace_id {
            Some(id) => id,
            None => return,
        };
        let room_id = match self.state.active_room_id {
            Some(id) => id,
            None => return,
        };

        // Clear search state — suspended panes may receive new output.
        self.search_state = None;

        // Sync layout into state so the room can be cold-restored.
        self.sync_layout();

        // Move live state out of self into suspended storage.
        // PaneId uses UUID, so uniqueness is guaranteed across rooms.
        let room_state = RoomState {
            panes: std::mem::take(&mut self.panes),
            tabs: std::mem::replace(&mut self.tabs, TabContainer::new()),
            pane_presets: std::mem::take(&mut self.pane_presets),
            focused_pane: self.focused_pane.take(),
            fullscreen_pane: self.fullscreen_pane.take(),
        };

        self.suspended_rooms.insert((ws_id, room_id), room_state);
    }

    /// Restore a suspended room's live state, or fall back to cold restore from
    /// the persisted layout, or create a default shell tab.
    fn restore_room(&mut self, ws_id: WorkspaceId, room_id: RoomId) {
        if let Some(room_state) = self.suspended_rooms.remove(&(ws_id, room_id)) {
            // Hot restore: swap live PTY panes back in.
            self.panes = room_state.panes;
            self.tabs = room_state.tabs;
            self.pane_presets = room_state.pane_presets;
            self.focused_pane = room_state.focused_pane;
            self.fullscreen_pane = room_state.fullscreen_pane;

            // Drain any accumulated output while suspended.
            for pane in self.panes.values_mut() {
                let _ = pane.process_output();
            }
        } else {
            // Cold restore from persisted layout, or create default.
            let room_layout = self
                .state
                .ws_by_id(ws_id)
                .and_then(|ws| ws.room_by_id(room_id))
                .filter(|r| !r.tabs.is_empty())
                .map(|r| (r.active_tab.unwrap_or(0), r.tabs.clone()));

            if let Some((active_tab, tabs)) = room_layout {
                self.restore_layout(active_tab, tabs);
            } else {
                // No saved layout — create a default shell tab.
                self.panes.clear();
                self.pane_presets.clear();
                self.tabs = TabContainer::new();
                self.focused_pane = None;
                self.fullscreen_pane = None;
                if let Some(id) = self.spawn_pane("shell", None) {
                    self.tabs.add_tab("shell".into(), SplitTree::leaf(id));
                    self.focused_pane = Some(id);
                }
            }
        }
    }

    /// Compute the floating pane overlay rect (centered on terminal panel, 90% size).
    fn floating_pane_area(&self) -> Rect {
        let base = self.panel_rects.terminal;
        let width = (base.width * 9 / 10).max(20);
        let height = (base.height * 9 / 10).max(10);
        let x = base.x + (base.width.saturating_sub(width)) / 2;
        let y = base.y + (base.height.saturating_sub(height)) / 2;
        Rect::new(x, y, width, height)
    }

    /// Spawn an arbitrary command in a new PTY pane without going through presets.
    fn spawn_command(&mut self, cmd: &str, args: &[String], cwd: &std::path::Path, preset_name: &str, cols: u16, rows: u16) -> Option<PaneId> {
        let id = PaneId::new();
        let pane = PtyPane::spawn_with_envs(cmd, args, Some(cwd), cols, rows, &[]).ok()?;
        self.panes.insert(id, pane);
        self.pane_presets.insert(id, preset_name.to_string());
        Some(id)
    }

    /// Open the selected explorer entry: toggle directories, open files in $EDITOR.
    fn explorer_select(&mut self) {
        let entry = match self.explorer_state.selected_entry() {
            Some(e) => e.clone(),
            None => return,
        };
        if entry.kind == humu::explorer::FileKind::Directory {
            self.explorer_state.toggle_dir();
            return;
        }
        // Open file in $EDITOR as floating pane
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
        let cwd = self.explorer_state.root.clone();
        let filepath = entry.path.clone();
        let args = vec![filepath.to_string_lossy().into_owned()];
        let title = entry.name.clone();
        let fp = self.floating_pane_area();
        let (cols, rows) = (fp.width.saturating_sub(2), fp.height.saturating_sub(2));
        if let Some(id) = self.spawn_command(&editor, &args, &cwd, "_editor", cols, rows) {
            self.popup = PopupState::FloatingPane { pane_id: id, title };
        }
    }

    /// Open a git diff view for the selected modified file using delta.
    fn explorer_diff_file(&mut self) {
        let entry = match self.explorer_state.selected_entry() {
            Some(e)
                if e.kind == humu::explorer::FileKind::File
                    && e.git_status == Some(humu::explorer::GitStatus::Modified) =>
            {
                e.clone()
            }
            _ => return,
        };
        if !self.explorer_state.check_delta() {
            self.show_error("delta not installed — install from https://github.com/dandavison/delta");
            return;
        }
        let cwd = self.explorer_state.root.clone();
        let rel_path = entry.path.strip_prefix(&cwd).unwrap_or(&entry.path);
        let escaped_path = rel_path.display().to_string().replace('\'', "'\\''");
        let diff_cmd = format!("git diff '{}' | delta --side-by-side --paging=always", escaped_path);
        let args = vec!["-c".to_string(), diff_cmd];
        let title = format!("diff: {}", entry.name);
        let fp = self.floating_pane_area();
        let (cols, rows) = (fp.width.saturating_sub(2), fp.height.saturating_sub(2));
        if let Some(id) = self.spawn_command("sh", &args, &cwd, "_diff", cols, rows) {
            self.popup = PopupState::FloatingPane { pane_id: id, title };
        }
    }

    /// Switch to the room identified by the current workspace/room selection,
    /// suspending the current room and restoring the target room.
    fn switch_to_selected_room(&mut self) {
        let target_ws_id = match self.workspace_selected {
            Some(id) => id,
            None => return,
        };

        // If switching to a different workspace, clear room_selected so
        // the last_room_id fallback kicks in.
        if self.state.active_workspace_id != Some(target_ws_id) {
            self.room_selected = None;
        }

        // Resolve room ID:
        // 1. If room_selected is set (same workspace navigation), use it.
        // 2. Otherwise, restore the last-used room for this workspace.
        // 3. Otherwise, use the first discovered room (default/main).
        // 4. Otherwise, ensure the "main" room entry exists.
        let target_room_id = if let Some(rid) = self.room_selected {
            rid
        } else if let Some(last) = self.state.ws_by_id(target_ws_id).and_then(|w| w.last_room_id) {
            self.room_selected = Some(last);
            last
        } else {
            let items = self.room_items_for_workspace(target_ws_id);
            match items.first().and_then(|r| r.id) {
                Some(id) => {
                    self.room_selected = Some(id);
                    id
                }
                None => {
                    let ws_name = match self.state.ws_by_id(target_ws_id) {
                        Some(w) => w.name.clone(),
                        None => return,
                    };
                    match humu::config::ensure_room_id_for_workspace(
                        &mut self.state, &ws_name, "main",
                    ) {
                        Some(id) => {
                            self.room_selected = Some(id);
                            id
                        }
                        None => return,
                    }
                }
            }
        };

        // Save last room on the current workspace before suspending.
        if let (Some(ws_id), Some(room_id)) = (self.state.active_workspace_id, self.state.active_room_id) {
            if let Some(ws) = self.state.ws_by_id_mut(ws_id) {
                ws.last_room_id = Some(room_id);
            }
        }

        // Suspend current room (preserves live PTY panes).
        self.suspend_current_room();

        // Update active IDs directly.
        self.state.active_workspace_id = Some(target_ws_id);
        self.state.active_room_id = Some(target_room_id);

        // Restore target room (hot if suspended, cold otherwise).
        self.restore_room(target_ws_id, target_room_id);

        // Reset explorer to the new room's path.
        if let Some(path) = self.current_room_path() {
            self.explorer_state = humu::explorer::ExplorerState::new(path);
            self.explorer_state.scan();
        }

        // Refresh room git stats immediately so the panel doesn't show stale/empty data.
        self.refresh_room_cache();

        self.save_state();
    }

    // ── ID ↔ Name bridge helpers (Task 3 shims; superseded by Task 5) ─────────

    /// Return the name of the active workspace, looked up by ID.
    fn active_workspace_name(&self) -> Option<String> {
        let id = self.state.active_workspace_id?;
        self.state.ws_by_id(id).map(|w| w.name.clone())
    }

    /// Return the name of the active room, looked up by ID.
    fn active_room_name(&self) -> Option<String> {
        let id = self.state.active_room_id?;
        let ws_id = self.state.active_workspace_id?;
        let ws = self.state.ws_by_id(ws_id)?;
        ws.room_by_id(id).map(|r| r.name.clone())
    }

}

impl Drop for App {
    fn drop(&mut self) {
        let port_path = humu_dir().join("port");
        let _ = std::fs::remove_file(&port_path);
    }
}

/// Returns true if the ranges [a_start, a_end) and [b_start, b_end) overlap.
fn ranges_overlap(a_start: u16, a_end: u16, b_start: u16, b_end: u16) -> bool {
    a_start < b_end && b_start < a_end
}

/// Compute the CSI u modifier parameter: 1 + bitmask(shift=1, alt=2, ctrl=4).
fn csi_u_modifier(modifiers: KeyModifiers) -> u8 {
    1 + if modifiers.contains(KeyModifiers::SHIFT) { 1 } else { 0 }
        + if modifiers.contains(KeyModifiers::ALT) { 2 } else { 0 }
        + if modifiers.contains(KeyModifiers::CONTROL) { 4 } else { 0 }
}

fn key_event_to_bytes(key: &KeyEvent) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let has_modifier = key.modifiers != KeyModifiers::NONE;
    match key.code {
        KeyCode::Char(c) if ctrl => {
            let base = vec![(c as u8) & 0x1f];
            if alt { [b"\x1b".as_slice(), &base].concat() } else { base }
        }
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            let base = s.as_bytes().to_vec();
            if alt { [b"\x1b".as_slice(), &base].concat() } else { base }
        }
        KeyCode::Enter if has_modifier => {
            // CSI u format: \x1b[13;{modifier}u
            format!("\x1b[13;{}u", csi_u_modifier(key.modifiers)).into_bytes()
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab if has_modifier => {
            format!("\x1b[9;{}u", csi_u_modifier(key.modifiers)).into_bytes()
        }
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


fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[(n >> 18 & 63) as usize] as char);
        result.push(CHARS[(n >> 12 & 63) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[(n >> 6 & 63) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(n & 63) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}
