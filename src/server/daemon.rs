use super::session::{AttachError, AttachOwner, SessionManager};
use anyhow::{Context, Result, bail};
use humu::config::humu_dir;
use humu::hook::http::generate_hook_files;
use humu::log;
use humu::shared::protocol::{ClientRequest, PROTOCOL_VERSION, ServerResponse, encode_frame};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
struct DaemonPaths {
    socket_path: PathBuf,
    metadata_path: PathBuf,
    lock_path: PathBuf,
}

impl DaemonPaths {
    fn default() -> Self {
        let base = humu_dir();
        Self {
            socket_path: base.join("server.sock"),
            metadata_path: base.join("server.json"),
            lock_path: base.join("server.lock"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonMetadata {
    pub pid: u32,
    pub started_at: u64,
    pub socket_path: String,
    pub protocol_version: u32,
}

struct StartupLock {
    path: PathBuf,
}

impl Drop for StartupLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

struct RuntimeFiles {
    socket_path: PathBuf,
    metadata_path: PathBuf,
}

impl Drop for RuntimeFiles {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket_path);
        let _ = fs::remove_file(&self.metadata_path);
    }
}

pub fn run(daemon: bool) -> Result<()> {
    let _daemon = daemon;
    let paths = DaemonPaths::default();
    log::init();
    generate_hook_files(&humu_dir()).context("generate hook files for daemon shell")?;

    if ping_protocol_version(&paths).is_ok() {
        return Ok(());
    }

    let Some(startup_lock) = acquire_startup_lock(&paths)? else {
        return Ok(());
    };
    cleanup_stale_runtime_files(&paths)?;

    let listener = bind_listener(&paths)?;
    write_metadata(&paths)?;
    drop(startup_lock);

    let _runtime_files = RuntimeFiles {
        socket_path: paths.socket_path.clone(),
        metadata_path: paths.metadata_path.clone(),
    };
    serve(listener)
}

pub fn attach_shell(session_name: &str) -> Result<()> {
    let paths = DaemonPaths::default();
    let metadata = read_metadata(&paths).context("read daemon metadata for attach")?;
    let protocol_version = ping_protocol_version(&paths).context("ping daemon for attach")?;
    if protocol_version != PROTOCOL_VERSION {
        bail!(
            "protocol version mismatch for session {session_name}: client={} server={protocol_version}",
            PROTOCOL_VERSION
        );
    }
    if metadata.protocol_version != protocol_version {
        bail!(
            "daemon metadata protocol mismatch for session {session_name}: metadata={} live={protocol_version}",
            metadata.protocol_version
        );
    }
    bail!("attach client is not implemented yet for session {session_name}");
}

pub fn list_sessions_shell() -> Result<()> {
    let paths = DaemonPaths::default();
    let protocol_version = ping_protocol_version(&paths).context("ping daemon for list-sessions")?;
    if protocol_version != PROTOCOL_VERSION {
        bail!(
            "protocol version mismatch for list-sessions: client={} server={protocol_version}",
            PROTOCOL_VERSION
        );
    }

    let response = send_request::<ServerResponse>(&paths, &ClientRequest::ListSessions)?;
    match response {
        ServerResponse::Sessions { sessions } => {
            for session in sessions {
                let state = if session.attached {
                    "attached"
                } else {
                    "detached"
                };
                println!("{}\t{}", session.name, state);
            }
            Ok(())
        }
        other => bail!("unexpected list-sessions response: {other:?}"),
    }
}

pub fn force_detach_shell(session_name: &str) -> Result<()> {
    let paths = DaemonPaths::default();
    let protocol_version = ping_protocol_version(&paths).context("ping daemon for detach")?;
    if protocol_version != PROTOCOL_VERSION {
        bail!(
            "protocol version mismatch for detach: client={} server={protocol_version}",
            PROTOCOL_VERSION
        );
    }

    let response = send_request::<ServerResponse>(
        &paths,
        &ClientRequest::ForceDetachSession {
            name: session_name.to_string(),
        },
    )?;
    match response {
        ServerResponse::Detached { .. } | ServerResponse::Ack => Ok(()),
        ServerResponse::Error { message } => bail!(message),
        other => bail!("unexpected detach response: {other:?}"),
    }
}

fn acquire_startup_lock(paths: &DaemonPaths) -> Result<Option<StartupLock>> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&paths.lock_path)
        {
            Ok(mut file) => {
                writeln!(file, "{}", std::process::id())?;
                return Ok(Some(StartupLock {
                    path: paths.lock_path.clone(),
                }));
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                if lock_holder_is_dead(&paths.lock_path)? {
                    let _ = fs::remove_file(&paths.lock_path);
                    continue;
                }
                if ping_protocol_version(paths).is_ok() {
                    return Ok(None);
                }
                if Instant::now() >= deadline {
                    bail!(
                        "timed out waiting for daemon startup lock at {}",
                        paths.lock_path.display()
                    );
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(err) => return Err(err.into()),
        }
    }
}

fn lock_holder_is_dead(lock_path: &PathBuf) -> Result<bool> {
    let Ok(contents) = fs::read_to_string(lock_path) else {
        return Ok(false);
    };
    let Ok(pid) = contents.trim().parse::<u32>() else {
        return Ok(false);
    };
    Ok(!process_is_alive(pid))
}

fn cleanup_stale_runtime_files(paths: &DaemonPaths) -> Result<()> {
    if ping_protocol_version(paths).is_ok() {
        bail!("daemon already running");
    }

    let metadata = read_metadata(paths).ok();
    if let Some(metadata) = &metadata
        && process_is_alive(metadata.pid)
    {
        bail!(
            "daemon pid {} is still alive but the socket did not answer ping",
            metadata.pid
        );
    }

    if paths.socket_path.exists() {
        let _ = fs::remove_file(&paths.socket_path);
    }
    if metadata.is_some() || paths.metadata_path.exists() {
        let _ = fs::remove_file(&paths.metadata_path);
    }
    Ok(())
}

fn bind_listener(paths: &DaemonPaths) -> Result<UnixListener> {
    if let Some(parent) = paths.socket_path.parent() {
        fs::create_dir_all(parent)?;
    }
    UnixListener::bind(&paths.socket_path)
        .with_context(|| format!("bind daemon socket at {}", paths.socket_path.display()))
}

fn write_metadata(paths: &DaemonPaths) -> Result<()> {
    let metadata = DaemonMetadata {
        pid: std::process::id(),
        started_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        socket_path: paths.socket_path.to_string_lossy().into_owned(),
        protocol_version: PROTOCOL_VERSION,
    };
    fs::write(
        &paths.metadata_path,
        serde_json::to_vec_pretty(&metadata).context("serialize daemon metadata")?,
    )
    .with_context(|| format!("write daemon metadata at {}", paths.metadata_path.display()))
}

fn read_metadata(paths: &DaemonPaths) -> Result<DaemonMetadata> {
    let bytes = fs::read(&paths.metadata_path)
        .with_context(|| format!("read daemon metadata at {}", paths.metadata_path.display()))?;
    serde_json::from_slice(&bytes).context("parse daemon metadata")
}

fn ping_protocol_version(paths: &DaemonPaths) -> Result<u32> {
    match send_request::<ServerResponse>(paths, &ClientRequest::Ping)? {
        ServerResponse::Pong { protocol_version } => Ok(protocol_version),
        other => bail!("unexpected ping response: {other:?}"),
    }
}

fn send_request<T: DeserializeOwned>(paths: &DaemonPaths, request: &ClientRequest) -> Result<T> {
    let mut stream = UnixStream::connect(&paths.socket_path)
        .with_context(|| format!("connect to daemon at {}", paths.socket_path.display()))?;
    stream.write_all(&encode_frame(request)?)?;
    let _ = stream.shutdown(Shutdown::Write);
    read_framed_message(&mut stream)
}

fn read_framed_message<T: DeserializeOwned>(stream: &mut UnixStream) -> Result<T> {
    let mut decoder = humu::shared::protocol::FrameDecoder::new();
    let mut buf = [0u8; 4096];
    loop {
        let read = stream.read(&mut buf)?;
        if read == 0 {
            bail!("stream closed before a full frame was received");
        }
        decoder.push(&buf[..read]);
        if let Some(message) = decoder.try_decode()? {
            return Ok(message);
        }
    }
}

fn serve(listener: UnixListener) -> Result<()> {
    let sessions = Arc::new(Mutex::new(SessionManager::default()));
    let next_client_id = Arc::new(AtomicU64::new(1));
    loop {
        let (stream, _) = listener.accept().context("accept daemon client")?;
        let sessions = Arc::clone(&sessions);
        let client_id = format!(
            "client-{}",
            next_client_id.fetch_add(1, Ordering::Relaxed)
        );
        thread::spawn(move || {
            let _ = handle_client(stream, sessions, client_id);
        });
    }
}

fn handle_client(
    mut stream: UnixStream,
    sessions: Arc<Mutex<SessionManager>>,
    client_id: String,
) -> Result<()> {
    let mut decoder = humu::shared::protocol::FrameDecoder::new();
    let mut buf = [0u8; 4096];
    loop {
        let read = stream.read(&mut buf)?;
        if read == 0 {
            return Ok(());
        }
        decoder.push(&buf[..read]);
        while let Some(request) = decoder.try_decode::<ClientRequest>()? {
            let response = handle_request(request, &sessions, &client_id)?;
            stream.write_all(&encode_frame(&response)?)?;
        }
    }
}

fn handle_request(
    request: ClientRequest,
    sessions: &Arc<Mutex<SessionManager>>,
    client_id: &str,
) -> Result<ServerResponse> {
    let mut sessions = sessions.lock().expect("session manager lock");
    match request {
        ClientRequest::Ping => Ok(ServerResponse::Pong {
            protocol_version: PROTOCOL_VERSION,
        }),
        ClientRequest::ListSessions => Ok(ServerResponse::Sessions {
            sessions: sessions.list(),
        }),
        ClientRequest::CreateSession { name } => Ok(ServerResponse::SessionCreated {
            session: sessions.create(&name),
        }),
        ClientRequest::AttachSession { name, cols, rows } => {
            sessions.record_size(&name, cols, rows);
            let owner = AttachOwner::new(client_id.to_string()).with_attached_at(current_timestamp());
            match sessions.attach(&name, owner) {
                Ok(_) => Ok(ServerResponse::Attached {
                    session_name: name.clone(),
                    snapshot: sessions.snapshot(&name),
                }),
                Err(AttachError::AlreadyAttached {
                    session_name,
                    owner_pid,
                    attached_at,
                }) => Ok(ServerResponse::AlreadyAttached {
                    session_name,
                    owner_pid,
                    attached_at,
                }),
            }
        }
        ClientRequest::ForceDetachSession { name } => {
            sessions.detach(&name);
            Ok(ServerResponse::Detached { session_name: name })
        }
        ClientRequest::Detach => Ok(ServerResponse::Ack),
        ClientRequest::ResizeSession { .. }
        | ClientRequest::RunAction { .. }
        | ClientRequest::SendInput { .. }
        | ClientRequest::SubscribeUpdates
        | ClientRequest::FocusChanged { .. } => Ok(ServerResponse::Error {
            message: "server shell only supports ping/session registry commands in Task 4"
                .to_string(),
        }),
    }
}

fn current_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

fn process_is_alive(pid: u32) -> bool {
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
