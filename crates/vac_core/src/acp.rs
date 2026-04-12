//! ACP (Agent Communication Protocol) server — production-grade session-aware agent protocol.
//!
//! Protocol: newline-delimited JSON over TCP.
//! Supports: session create/resume, task streaming, diagnostics, approval flow.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

// ── Session ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpSession {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub task_history: Vec<String>,
}

impl AcpSession {
    pub fn new() -> Self {
        let now = Utc::now();
        Self { id: Uuid::new_v4(), created_at: now, last_active: now, task_history: vec![] }
    }

    pub fn touch(&mut self) { self.last_active = Utc::now(); }

    pub fn save(&self, sessions_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(sessions_dir)?;
        let path = sessions_dir.join(format!("{}.acp.json", self.id));
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default())
    }

    pub fn load(sessions_dir: &Path, id: Uuid) -> Option<Self> {
        let path = sessions_dir.join(format!("{id}.acp.json"));
        std::fs::read_to_string(path).ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    }
}

impl Default for AcpSession {
    fn default() -> Self { Self::new() }
}

// ── Server ────────────────────────────────────────────────────────────────────

pub struct AcpServer {
    state: Arc<RwLock<AcpState>>,
    sessions: Arc<RwLock<HashMap<Uuid, AcpSession>>>,
    sessions_dir: PathBuf,
}

#[derive(Default)]
struct AcpState {
    running: bool,
    port: Option<u16>,
    connections: usize,
}

pub type TaskHandler = Arc<
    dyn Fn(String, Option<Uuid>) -> tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>
        + Send + Sync,
>;

impl AcpServer {
    pub fn new(project_root: &Path) -> Self {
        Self {
            state: Arc::new(RwLock::new(AcpState::default())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            sessions_dir: project_root.join(".vac/sessions"),
        }
    }

    pub async fn start(&self, port: u16, task_handler: TaskHandler) -> Result<(), String> {
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
        let sessions = self.sessions.clone();
        let sessions_dir = self.sessions_dir.clone();

        tokio::spawn(async move {
            loop {
                if !state.read().await.running { break; }
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        info!(%addr, "ACP editor connected");
                        state.write().await.connections += 1;
                        let st = state.clone();
                        let sess = sessions.clone();
                        let sdir = sessions_dir.clone();
                        let handler = task_handler.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_acp_connection(stream, handler, sess, sdir).await {
                                warn!(error = %e, "ACP connection error");
                            }
                            st.write().await.connections -= 1;
                        });
                    }
                    Err(e) => { error!(error = %e, "ACP accept error"); break; }
                }
            }
        });

        Ok(())
    }

    pub async fn stop(&self) { self.state.write().await.running = false; }
    pub async fn is_running(&self) -> bool { self.state.read().await.running }
    pub async fn port(&self) -> Option<u16> { self.state.read().await.port }
    pub async fn connection_count(&self) -> usize { self.state.read().await.connections }
}

// ── Connection handler ────────────────────────────────────────────────────────

async fn handle_acp_connection(
    stream: TcpStream,
    task_handler: TaskHandler,
    sessions: Arc<RwLock<HashMap<Uuid, AcpSession>>>,
    sessions_dir: PathBuf,
) -> Result<(), String> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() { continue; }

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

        let response = match method {
            "ping" => serde_json::json!({ "id": id, "event": "pong" }),

            "session/create" => {
                let session = AcpSession::new();
                let sid = session.id;
                let _ = session.save(&sessions_dir);
                sessions.write().await.insert(sid, session);
                serde_json::json!({ "id": id, "event": "session_created", "data": { "session_id": sid } })
            }

            "session/resume" => {
                let sid_str = req.get("params").and_then(|p| p.get("session_id")).and_then(|s| s.as_str()).unwrap_or("");
                if let Ok(sid) = Uuid::parse_str(sid_str) {
                    if let Some(session) = AcpSession::load(&sessions_dir, sid) {
                        let history = session.task_history.clone();
                        sessions.write().await.insert(sid, session);
                        serde_json::json!({ "id": id, "event": "session_resumed", "data": { "session_id": sid, "history": history } })
                    } else {
                        serde_json::json!({ "id": id, "error": "Session not found" })
                    }
                } else {
                    serde_json::json!({ "id": id, "error": "Invalid session_id" })
                }
            }

            "run_task" => {
                let task = req.get("params").and_then(|p| p.get("task")).and_then(|t| t.as_str()).unwrap_or("").to_string();
                let session_id = req.get("params")
                    .and_then(|p| p.get("session_id"))
                    .and_then(|s| s.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok());

                // Update session history
                if let Some(sid) = session_id {
                    if let Some(sess) = sessions.write().await.get_mut(&sid) {
                        sess.task_history.push(task.clone());
                        sess.touch();
                        let _ = sess.save(&sessions_dir);
                    }
                }

                let ack = serde_json::json!({ "id": id, "event": "task_started", "data": { "task": task } });
                let _ = writer.write_all(format!("{ack}\n").as_bytes()).await;

                let mut rx = task_handler(task, session_id);
                while let Some(event) = rx.recv().await {
                    let msg = serde_json::json!({ "id": id, "event": "update", "data": event });
                    if writer.write_all(format!("{msg}\n").as_bytes()).await.is_err() { break; }
                }

                serde_json::json!({ "id": id, "event": "task_done" })
            }

            "get_status" => serde_json::json!({
                "id": id,
                "event": "status",
                "data": {
                    "protocol": "acp/1.0",
                    "server": "vac-acp-server",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),

            _ => serde_json::json!({ "id": id, "error": format!("Unknown method: {method}") }),
        };

        let _ = writer.write_all(format!("{response}\n").as_bytes()).await;
    }

    Ok(())
}

// ── RuntimeUpdate → ACP event ─────────────────────────────────────────────────

/// Convert a RuntimeUpdate to an ACP event JSON payload.
/// Returns None for internal-only updates that editors don't need.
pub fn runtime_update_to_acp_event(update: &crate::engine::RuntimeUpdate) -> Option<serde_json::Value> {
    use crate::engine::RuntimeUpdate::*;
    match update {
        Status(msg) => Some(serde_json::json!({ "event": "status", "data": { "message": msg } })),
        AssistantChunk(text) => Some(serde_json::json!({ "event": "assistant_chunk", "data": { "text": text } })),
        ToolCall { id, name, arguments } => Some(serde_json::json!({
            "event": "tool_call", "data": { "id": id, "name": name, "args": arguments }
        })),
        ToolResult { id, name, content, success } => Some(serde_json::json!({
            "event": "tool_result", "data": { "id": id, "name": name, "success": success, "content": content }
        })),
        ValidationResult { score, issues } => Some(serde_json::json!({
            "event": "validation", "data": { "score": score, "issues": issues }
        })),
        LspDiagnostics(snap) => Some(serde_json::json!({
            "event": "lsp_diagnostics", "data": { "errors": snap.total_errors, "warnings": snap.total_warnings }
        })),
        Completed(result) => Some(serde_json::json!({
            "event": "completed", "data": { "summary": result.summary }
        })),
        Failed(reason) => Some(serde_json::json!({
            "event": "failed", "data": { "reason": reason }
        })),
        _ => None,
    }
}
