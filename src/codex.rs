use crate::hook::http::AgentState;
use crate::id::PaneId;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone)]
pub struct CodexUpdate {
    pub pane_id: PaneId,
    pub state: AgentState,
    pub session_id: Option<String>,
}

#[derive(Debug)]
struct CodexPaneTracker {
    cwd: PathBuf,
    started_at: SystemTime,
    session_id: Option<String>,
    session_path: Option<PathBuf>,
    last_state: Option<AgentState>,
}

pub struct CodexTracker {
    sessions_root: PathBuf,
    panes: HashMap<PaneId, CodexPaneTracker>,
}

impl CodexTracker {
    pub fn new(sessions_root: PathBuf) -> Self {
        Self {
            sessions_root,
            panes: HashMap::new(),
        }
    }

    pub fn track_pane(
        &mut self,
        pane_id: PaneId,
        cwd: PathBuf,
        session_id: Option<String>,
        started_at: SystemTime,
    ) {
        self.panes.insert(
            pane_id,
            CodexPaneTracker {
                cwd,
                started_at,
                session_id,
                session_path: None,
                last_state: None,
            },
        );
    }

    pub fn remove_pane(&mut self, pane_id: PaneId) {
        self.panes.remove(&pane_id);
    }

    pub fn poll(&mut self) -> Vec<CodexUpdate> {
        let pane_ids: Vec<PaneId> = self.panes.keys().copied().collect();
        let mut updates = Vec::new();

        for pane_id in pane_ids {
            let Some(tracker) = self.panes.get_mut(&pane_id) else {
                continue;
            };

            if tracker.session_path.is_none() {
                tracker.session_path = if let Some(session_id) = tracker.session_id.as_deref() {
                    find_session_path_by_id(&self.sessions_root, session_id)
                } else {
                    find_session_path_for_cwd(&self.sessions_root, &tracker.cwd, tracker.started_at)
                };
            }

            let Some(session_path) = tracker.session_path.clone() else {
                continue;
            };

            let Ok(summary) = read_session_summary(&session_path) else {
                continue;
            };

            if tracker.session_id.is_none() {
                tracker.session_id = Some(summary.session_id.clone());
            }

            if tracker.last_state.as_ref() != Some(&summary.state) {
                tracker.last_state = Some(summary.state.clone());
                updates.push(CodexUpdate {
                    pane_id,
                    state: summary.state,
                    session_id: tracker.session_id.clone(),
                });
            }
        }

        updates
    }
}

#[derive(Debug)]
struct SessionSummary {
    session_id: String,
    state: AgentState,
}

#[derive(Debug, Deserialize)]
struct SessionMetaLine {
    payload: SessionMetaPayload,
}

#[derive(Debug, Deserialize)]
struct SessionMetaPayload {
    id: String,
    cwd: PathBuf,
}

#[derive(Debug, Deserialize)]
struct EventLine {
    payload: EventPayload,
}

#[derive(Debug, Deserialize)]
struct EventPayload {
    #[serde(rename = "type")]
    event_type: String,
}

fn read_session_summary(path: &Path) -> anyhow::Result<SessionSummary> {
    let content = fs::read_to_string(path)?;
    let mut lines = content.lines();
    let first = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing session meta"))?;
    let meta: SessionMetaLine = serde_json::from_str(first)?;

    let mut state = AgentState::Idle;
    for line in lines {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if value.get("type").and_then(serde_json::Value::as_str) != Some("event_msg") {
            continue;
        }
        let Ok(event) = serde_json::from_value::<EventLine>(value) else {
            continue;
        };
        match event.payload.event_type.as_str() {
            "task_started" => state = AgentState::Working,
            "task_complete" => state = AgentState::Idle,
            _ => {}
        }
    }

    Ok(SessionSummary {
        session_id: meta.payload.id,
        state,
    })
}

fn find_session_path_by_id(root: &Path, session_id: &str) -> Option<PathBuf> {
    for path in session_files(root) {
        if path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|name| name.ends_with(&format!("{session_id}.jsonl")))
        {
            return Some(path);
        }
    }
    None
}

fn find_session_path_for_cwd(root: &Path, cwd: &Path, started_at: SystemTime) -> Option<PathBuf> {
    let slack = started_at
        .checked_sub(Duration::from_secs(10))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut best: Option<(SystemTime, PathBuf)> = None;

    for path in session_files(root) {
        let Ok(meta) = read_session_meta(&path) else {
            continue;
        };
        if meta.payload.cwd != cwd {
            continue;
        }
        let modified = fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if modified < slack {
            continue;
        }
        let is_better = best
            .as_ref()
            .is_none_or(|(best_time, _)| modified > *best_time);
        if is_better {
            best = Some((modified, path));
        }
    }

    best.map(|(_, path)| path)
}

fn read_session_meta(path: &Path) -> anyhow::Result<SessionMetaLine> {
    let content = fs::read_to_string(path)?;
    let first = content
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing session meta"))?;
    Ok(serde_json::from_str(first)?)
}

fn session_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_session_files(root, &mut files);
    files
}

fn collect_session_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_session_files(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            out.push(path);
        }
    }
}
