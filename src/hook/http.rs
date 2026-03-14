use crate::id::PaneId;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::post;
use axum::Router;
use serde::Deserialize;
use std::net::SocketAddr;
use tokio::sync::broadcast;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentState {
    Working,
    NeedsInput,
    Idle,
}

#[derive(Debug, Clone)]
pub struct HookEvent {
    pub pane_id: PaneId,
    pub event_type: AgentState,
    pub session_id: Option<String>,
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
    pane_id: Option<u64>,
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
                    let pane_id = match params.pane_id {
                        Some(id) => PaneId(id),
                        None => return StatusCode::BAD_REQUEST,
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
