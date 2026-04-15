//! MCP internal server — real lifecycle with JSON-RPC over TCP.

use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::error::ToolError;
use crate::registry::{ToolContext, ToolRegistry};

pub struct McpServer {
    registry: Arc<ToolRegistry>,
    state: Arc<RwLock<ServerState>>,
    project_root: std::path::PathBuf,
}

#[derive(Default)]
struct ServerState {
    running: bool,
    port: Option<u16>,
    connections: usize,
}

impl McpServer {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self {
            registry,
            state: Arc::new(RwLock::new(ServerState::default())),
            project_root: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        }
    }

    pub fn with_project_root(mut self, root: std::path::PathBuf) -> Self {
        self.project_root = root;
        self
    }

    /// Start the MCP server on the given port.
    /// Spawns a background task that accepts JSON-RPC connections.
    pub async fn start(&self, port: u16) -> Result<(), ToolError> {
        let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
            .await
            .map_err(|e| {
                ToolError::McpError(format!("Failed to bind MCP server on port {port}: {e}"))
            })?;

        {
            let mut state = self.state.write().await;
            state.running = true;
            state.port = Some(port);
        }

        info!(port, "MCP server listening");

        let registry = self.registry.clone();
        let state = self.state.clone();
        let project_root = self.project_root.clone();

        tokio::spawn(async move {
            loop {
                let running = state.read().await.running;
                if !running {
                    break;
                }
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        info!(%addr, "MCP client connected");
                        state.write().await.connections += 1;
                        let reg = registry.clone();
                        let st = state.clone();
                        let root = project_root.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, reg, root).await {
                                warn!(error = %e, "MCP connection error");
                            }
                            st.write().await.connections -= 1;
                        });
                    }
                    Err(e) => {
                        error!(error = %e, "MCP accept error");
                        break;
                    }
                }
            }
            info!("MCP server stopped");
        });

        Ok(())
    }

    pub async fn stop(&self) {
        self.state.write().await.running = false;
        info!("MCP server shutdown requested");
    }

    pub async fn is_running(&self) -> bool {
        self.state.read().await.running
    }

    pub async fn port(&self) -> Option<u16> {
        self.state.read().await.port
    }

    pub async fn connection_count(&self) -> usize {
        self.state.read().await.connections
    }

    /// Handle a single JSON-RPC request (used for in-process calls).
    pub async fn handle_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, ToolError> {
        dispatch(method, params, &self.registry, &self.project_root).await
    }
}

async fn handle_connection(
    stream: TcpStream,
    registry: Arc<ToolRegistry>,
    project_root: std::path::PathBuf,
) -> Result<(), ToolError> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let req: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err = jsonrpc_error(None, -32700, &format!("Parse error: {e}"));
                let _ = writer.write_all(format!("{err}\n").as_bytes()).await;
                continue;
            }
        };

        let id = req.get("id").cloned();
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(serde_json::json!({}));

        let result = dispatch(method, params, &registry, &project_root).await;
        let response = match result {
            Ok(v) => serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": v }),
            Err(e) => jsonrpc_error(id.as_ref(), -32000, &e.to_string()),
        };

        let _ = writer.write_all(format!("{response}\n").as_bytes()).await;
    }

    Ok(())
}

async fn dispatch(
    method: &str,
    params: serde_json::Value,
    registry: &Arc<ToolRegistry>,
    project_root: &std::path::Path,
) -> Result<serde_json::Value, ToolError> {
    match method {
        "initialize" => Ok(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "vac-mcp-server", "version": env!("CARGO_PKG_VERSION") }
        })),
        "tools/list" => {
            let tools = registry.list().await;
            Ok(serde_json::json!({ "tools": tools }))
        }
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(|n| n.as_str())
                .ok_or_else(|| ToolError::McpError("Missing tool name".to_string()))?;
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::json!({}));
            // Use project_root so MCP-invoked tools have the same working context as engine path
            let context = ToolContext::new(project_root.to_path_buf());
            registry.execute(name, args, &context).await
        }
        _ => Err(ToolError::McpError(format!("Unknown method: {method}"))),
    }
}

fn jsonrpc_error(id: Option<&serde_json::Value>, code: i64, message: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}
