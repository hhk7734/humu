use crate::id::PaneId;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::post;
use axum::Router;
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::Path;
use tokio::sync::broadcast;

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
    std::fs::write(&notify_path, r#"#!/bin/bash
command -v curl &>/dev/null || exit 0
INPUT=$(cat)
EVENT=$(echo "$INPUT" | grep -oE '"hook_event_name"\s*:\s*"[^"]*"' | grep -oE '"[^"]*"$' | tr -d '"')
SESSION=$(echo "$INPUT" | grep -oE '"session_id"\s*:\s*"[^"]*"' | head -1 | grep -oE '"[^"]*"$' | tr -d '"')
[ -z "$HUMU_PORT" ] && exit 0
curl -s --connect-timeout 1 --max-time 2 -X POST \
  "http://127.0.0.1:${HUMU_PORT}/hook?workspaceId=${HUMU_WORKSPACE_ID}&roomId=${HUMU_ROOM_ID}&tabId=${HUMU_TAB_ID}&paneId=${HUMU_PANE_ID}&eventType=${EVENT}&sessionId=${SESSION}" \
  >/dev/null 2>&1 || true
"#)?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&notify_path, std::fs::Permissions::from_mode(0o755))?;
    }

    // Generate claude-settings.json
    let settings_path = hooks_dir.join("claude-settings.json");
    let notify_abs = notify_path.to_string_lossy();
    let settings = serde_json::json!({
        "hooks": {
            "UserPromptSubmit": [{"hooks": [{"type": "command", "command": notify_abs}]}],
            "Stop": [{"hooks": [{"type": "command", "command": notify_abs}]}],
            "PostToolUse": [{"matcher": "*", "hooks": [{"type": "command", "command": notify_abs}]}],
            "PostToolUseFailure": [{"matcher": "*", "hooks": [{"type": "command", "command": notify_abs}]}],
            "PermissionRequest": [{"matcher": "*", "hooks": [{"type": "command", "command": notify_abs}]}]
        }
    });
    std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;

    Ok(())
}

pub struct HookServer {
    port: u16,
    tx: broadcast::Sender<HookEvent>,
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
        "UserPromptSubmit" | "PostToolUse" | "PostToolUseFailure" => Some(AgentState::Working),
        "PermissionRequest" => Some(AgentState::NeedsInput),
        "Stop" => Some(AgentState::Idle),
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
                        Some(s) if !s.is_empty() => {
                            match uuid::Uuid::parse_str(s) {
                                Ok(u) => PaneId(u),
                                Err(_) => return StatusCode::BAD_REQUEST,
                            }
                        }
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

        tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });

        Ok(Self {
            port: addr.port(),
            tx,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn subscribe(&self) -> broadcast::Receiver<HookEvent> {
        self.tx.subscribe()
    }
}
