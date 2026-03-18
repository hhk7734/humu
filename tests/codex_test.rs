use humu::codex::CodexTracker;
use humu::hook::http::AgentState;
use humu::id::PaneId;
use std::fs;
use std::time::SystemTime;
use tempfile::tempdir;

#[test]
fn codex_tracker_discovers_session_and_reports_working_then_idle() {
    let dir = tempdir().unwrap();
    let sessions_root = dir.path().join("sessions/2026/03/18");
    fs::create_dir_all(&sessions_root).unwrap();

    let cwd = dir.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();

    let session_id = "019d015a-ab86-7680-84a1-f48751186599";
    let session_path = sessions_root.join(format!("rollout-2026-03-18T23-30-12-{session_id}.jsonl"));
    fs::write(
        &session_path,
        format!(
            "{{\"timestamp\":\"2026-03-18T14:30:12.000Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{}\",\"cwd\":\"{}\"}}}}\n\
{{\"timestamp\":\"2026-03-18T14:30:13.000Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_started\"}}}}\n",
            session_id,
            cwd.display(),
        ),
    )
    .unwrap();

    let pane_id = PaneId::new();
    let mut tracker = CodexTracker::new(dir.path().join("sessions"));
    tracker.track_pane(pane_id, cwd.clone(), None, SystemTime::now());

    let updates = tracker.poll();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].pane_id, pane_id);
    assert_eq!(updates[0].state, AgentState::Working);
    assert_eq!(updates[0].session_id.as_deref(), Some(session_id));

    fs::write(
        &session_path,
        format!(
            "{{\"timestamp\":\"2026-03-18T14:30:12.000Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{}\",\"cwd\":\"{}\"}}}}\n\
{{\"timestamp\":\"2026-03-18T14:30:13.000Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_started\"}}}}\n\
{{\"timestamp\":\"2026-03-18T14:30:20.000Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_complete\"}}}}\n",
            session_id,
            cwd.display(),
        ),
    )
    .unwrap();

    let updates = tracker.poll();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].state, AgentState::Idle);
    assert_eq!(updates[0].session_id.as_deref(), Some(session_id));
}

#[test]
fn codex_tracker_finds_known_session_id_without_cwd_matching() {
    let dir = tempdir().unwrap();
    let sessions_root = dir.path().join("sessions/2026/03/18");
    fs::create_dir_all(&sessions_root).unwrap();

    let session_id = "019d0159-ac86-7092-80e6-2062bac8e3b8";
    let session_path = sessions_root.join(format!("rollout-2026-03-18T23-29-07-{session_id}.jsonl"));
    fs::write(
        &session_path,
        concat!(
            "{\"timestamp\":\"2026-03-18T14:29:07.337Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"019d0159-ac86-7092-80e6-2062bac8e3b8\",\"cwd\":\"/tmp/other\"}}\n",
            "{\"timestamp\":\"2026-03-18T14:29:18.331Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\"}}\n"
        ),
    )
    .unwrap();

    let pane_id = PaneId::new();
    let mut tracker = CodexTracker::new(dir.path().join("sessions"));
    tracker.track_pane(
        pane_id,
        dir.path().join("unrelated"),
        Some(session_id.to_string()),
        SystemTime::now(),
    );

    let updates = tracker.poll();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].state, AgentState::Idle);
    assert_eq!(updates[0].session_id.as_deref(), Some(session_id));
}
