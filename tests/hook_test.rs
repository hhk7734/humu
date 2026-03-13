use humu::hook::server::HookServer;
use std::io::Write;
use std::os::unix::net::UnixStream;
use tempfile::TempDir;

#[tokio::test]
async fn test_hook_server_receives_event() {
    let dir = TempDir::new().unwrap();
    let sock_path = dir.path().join("humu.sock");

    let _server = HookServer::new(&sock_path).await.unwrap();
    let mut rx = _server.subscribe();

    let mut stream = UnixStream::connect(&sock_path).unwrap();
    let event = r#"{"workspace":"humu","room":"feat/auth","hook_type":"PreToolUse","tool":"Edit"}"#;
    writeln!(stream, "{event}").unwrap();
    drop(stream);

    let received = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        rx.recv(),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(received.workspace, "humu");
    assert_eq!(received.room, "feat/auth");
    assert_eq!(received.hook_type, "PreToolUse");
}
