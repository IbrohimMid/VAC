//! ACP (Agent Communication Protocol) server — editor-facing session endpoint.
//!
//! Provides a simple JSON-over-TCP server that editors (Zed, VS Code, Helix)
//! can connect to for streaming task execution and session persistence.
//!
//! Protocol: newline-delimited JSON (same as MCP server).
//! Each request: { "id": N, "method": "...", "params": {...} }
//! Each response/event: { "id": N, "event": "...", "data": {...} }

use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

pub struct AcpServer {
    state: Arc<RwLock<AcpState>>,
}

#[derive(Default)]
struct AcpState {
    running: bool,
    port: Option<u16>,
    connections: usize,
}

/// Callback type for handling task execution requests from editor.
/// Returns a stream of JSON events.
pub type TaskHandler = Arc<
    dyn Fn(String) -> tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>
        + Send
        + Sync,
>;

impl AcpServer {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(AcpState::default())),
        }
    }

    /// Start the ACP server on the given port.
    /// `task_handler` is called for each "run_task" request from an editor.
    pub async fn start(
        &self,
        port: u16,
        task_handler: TaskHandler,
    ) -> Result<(), String> {
        let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
            .await
            .map_err(|e| format!("Failed to bind ACP server on port {port}: {e}"))?;

        {
            let mut s = self.state.write().await;
            s.running = true;
            s.port = Some(port);
        }

        info!(port, "ACP server listening");

        let state = self.state.clone();
        tokio::spawn(async move {
            loop {
                if !state.read().await.running {
                    break;
                }
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        info!(%addr, "ACP editor connected");
                        state.write().await.connections += 1;
                        let st = state.clone();
                        let handler = task_handler.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_acp_connection(stream, handler).await {
                                warn!(error = %e, "ACP connection error");
                            }
                            st.write().await.connections -= 1;
                        });
                    }
                    Err(e) => {
                        error!(error = %e, "ACP accept error");
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn stop(&self) {
        self.state.write().await.running = false;
    }

    pub async fn is_running(&self) -> bool {
        self.state.read().await.running
    }

    pub async fn port(&self) -> Option<u16> {
        self.state.read().await.port
    }
}

impl Default for AcpServer {
    fn default() -> Self {
        Self::new()
    }
}

async fn handle_acp_connection(
    stream: TcpStream,
    task_handler: TaskHandler,
) -> Result<(), String> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let req: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err = serde_json::json!({ "error": format!("Parse error: {e}") });
                let _ = writer.write_all(format!("{err}\n").as_bytes()).await;
                continue;
            }
        };

        let id = req.get("id").cloned().unwrap_or(serde_json::json!(null));
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");

        match method {
            "ping" => {
                let resp = serde_json::json!({ "id": id, "event": "pong" });
                let _ = writer.write_all(format!("{resp}\n").as_bytes()).await;
            }
            "run_task" => {
                let task = req
                    .get("params")
                    .and_then(|p| p.get("task"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();

                // Acknowledge
                let ack = serde_json::json!({ "id": id, "event": "task_started", "data": { "task": task } });
                let _ = writer.write_all(format!("{ack}\n").as_bytes()).await;

                // Stream events from task handler
                let mut rx = task_handler(task);
                while let Some(event) = rx.recv().await {
                    let msg = serde_json::json!({ "id": id, "event": "update", "data": event });
                    if writer.write_all(format!("{msg}\n").as_bytes()).await.is_err() {
                        break;
                    }
                }

                let done = serde_json::json!({ "id": id, "event": "task_done" });
                let _ = writer.write_all(format!("{done}\n").as_bytes()).await;
            }
            "get_status" => {
                let resp = serde_json::json!({
                    "id": id,
                    "event": "status",
                    "data": { "protocol": "acp/1.0", "server": "vac-acp-server" }
                });
                let _ = writer.write_all(format!("{resp}\n").as_bytes()).await;
            }
            _ => {
                let err = serde_json::json!({ "id": id, "error": format!("Unknown method: {method}") });
                let _ = writer.write_all(format!("{err}\n").as_bytes()).await;
            }
        }
    }

    Ok(())
}
