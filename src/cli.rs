use anyhow::{Result, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cli {
    pub command: Option<Command>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Server { daemon: bool },
    Attach { session: Option<String> },
    ListSessions,
    Detach { session: Option<String>, force: bool },
}

impl Cli {
    pub fn parse() -> Result<Self> {
        Self::parse_from(std::env::args().skip(1))
    }

    pub fn parse_from<I, S>(args: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
        let Some(command) = args.first().map(String::as_str) else {
            return Ok(Self { command: None });
        };

        match command {
            "server" => Ok(Self {
                command: Some(Command::Server {
                    daemon: parse_server_args(&args[1..])?,
                }),
            }),
            "attach" => Ok(Self {
                command: Some(Command::Attach {
                    session: parse_optional_session(&args[1..])?,
                }),
            }),
            "list-sessions" => {
                if args.len() > 1 {
                    bail!("list-sessions does not accept positional arguments");
                }
                Ok(Self {
                    command: Some(Command::ListSessions),
                })
            }
            "detach" => Ok(Self {
                command: Some({
                    let (session, force) = parse_detach_session(&args[1..])?;
                    Command::Detach { session, force }
                }),
            }),
            other => bail!("unknown command: {other}"),
        }
    }
}

fn parse_server_args(args: &[String]) -> Result<bool> {
    let mut daemon = false;
    for arg in args {
        match arg.as_str() {
            "--daemon" => daemon = true,
            other => bail!("unknown server argument: {other}"),
        }
    }
    Ok(daemon)
}

fn parse_optional_session(args: &[String]) -> Result<Option<String>> {
    match args {
        [] => Ok(None),
        [session] if !session.starts_with('-') => Ok(Some(session.clone())),
        [other] => bail!("unexpected attach argument: {other}"),
        _ => bail!("attach accepts at most one session name"),
    }
}

fn parse_detach_session(args: &[String]) -> Result<(Option<String>, bool)> {
    let mut force = false;
    let mut session = None;
    for arg in args {
        match arg.as_str() {
            "--force" => force = true,
            other if !other.starts_with('-') && session.is_none() => {
                session = Some(other.to_string());
            }
            other => bail!("unexpected detach argument: {other}"),
        }
    }
    Ok((session, force))
}
