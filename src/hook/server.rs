use anyhow::Result;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tokio::io::AsyncBufReadExt;
use tokio::net::UnixListener;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Deserialize)]
pub struct HookEvent {
    pub workspace: String,
    pub room: String,
    pub hook_type: String,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

pub struct HookServer {
    sock_path: PathBuf,
    tx: broadcast::Sender<HookEvent>,
}

impl HookServer {
    pub async fn new(sock_path: &Path) -> Result<Self> {
        if sock_path.exists() {
            std::fs::remove_file(sock_path)?;
        }
        if let Some(parent) = sock_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(sock_path)?;
        let (tx, _) = broadcast::channel(256);
        let tx_clone = tx.clone();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let tx = tx_clone.clone();
                        tokio::spawn(async move {
                            let reader = tokio::io::BufReader::new(stream);
                            let mut lines = reader.lines();
                            while let Ok(Some(line)) = lines.next_line().await {
                                if let Ok(event) = serde_json::from_str::<HookEvent>(&line) {
                                    let _ = tx.send(event);
                                }
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("hook server accept error: {e}");
                    }
                }
            }
        });

        Ok(Self {
            sock_path: sock_path.to_path_buf(),
            tx,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<HookEvent> {
        self.tx.subscribe()
    }
}

impl Drop for HookServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.sock_path);
    }
}
