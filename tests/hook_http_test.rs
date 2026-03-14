use humu::hook::http::{AgentState, HookServer};
use humu::id::PaneId;

#[tokio::test]
async fn hook_server_starts_and_returns_port() {
    let server = HookServer::start().await.unwrap();
    let port = server.port();
    assert!(port > 0);
}

#[tokio::test]
async fn hook_event_updates_state() {
    let server = HookServer::start().await.unwrap();
    let port = server.port();
    let mut rx = server.subscribe();

    // Send a PostToolUse event
    let url = format!(
        "http://127.0.0.1:{port}/hook?workspaceId=550e8400-e29b-41d4-a716-446655440000&roomId=660e8400-e29b-41d4-a716-446655440001&tabId=1&paneId=42&eventType=PostToolUse&sessionId=sess123"
    );
    let resp = reqwest::Client::new().post(&url).send().await.unwrap();
    assert_eq!(resp.status(), 200);

    let event = rx.recv().await.unwrap();
    assert_eq!(event.pane_id, PaneId(42));
    assert_eq!(event.event_type, AgentState::Working);
    assert_eq!(event.session_id, Some("sess123".to_string()));
}

#[tokio::test]
async fn unknown_event_type_returns_200() {
    let server = HookServer::start().await.unwrap();
    let port = server.port();

    let url = format!(
        "http://127.0.0.1:{port}/hook?workspaceId=abc&roomId=def&tabId=1&paneId=1&eventType=FutureEvent"
    );
    let resp = reqwest::Client::new().post(&url).send().await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn missing_params_returns_400() {
    let server = HookServer::start().await.unwrap();
    let port = server.port();

    let url = format!("http://127.0.0.1:{port}/hook?paneId=1");
    let resp = reqwest::Client::new().post(&url).send().await.unwrap();
    assert_eq!(resp.status(), 400);
}
