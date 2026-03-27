use anyhow::{Context, Result, anyhow};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::client::state::ClientState;
use crate::client::tui_app::TuiApp;
use crate::config::humu_dir;
use crate::shared::protocol::{
    ClientRequest, FrameDecoder, ServerEvent, ServerResponse, encode_frame,
};

pub struct AttachedClient {
    stream: UnixStream,
    state: ClientState,
    session_name: String,
}

impl AttachedClient {
    pub fn connect_default(session_name: &str) -> Result<Self> {
        ensure_server_running()?;
        Self::connect_socket(&humu_dir().join("server.sock"), session_name, 80, 24)
    }

    pub fn connect_socket(
        socket_path: &Path,
        session_name: &str,
        cols: u16,
        rows: u16,
    ) -> Result<Self> {
        let mut stream = UnixStream::connect(socket_path)
            .with_context(|| format!("connect daemon socket {}", socket_path.display()))?;

        let attach_response = send_request_on_stream(
            &mut stream,
            &ClientRequest::AttachSession {
                name: session_name.to_string(),
                cols,
                rows,
            },
        )?;
        let snapshot = match attach_response {
            ServerResponse::Attached {
                session_name,
                snapshot,
            } => {
                if session_name != session_name_from_snapshot(&snapshot) {
                    return Err(anyhow!("attach response session name did not match snapshot"));
                }
                snapshot
            }
            ServerResponse::AlreadyAttached {
                session_name,
                owner_pid,
                attached_at,
            } => {
                let mut details = Vec::new();
                if let Some(pid) = owner_pid {
                    details.push(format!("pid {pid}"));
                }
                if let Some(attached_at) = attached_at {
                    details.push(format!("attached at {attached_at}"));
                }
                let suffix = if details.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", details.join(", "))
                };
                return Err(anyhow!("session \"{session_name}\" is already attached{suffix}"));
            }
            other => {
                return Err(anyhow!("unexpected attach response: {other:?}"));
            }
        };

        match send_request_on_stream(&mut stream, &ClientRequest::SubscribeUpdates)? {
            ServerResponse::Subscribed { session_name } if session_name == snapshot.session_name => {
            }
            other => {
                return Err(anyhow!("unexpected subscribe response: {other:?}"));
            }
        }

        let mut state = ClientState::from_snapshot(snapshot.clone());
        state.mark_subscribed();
        Ok(Self {
            stream,
            state,
            session_name: snapshot.session_name,
        })
    }

    pub fn session_name(&self) -> &str {
        &self.session_name
    }

    pub fn state(&self) -> &ClientState {
        &self.state
    }

    pub fn send_request(&mut self, request: &ClientRequest) -> Result<ServerResponse> {
        send_request_on_stream(&mut self.stream, request)
    }

    pub fn read_event(&mut self) -> Result<ServerEvent> {
        let event: ServerEvent = read_framed_message(&mut self.stream)?;
        self.state.apply(event.clone());
        Ok(event)
    }

    pub fn read_event_timeout(&mut self, timeout: Duration) -> Result<Option<ServerEvent>> {
        self.stream
            .set_read_timeout(Some(timeout))
            .context("set attach stream read timeout")?;
        let event = match try_read_framed_message::<ServerEvent>(&mut self.stream) {
            Ok(event) => {
                self.state.apply(event.clone());
                Some(event)
            }
            Err(err) if is_timeout(&err) => None,
            Err(err) => return Err(err),
        };
        self.stream
            .set_read_timeout(None)
            .context("clear attach stream read timeout")?;
        Ok(event)
    }
}

pub fn ensure_server_running() -> Result<PathBuf> {
    let socket_path = humu_dir().join("server.sock");
    if socket_path.exists() {
        Ok(socket_path)
    } else {
        Err(anyhow!(
            "daemon socket {} does not exist",
            socket_path.display()
        ))
    }
}

pub fn attach(session_name: &str) -> Result<()> {
    ensure_server_running()?;
    let client = AttachedClient::connect_default(session_name)?;
    TuiApp::new(client).run()
}

fn session_name_from_snapshot(snapshot: &crate::shared::render::FullSnapshot) -> &str {
    &snapshot.session_name
}

fn send_request_on_stream(stream: &mut UnixStream, request: &ClientRequest) -> Result<ServerResponse> {
    stream.write_all(&encode_frame(request)?)?;
    read_framed_message(stream)
}

fn read_framed_message<T: serde::de::DeserializeOwned>(stream: &mut UnixStream) -> Result<T> {
    let mut decoder = FrameDecoder::new();
    let mut buf = [0u8; 4096];
    loop {
        let read = stream.read(&mut buf)?;
        if read == 0 {
            return Err(anyhow!("stream closed before a full frame was received"));
        }
        decoder.push(&buf[..read]);
        if let Some(message) = decoder.try_decode()? {
            return Ok(message);
        }
    }
}

fn try_read_framed_message<T: serde::de::DeserializeOwned>(stream: &mut UnixStream) -> Result<T> {
    let mut decoder = FrameDecoder::new();
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => return Err(anyhow!("stream closed before a full frame was received")),
            Ok(read) => {
                decoder.push(&buf[..read]);
                if let Some(message) = decoder.try_decode()? {
                    return Ok(message);
                }
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock || err.kind() == ErrorKind::TimedOut => {
                return Err(err.into());
            }
            Err(err) => return Err(err.into()),
        }
    }
}

fn is_timeout(err: &anyhow::Error) -> bool {
    err.downcast_ref::<std::io::Error>().is_some_and(|io_err| {
        io_err.kind() == ErrorKind::WouldBlock || io_err.kind() == ErrorKind::TimedOut
    })
}
