mod app;
mod cli;
mod server;

use anyhow::{Result, bail};
use app::App;
use cli::{Cli, Command};
use std::io::{IsTerminal, stdin, stdout};

fn main() -> Result<()> {
    let cli = Cli::parse()?;
    match cli.command {
        None => run_attached_session("default"),
        Some(Command::Server { daemon }) => server::daemon::run(daemon),
        Some(Command::Attach { session }) => {
            let session_name = session.as_deref().unwrap_or("default");
            run_attached_session(session_name)
        }
        Some(Command::ListSessions) => server::daemon::list_sessions_shell(),
        Some(Command::Detach { session, force }) => {
            if !force {
                bail!("detach shell is not implemented yet; use --force once the client exists");
            }
            server::daemon::force_detach_shell(session.as_deref().unwrap_or("default"))
        }
    }
}

fn run_attached_session(session_name: &str) -> Result<()> {
    server::daemon::run(true)?;
    if stdin().is_terminal() && stdout().is_terminal() {
        App::new_with_session(session_name)?.run()
    } else {
        humu::client::attach::attach(session_name)
    }
}
