mod app;
mod cli;
mod server;

use anyhow::{Result, bail};
use cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse()?;
    match cli.command {
        None => {
            let mut app = app::App::new()?;
            app.run()
        }
        Some(Command::Server { daemon }) => server::daemon::run(daemon),
        Some(Command::Attach { session }) => {
            server::daemon::attach_shell(session.as_deref().unwrap_or("default"))
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
