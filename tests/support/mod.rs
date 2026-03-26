#![allow(dead_code)]

use humu::config::{
    HumuState, PersistedRoomLayout, RoomEntry, SessionState, SplitNode, TabLayout, WorkspaceEntry,
};
use humu::id::{RoomId, WorkspaceId};
use humu::tui::layout::{PaneId, SplitTree};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::Read;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use uuid::Uuid;

#[path = "../../src/app.rs"]
#[allow(dead_code)]
mod app_impl;
#[path = "../../src/server/persistence.rs"]
pub mod persistence;

pub use app_impl::App;

pub struct TestEnv {
    pub home: TempDir,
    cwd: TempDir,
}

impl TestEnv {
    pub fn humu_dir(&self) -> &Path {
        self.home.path()
    }

    pub fn cwd(&self) -> &Path {
        self.cwd.path()
    }

    pub fn state_path(&self) -> PathBuf {
        self.humu_dir().join("state.yaml")
    }

    pub fn config_path(&self) -> PathBuf {
        self.humu_dir().join("config.yaml")
    }

    pub fn hook_port_path(&self) -> PathBuf {
        self.humu_dir().join("port")
    }

    pub fn server_socket_path(&self) -> PathBuf {
        self.humu_dir().join("server.sock")
    }

    pub fn server_metadata_path(&self) -> PathBuf {
        self.humu_dir().join("server.json")
    }

    pub fn server_lock_path(&self) -> PathBuf {
        self.humu_dir().join("server.lock")
    }

    pub fn apply_to_command(&self, command: &mut Command) {
        command
            .current_dir(self.cwd())
            .env("HOME", self.humu_dir())
            .env("HUMU_DIR", self.humu_dir());
    }
}

pub struct PtyHarness {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn std::io::Write + Send>,
    output_rx: mpsc::Receiver<Vec<u8>>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    output: Vec<u8>,
}

pub struct ScopedChild {
    child: Child,
    reap_on_drop: bool,
}

impl ScopedChild {
    pub fn process_id(&self) -> Option<u32> {
        Some(self.child.id())
    }

    pub fn child_is_alive(&mut self) -> bool {
        self.try_wait().is_none()
    }

    pub fn try_wait(&mut self) -> Option<ExitStatus> {
        match self.child.try_wait().expect("query child status") {
            Some(status) => {
                self.reap_on_drop = false;
                Some(status)
            }
            None => None,
        }
    }

    pub fn wait(&mut self) -> ExitStatus {
        let status = self.child.wait().expect("wait for child");
        self.reap_on_drop = false;
        status
    }

    pub fn kill(&mut self) {
        if self.try_wait().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        self.reap_on_drop = false;
    }

    pub fn stdout(&mut self) -> Option<&mut std::process::ChildStdout> {
        self.child.stdout.as_mut()
    }

    pub fn stderr(&mut self) -> Option<&mut std::process::ChildStderr> {
        self.child.stderr.as_mut()
    }

    pub fn stdin(&mut self) -> Option<&mut std::process::ChildStdin> {
        self.child.stdin.as_mut()
    }
}

impl Drop for ScopedChild {
    fn drop(&mut self) {
        if self.reap_on_drop {
            self.kill();
        }
    }
}

impl PtyHarness {
    pub fn spawn<S: AsRef<OsStr>>(
        command: S,
        args: &[String],
        cwd: Option<&Path>,
        cols: u16,
        rows: u16,
        envs: &[(String, String)],
    ) -> Self {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open pty");

        let mut builder = CommandBuilder::new(command);
        builder.args(args);
        if let Some(dir) = cwd {
            builder.cwd(dir);
        }
        builder.env("TERM", "xterm-256color");
        for (key, value) in envs {
            builder.env(key, value);
        }

        let child = pair.slave.spawn_command(builder).expect("spawn pty child");
        drop(pair.slave);

        let writer = pair.master.take_writer().expect("pty writer");
        let mut reader = pair.master.try_clone_reader().expect("pty reader");
        let (output_tx, output_rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if output_tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            master: pair.master,
            writer,
            output_rx,
            child,
            output: Vec::new(),
        }
    }

    pub fn child_is_alive(&mut self) -> bool {
        self.child
            .try_wait()
            .expect("query pty child status")
            .is_none()
    }

    pub fn write_input(&mut self, bytes: &[u8]) {
        use std::io::Write;

        self.writer.write_all(bytes).expect("write pty input");
        self.writer.flush().expect("flush pty input");
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("resize pty");
    }

    pub fn drain_output(&mut self) -> &[u8] {
        while let Ok(chunk) = self.output_rx.try_recv() {
            self.output.extend_from_slice(&chunk);
        }
        &self.output
    }

    pub fn output_string(&mut self) -> String {
        String::from_utf8_lossy(self.drain_output()).into_owned()
    }

    pub fn wait_for_output(&mut self, needle: &str, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.output_string().contains(needle) {
                return true;
            }
            thread::sleep(Duration::from_millis(25));
        }
        self.output_string().contains(needle)
    }
}

impl Drop for PtyHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn isolated_humu_home() -> TestEnv {
    let home = tempfile::tempdir().expect("create isolated humu home");
    let cwd = tempfile::tempdir().expect("create isolated cwd");
    std::fs::create_dir_all(home.path()).expect("ensure humu home exists");
    TestEnv { home, cwd }
}

pub fn humu_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_humu"))
}

pub fn humu_command(env: &TestEnv) -> Command {
    let mut command = Command::new(humu_binary());
    env.apply_to_command(&mut command);
    command
}

pub fn humu_server_command(env: &TestEnv) -> Command {
    let mut command = humu_command(env);
    command.arg("server");
    command
}

pub fn humu_attach_command(env: &TestEnv, session: &str) -> Command {
    let mut command = humu_command(env);
    command.arg("attach").arg(session);
    command
}

pub fn spawn_scoped_command(mut command: Command) -> ScopedChild {
    let child = command.spawn().expect("spawn scoped child");
    ScopedChild {
        child,
        reap_on_drop: true,
    }
}

pub fn spawn_humu_server(env: &TestEnv) -> ScopedChild {
    let mut command = humu_server_command(env);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    spawn_scoped_command(command)
}

pub fn spawn_humu_attach(env: &TestEnv, session: &str) -> PtyHarness {
    spawn_humu_attach_with_size(env, session, 80, 24)
}

pub fn spawn_humu_attach_with_size(
    env: &TestEnv,
    session: &str,
    cols: u16,
    rows: u16,
) -> PtyHarness {
    PtyHarness::spawn(
        humu_binary().as_os_str(),
        &["attach".to_string(), session.to_string()],
        Some(env.cwd()),
        cols,
        rows,
        &test_env_vars(env),
    )
}

pub fn run_humu_attach(env: &TestEnv, session: &str) -> ExitStatus {
    humu_attach_command(env, session)
        .status()
        .expect("run humu attach")
}

pub fn process_is_alive(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn spawn_sleeping_shell() -> PtyHarness {
    PtyHarness::spawn(
        "bash",
        &["-lc".to_string(), "printf 'ready\\n'; sleep 60".to_string()],
        None,
        80,
        24,
        &[],
    )
}

fn test_env_vars(env: &TestEnv) -> Vec<(String, String)> {
    vec![
        (
            "HOME".to_string(),
            env.humu_dir().as_os_str().to_string_lossy().into_owned(),
        ),
        (
            "HUMU_DIR".to_string(),
            env.humu_dir().as_os_str().to_string_lossy().into_owned(),
        ),
    ]
}

pub fn workspace_id(name: &str) -> WorkspaceId {
    match name {
        "humu" => WorkspaceId(Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()),
        other => panic!("unknown workspace fixture: {other}"),
    }
}

pub fn room_id(name: &str) -> RoomId {
    match name {
        "main" => RoomId(Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap()),
        "feat-x" => RoomId(Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap()),
        other => panic!("unknown room fixture: {other}"),
    }
}

pub fn legacy_state_fixture() -> HumuState {
    let ws_id = workspace_id("humu");
    let main_room_id = room_id("main");
    let feature_room_id = room_id("feat-x");
    let workspace_path = PathBuf::from("/tmp/humu");

    HumuState {
        active_workspace_id: Some(ws_id),
        active_room_id: Some(main_room_id),
        workspaces: vec![WorkspaceEntry {
            name: "humu".to_string(),
            id: ws_id,
            path: workspace_path.clone(),
            last_room_id: Some(main_room_id),
            rooms: vec![
                RoomEntry {
                    name: "main".to_string(),
                    id: main_room_id,
                    path: workspace_path.clone(),
                    active_tab: Some(0),
                    tabs: vec![TabLayout {
                        name: "shell".to_string(),
                        split: SplitNode::Leaf {
                            preset: "shell".to_string(),
                            session_id: None,
                        },
                    }],
                },
                RoomEntry {
                    name: "feat-x".to_string(),
                    id: feature_room_id,
                    path: workspace_path.join("feat-x"),
                    active_tab: None,
                    tabs: vec![],
                },
            ],
        }],
        panel_widths: Some([24, 20]),
        sessions: vec![],
    }
}

pub fn migrated_state_fixture() -> HumuState {
    persistence::migrate_legacy_state(legacy_state_fixture())
}

pub fn app_with_migrated_state() -> App {
    let mut app = App::test_with_state(migrated_state_fixture(), temp_state_path());
    let pane_id = PaneId::new();
    app.tabs.add_tab("shell".into(), SplitTree::leaf(pane_id));
    app.pane_presets.insert(pane_id, "shell".to_string());
    app.focused_pane = Some(pane_id);
    app
}

pub fn reload_state(app: &mut App) -> HumuState {
    app.test_persist_layout();
    persistence::load_state(app.test_state_path()).expect("reload state")
}

pub fn round_trip_state(state: HumuState) -> HumuState {
    let path = temp_state_path();
    persistence::save_state(&path, &state).expect("save state");
    persistence::load_state(&path).expect("load state")
}

pub fn persist_named_session_layout(
    state: &mut HumuState,
    session_name: &str,
    room_name: &str,
    tab_name: &str,
) {
    let ws_id = workspace_id("humu");
    let session = state.ensure_session(session_name);
    session.active_workspace_id = Some(ws_id);
    session.active_room_id = Some(room_id(room_name));
    insert_session_room_layout(state, session_name, room_name, tab_name);
}

pub fn insert_session_room_layout(
    state: &mut HumuState,
    session_name: &str,
    room_name: &str,
    tab_name: &str,
) {
    let room_id = room_id(room_name);
    state.ensure_session(session_name).tabs_by_room.insert(
        room_id,
        PersistedRoomLayout {
            active_tab: 0,
            tabs: vec![TabLayout {
                name: tab_name.to_string(),
                split: SplitNode::Leaf {
                    preset: "shell".to_string(),
                    session_id: None,
                },
            }],
        },
    );
}

pub fn default_session(state: &HumuState) -> &SessionState {
    state
        .session_by_name(persistence::DEFAULT_SESSION_NAME)
        .expect("default session")
}

pub fn temp_state_path() -> PathBuf {
    std::env::temp_dir().join(format!("humu-session-persistence-{}.yaml", Uuid::new_v4()))
}

pub fn session_by_name<'a>(state: &'a HumuState, name: &str) -> &'a SessionState {
    state.session_by_name(name).expect("session not found")
}

pub fn workspace_room_paths() -> (PathBuf, PathBuf) {
    let workspace_path = PathBuf::from("/tmp/humu");
    let feature_path = workspace_path.join("feat-x");
    (workspace_path, feature_path)
}
