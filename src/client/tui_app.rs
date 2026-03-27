use anyhow::Result;
use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use std::io::{IsTerminal, Write, stdin, stdout};
use std::time::Duration;

use crate::client::attach::AttachedClient;
use crate::id::PaneId;
use crate::shared::protocol::ClientRequest;
use crate::shared::render::AgentStatus;

pub struct TuiApp {
    client: AttachedClient,
}

impl TuiApp {
    pub fn new(client: AttachedClient) -> Self {
        Self { client }
    }

    pub fn run(self) -> Result<()> {
        if !stdin().is_terminal() || !stdout().is_terminal() {
            let _ = self.client.state();
            return Ok(());
        }

        enable_raw_mode()?;
        stdout().execute(EnterAlternateScreen)?;
        let result = self.run_attached_loop();
        let _ = stdout().execute(LeaveAlternateScreen);
        let _ = disable_raw_mode();
        result
    }

    fn run_attached_loop(mut self) -> Result<()> {
        render_snapshot_summary(&self.client)?;

        loop {
            if let Ok(event) = self.client.read_event_timeout(Duration::from_millis(10)) {
                if event.is_some() {
                    render_snapshot_summary(&self.client)?;
                }
            }
            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(key)
                        if key.code == KeyCode::Char('q')
                            && key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        break;
                    }
                    Event::Key(key) if key.code == KeyCode::Char('n') => {
                        self.client.send_request(&ClientRequest::RegisterPane {
                            pane_id: PaneId::new(),
                            preset_name: "shell".to_string(),
                            cwd: None,
                            session_id: None,
                            started_at_unix_secs: 0,
                        })?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

fn render_snapshot_summary(client: &AttachedClient) -> Result<()> {
    let snapshot = client.state().snapshot();
    let mut output = String::new();
    output.push_str("\x1b[2J\x1b[H");
    for pane in snapshot.panes.values() {
        let indicator = pane
            .agent_state
            .as_ref()
            .map(|agent_state| match agent_state.status {
                AgentStatus::Working => " ⠋",
                AgentStatus::NeedsInput => " !",
                AgentStatus::Idle => "",
            })
            .unwrap_or("");
        output.push_str(&pane.preset_name);
        output.push_str(indicator);
        output.push('\n');
    }

    let mut stdout = stdout();
    stdout.write_all(output.as_bytes())?;
    stdout.flush()?;
    Ok(())
}
