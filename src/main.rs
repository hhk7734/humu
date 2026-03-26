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
            if let Some(session_name) = session.as_deref()
                && session_name != "default"
            {
                bail!(
                    "attach fallback only supports the default session until the real client exists"
                );
            }
            let mut app = app::App::new()?;
            app.run()
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
