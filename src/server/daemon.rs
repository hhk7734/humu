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
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
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
    if daemon {
        return launch_daemonized_server();
    }

    run_foreground()
}

fn run_foreground() -> Result<()> {
    let paths = DaemonPaths::default();
    log::init();
    generate_hook_files(&humu_dir()).context("generate hook files for daemon shell")?;

    if existing_daemon_ready(&paths, "server startup")? {
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

fn launch_daemonized_server() -> Result<()> {
    let paths = DaemonPaths::default();
    if existing_daemon_ready(&paths, "daemonized server startup")? {
        return Ok(());
    }

    let mut child = spawn_daemon_child()?;
    wait_for_daemon_ready(&paths, &mut child)
}

fn spawn_daemon_child() -> Result<Child> {
    let current_exe = std::env::current_exe().context("resolve current humu binary")?;
    let mut command = Command::new(current_exe);
    command
        .arg("server")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
        .spawn()
        .context("spawn daemonized humu server child")
}

fn wait_for_daemon_ready(paths: &DaemonPaths, child: &mut Child) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if existing_daemon_ready(paths, "daemonized server readiness")? {
            return Ok(());
        }
        if let Some(status) = child.try_wait().context("poll daemonized child status")? {
            bail!("daemonized server child exited before readiness: {status}");
        }
        if Instant::now() >= deadline {
            bail!("timed out waiting for daemonized server startup");
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn existing_daemon_ready(paths: &DaemonPaths, context_label: &str) -> Result<bool> {
    match ping_protocol_version(paths) {
        Ok(protocol_version) if protocol_version == PROTOCOL_VERSION => Ok(true),
        Ok(protocol_version) => bail!(
            "protocol version mismatch for {context_label}: client={} server={protocol_version}",
            PROTOCOL_VERSION
        ),
        Err(_) => Ok(false),
    }
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
                let owner_pid = session
                    .owner_pid
                    .map(|pid| pid.to_string())
                    .unwrap_or_else(|| "-".to_string());
                let attached_at = session.attached_at.unwrap_or_else(|| "-".to_string());
                println!(
                    "{}\t{}\towner_pid={}\tattached_at={}",
                    session.name, state, owner_pid, attached_at
                );
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
                if existing_daemon_ready(paths, "daemon startup lock wait")? {
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
    if existing_daemon_ready(paths, "stale runtime cleanup")? {
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
        let owner_pid = peer_pid(&stream);
        let client_id = format!(
            "client-{}",
            next_client_id.fetch_add(1, Ordering::Relaxed)
        );
        thread::spawn(move || {
            let _ = handle_client(stream, sessions, client_id, owner_pid);
        });
    }
}

fn handle_client(
    mut stream: UnixStream,
    sessions: Arc<Mutex<SessionManager>>,
    client_id: String,
    owner_pid: Option<u32>,
) -> Result<()> {
    let mut attached_session = None;
    let mut decoder = humu::shared::protocol::FrameDecoder::new();
    let mut buf = [0u8; 4096];
    let result = (|| -> Result<()> {
        loop {
        let read = stream.read(&mut buf)?;
        if read == 0 {
                return Ok(());
        }
        decoder.push(&buf[..read]);
        while let Some(request) = decoder.try_decode::<ClientRequest>()? {
                let response = handle_request(
                    request,
                    &sessions,
                    &client_id,
                    owner_pid,
                    &mut attached_session,
                )?;
            stream.write_all(&encode_frame(&response)?)?;
        }
        }
    })();

    if let Some(session_name) = attached_session {
        let mut sessions = sessions.lock().expect("session manager lock");
        sessions.detach_owned(&session_name, &client_id);
    }
    result
}

fn handle_request(
    request: ClientRequest,
    sessions: &Arc<Mutex<SessionManager>>,
    client_id: &str,
    owner_pid: Option<u32>,
    attached_session: &mut Option<String>,
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
            let mut owner =
                AttachOwner::new(client_id.to_string()).with_attached_at(current_timestamp());
            if let Some(pid) = owner_pid {
                owner = owner.with_pid(pid);
            }
            let previous_session = attached_session.clone();
            match sessions.attach(&name, owner) {
                Ok(_) => {
                    if let Some(current) = previous_session
                        && current != name
                    {
                        sessions.detach_owned(&current, client_id);
                    }
                    sessions.record_size(&name, cols, rows);
                    *attached_session = Some(name.clone());
                    Ok(ServerResponse::Attached {
                        session_name: name.clone(),
                        snapshot: sessions.snapshot(&name),
                    })
                }
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
            if attached_session.as_deref() == Some(name.as_str()) {
                *attached_session = None;
            }
            Ok(ServerResponse::Detached { session_name: name })
        }
        ClientRequest::Detach => {
            let Some(session_name) = attached_session.take() else {
                return Ok(ServerResponse::Ack);
            };
            sessions.detach_owned(&session_name, client_id);
            Ok(ServerResponse::Detached { session_name })
        }
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

#[cfg(target_os = "linux")]
fn peer_pid(stream: &UnixStream) -> Option<u32> {
    #[repr(C)]
    struct UCred {
        pid: i32,
        uid: u32,
        gid: u32,
    }

    unsafe extern "C" {
        fn getsockopt(
            socket: i32,
            level: i32,
            option_name: i32,
            option_value: *mut core::ffi::c_void,
            option_len: *mut u32,
        ) -> i32;
    }

    const SOL_SOCKET: i32 = 1;
    const SO_PEERCRED: i32 = 17;

    let mut creds = UCred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<UCred>() as u32;
    let rc = unsafe {
        getsockopt(
            stream.as_raw_fd(),
            SOL_SOCKET,
            SO_PEERCRED,
            (&mut creds as *mut UCred).cast(),
            &mut len,
        )
    };
    if rc == 0 && creds.pid > 0 {
        return Some(creds.pid as u32);
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn peer_pid(_stream: &UnixStream) -> Option<u32> {
    None
}
