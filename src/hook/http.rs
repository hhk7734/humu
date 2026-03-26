use crate::id::PaneId;
use axum::Router;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::post;
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentState {
    Working,
    NeedsInput,
    Idle,
}

#[derive(Debug, Clone)]
pub struct HookEvent {
    pub workspace_id: Option<String>,
    pub room_id: Option<String>,
    pub tab_id: Option<String>,
    pub pane_id: PaneId,
    pub event_type: AgentState,
    pub session_id: Option<String>,
}

pub fn generate_hook_files(base_dir: &Path) -> anyhow::Result<()> {
    let hooks_dir = base_dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;

    // Generate notify.sh
    let notify_path = hooks_dir.join("notify.sh");
    std::fs::write(
        &notify_path,
        r#"#!/bin/bash
command -v curl &>/dev/null || exit 0
INPUT=$(cat)
# Try Claude's event name
EVENT=$(echo "$INPUT" | grep -oE '"hook_event_name"\s*:\s*"[^"]*"' | grep -oE '"[^"]*"$' | tr -d '"')
# If empty, try Gemini's notification_type
if [ -z "$EVENT" ]; then
  EVENT=$(echo "$INPUT" | grep -oE '"notification_type"\s*:\s*"[^"]*"' | grep -oE '"[^"]*"$' | tr -d '"')
fi
# If still empty, try Gemini's event (for BeforeAgent/AfterAgent which might not have notification_type)
if [ -z "$EVENT" ]; then
  EVENT=$(echo "$INPUT" | grep -oE '"event"\s*:\s*"[^"]*"' | grep -oE '"[^"]*"$' | tr -d '"')
fi

SESSION=$(echo "$INPUT" | grep -oE '"session_id"\s*:\s*"[^"]*"' | head -1 | grep -oE '"[^"]*"$' | tr -d '"')
[ -z "$HUMU_PORT" ] && exit 0
curl -s --connect-timeout 1 --max-time 2 -X POST \
  "http://127.0.0.1:${HUMU_PORT}/hook?workspaceId=${HUMU_WORKSPACE_ID}&roomId=${HUMU_ROOM_ID}&tabId=${HUMU_TAB_ID}&paneId=${HUMU_PANE_ID}&eventType=${EVENT}&sessionId=${SESSION}" \
  >/dev/null 2>&1 || true

# Gemini hooks MUST return JSON
echo '{"status":"ok"}'
"#,
    )?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&notify_path, std::fs::Permissions::from_mode(0o755))?;
    }

    let notify_abs = notify_path.to_string_lossy();

    // Generate claude-settings.json
    let claude_settings_path = hooks_dir.join("claude-settings.json");
    let claude_settings = serde_json::json!({
        "hooks": {
            "UserPromptSubmit": [{"hooks": [{"type": "command", "command": notify_abs}]}],
            "Stop": [{"hooks": [{"type": "command", "command": notify_abs}]}],
            "SessionEnd": [{"hooks": [{"type": "command", "command": notify_abs}]}],
            "PostToolUse": [{"matcher": "*", "hooks": [{"type": "command", "command": notify_abs}]}],
            "PostToolUseFailure": [{"matcher": "*", "hooks": [{"type": "command", "command": notify_abs}]}],
            "PermissionRequest": [{"matcher": "*", "hooks": [{"type": "command", "command": notify_abs}]}]
        }
    });
    std::fs::write(
        &claude_settings_path,
        serde_json::to_string_pretty(&claude_settings)?,
    )?;

    // Generate gemini-settings.json
    let gemini_settings_path = hooks_dir.join("gemini-settings.json");
    let gemini_settings = serde_json::json!({
        "hooks": {
            "BeforeAgent": [{"hooks": [{"type": "command", "command": notify_abs}]}],
            "AfterAgent": [{"hooks": [{"type": "command", "command": notify_abs}]}],
            "Notification": [{"matcher": "*", "hooks": [{"type": "command", "command": notify_abs}]}],
            "BeforeTool": [{"matcher": "*", "hooks": [{"type": "command", "command": notify_abs}]}],
            "AfterTool": [{"matcher": "*", "hooks": [{"type": "command", "command": notify_abs}]}]
        }
    });
    std::fs::write(
        &gemini_settings_path,
        serde_json::to_string_pretty(&gemini_settings)?,
    )?;

    Ok(())
}

pub struct HookServer {
    port: u16,
    tx: broadcast::Sender<HookEvent>,
    task: JoinHandle<()>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HookParams {
    workspace_id: Option<String>,
    room_id: Option<String>,
    tab_id: Option<String>,
    pane_id: Option<String>,
    event_type: Option<String>,
    session_id: Option<String>,
}

fn map_event_type(raw: &str) -> Option<AgentState> {
    match raw {
        // Claude Code
        "UserPromptSubmit" | "PostToolUse" => Some(AgentState::Working),
        "PostToolUseFailure" => Some(AgentState::Working), // may also fire on interrupt but next event clarifies
        "PermissionRequest" => Some(AgentState::NeedsInput),
        "Stop" | "SessionEnd" => Some(AgentState::Idle),

        // Gemini CLI
        "BeforeAgent" | "BeforeTool" | "AfterTool" => Some(AgentState::Working),
        "ActionRequired" | "ToolPermission" => Some(AgentState::NeedsInput),
        "AfterAgent" | "SessionComplete" => Some(AgentState::Idle),

        _ => None,
    }
}

impl HookServer {
    pub async fn start() -> anyhow::Result<Self> {
        let (tx, _) = broadcast::channel::<HookEvent>(256);
        let tx_clone = tx.clone();

        let app = Router::new().route(
            "/hook",
            post(move |Query(params): Query<HookParams>| {
                let tx = tx_clone.clone();
                async move {
                    let pane_id = match params.pane_id.as_deref() {
                        Some(s) if !s.is_empty() => match uuid::Uuid::parse_str(s) {
                            Ok(u) => PaneId(u),
                            Err(_) => return StatusCode::BAD_REQUEST,
                        },
                        _ => return StatusCode::BAD_REQUEST,
                    };
                    let event_type_str = match &params.event_type {
                        Some(s) => s.as_str(),
                        None => return StatusCode::BAD_REQUEST,
                    };

                    // Unknown event types return 200 (forward compatible)
                    let state = match map_event_type(event_type_str) {
                        Some(s) => s,
                        None => return StatusCode::OK,
                    };

                    let event = HookEvent {
                        workspace_id: params.workspace_id.filter(|s| !s.is_empty()),
                        room_id: params.room_id.filter(|s| !s.is_empty()),
                        tab_id: params.tab_id.filter(|s| !s.is_empty()),
                        pane_id,
                        event_type: state,
                        session_id: params.session_id.filter(|s| !s.is_empty()),
                    };
                    let _ = tx.send(event);
                    StatusCode::OK
                }
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr: SocketAddr = listener.local_addr()?;

        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });

        Ok(Self {
            port: addr.port(),
            tx,
            task,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn subscribe(&self) -> broadcast::Receiver<HookEvent> {
        self.tx.subscribe()
    }
}

impl Drop for HookServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub fn hook_port_path(base_dir: &Path) -> PathBuf {
    base_dir.join("port")
}

pub fn write_hook_port_file(base_dir: &Path, port: u16) -> anyhow::Result<()> {
    std::fs::write(hook_port_path(base_dir), port.to_string())?;
    Ok(())
}

pub fn remove_hook_port_file(base_dir: &Path) -> anyhow::Result<()> {
    let path = hook_port_path(base_dir);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
