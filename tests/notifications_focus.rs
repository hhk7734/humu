#[path = "../src/server/mod.rs"]
#[allow(dead_code)]
mod server_impl;
mod support;

use humu::config::{
    NotificationsConfig, OsNotificationConfig, SoundNotificationConfig, TelegramNotificationConfig,
};
use humu::hook::http::AgentState;
use humu::id::PaneId;
use server_impl::runtime::{RuntimeUpdateSource, SessionRuntime};
use std::fs;
use std::net::TcpStream;
use std::time::{Duration, Instant, SystemTime};
use tempfile::tempdir;

fn disabled_notifications() -> NotificationsConfig {
    NotificationsConfig {
        os: OsNotificationConfig {
            enabled: false,
            only_unfocused: true,
        },
        sound: SoundNotificationConfig {
            enabled: false,
            only_unfocused: true,
        },
        telegram: TelegramNotificationConfig::default(),
    }
}

fn wait_for(condition: impl Fn() -> bool, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(condition(), "condition not met before timeout");
}

#[test]
fn detached_session_is_treated_as_unfocused_for_notifications() {
    let humu_dir = tempdir().expect("temp humu dir");
    let codex_dir = tempdir().expect("temp codex dir");
    let runtime = SessionRuntime::start(
        humu_dir.path().to_path_buf(),
        disabled_notifications(),
        codex_dir.path().to_path_buf(),
    )
    .expect("start runtime");

    assert!(runtime.session_focus("default").delivers_only_unfocused());

    runtime.attach_session("default");
    assert!(!runtime.session_focus("default").delivers_only_unfocused());

    runtime.update_session_focus("default", false);
    assert!(runtime.session_focus("default").delivers_only_unfocused());

    runtime.detach_session("default");
    assert!(runtime.session_focus("default").delivers_only_unfocused());
}

#[test]
fn daemon_publishes_hook_port_file() {
    let env = support::isolated_humu_home();
    let _child = support::spawn_humu_server(&env);

    wait_for(|| env.hook_port_path().exists(), Duration::from_secs(5));

    let port = fs::read_to_string(env.hook_port_path())
        .expect("read hook port file")
        .trim()
        .parse::<u16>()
        .expect("parse hook port");
    assert!(port > 0);

    wait_for(
        || TcpStream::connect(("127.0.0.1", port)).is_ok(),
        Duration::from_secs(5),
    );
}

#[tokio::test]
async fn detached_hook_and_codex_updates_continue_without_client() {
    let humu_dir = tempdir().expect("temp humu dir");
    let codex_dir = tempdir().expect("temp codex dir");
    let runtime = SessionRuntime::start(
        humu_dir.path().to_path_buf(),
        disabled_notifications(),
        codex_dir.path().to_path_buf(),
    )
    .expect("start runtime");

    let hook_pane_id = PaneId::new();
    runtime.register_pane(
        "default",
        hook_pane_id,
        humu_dir.path().join("hook-workspace"),
        None,
        SystemTime::now(),
    );

    let codex_workspace = humu_dir.path().join("codex-workspace");
    fs::create_dir_all(&codex_workspace).expect("create codex workspace");
    let codex_pane_id = PaneId::new();
    runtime.register_pane(
        "default",
        codex_pane_id,
        codex_workspace.clone(),
        None,
        SystemTime::now(),
    );

    let session_root = codex_dir.path().join("2026/03/27");
    fs::create_dir_all(&session_root).expect("create codex session root");
    fs::write(
        session_root.join("task-2026-03-27T00-00-00-019d015a-ab86-7680-84a1-f48751186599.jsonl"),
        format!(
            "{{\"timestamp\":\"2026-03-27T00:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"019d015a-ab86-7680-84a1-f48751186599\",\"cwd\":\"{}\"}}}}\n\
{{\"timestamp\":\"2026-03-27T00:00:01.000Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_started\"}}}}\n",
            codex_workspace.display(),
        ),
    )
    .expect("write codex session file");

    let url = format!(
        "http://127.0.0.1:{}/hook?paneId={}&eventType=PostToolUse&sessionId=hook-session",
        runtime.hook_port(),
        hook_pane_id
    );
    let response = reqwest::Client::new()
        .post(url)
        .send()
        .await
        .expect("send hook event");
    assert_eq!(response.status(), 200);

    wait_for(
        || {
            let updates = runtime.recorded_updates();
            updates.iter().any(|update| {
                update.source == RuntimeUpdateSource::Hook
                    && update.pane_id == hook_pane_id
                    && update.state == AgentState::Working
            }) && updates.iter().any(|update| {
                update.source == RuntimeUpdateSource::Codex
                    && update.pane_id == codex_pane_id
                    && update.state == AgentState::Working
            })
        },
        Duration::from_secs(5),
    );
}
