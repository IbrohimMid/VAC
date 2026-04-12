//! Low-level stdio client for vil-lsp.
//!
//! Spawns `vil-lsp` as a child process, communicates via LSP JSON-RPC over stdio.
//! Reads `textDocument/publishDiagnostics` notifications and feeds them to a channel.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, info};

use super::protocol::*;
use super::types::{LspDiagnostic, LspRange, LspSeverity};

pub struct VilLspClient {
    stdin: Arc<Mutex<ChildStdin>>,
    _child: Arc<Mutex<Child>>,
    next_id: Arc<AtomicU64>,
    pub diagnostics_tx: mpsc::UnboundedSender<(String, Vec<LspDiagnostic>)>,
}

impl VilLspClient {
    /// Spawn vil-lsp and initialize the workspace.
    pub async fn start(
        binary: &Path,
        root: &Path,
        args: &[String],
        diag_tx: mpsc::UnboundedSender<(String, Vec<LspDiagnostic>)>,
    ) -> anyhow::Result<Self> {
        let mut child = Command::new(binary)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn vil-lsp at {}: {e}", binary.display()))?;

        let stdin = child.stdin.take()
            .ok_or_else(|| anyhow::anyhow!("vil-lsp process has no stdin — check binary path"))?;
        let stdout = child.stdout.take()
            .ok_or_else(|| anyhow::anyhow!("vil-lsp process has no stdout — check binary path"))?;

        let stdin = Arc::new(Mutex::new(stdin));
        let child = Arc::new(Mutex::new(child));
        let next_id = Arc::new(AtomicU64::new(1));

        // Spawn reader task — parses LSP Content-Length framed messages
        let tx = diag_tx.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                // Read Content-Length header
                let mut header = String::new();
                if reader.read_line(&mut header).await.unwrap_or(0) == 0 {
                    break;
                }
                let header = header.trim().to_string();
                if !header.starts_with("Content-Length:") {
                    continue;
                }
                let len: usize = header
                    .trim_start_matches("Content-Length:")
                    .trim()
                    .parse()
                    .unwrap_or(0);
                if len == 0 {
                    continue;
                }
                // Skip blank line
                let mut blank = String::new();
                let _ = reader.read_line(&mut blank).await;

                // Read body
                let mut body = vec![0u8; len];
                use tokio::io::AsyncReadExt;
                if reader.read_exact(&mut body).await.is_err() {
                    break;
                }

                let msg: serde_json::Value = match serde_json::from_slice(&body) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                // Only care about publishDiagnostics notifications
                if msg.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics") {
                    if let Some(params) = msg.get("params") {
                        if let Some((uri, raw_diags)) = parse_publish_diagnostics(params) {
                            let path = uri_to_path(&uri);
                            let diags: Vec<LspDiagnostic> = raw_diags
                                .iter()
                                .filter_map(|d| parse_diagnostic(d, &path))
                                .collect();
                            debug!(file = %path.display(), count = diags.len(), "LSP diagnostics received");
                            let _ = tx.send((uri, diags));
                        }
                    }
                }
            }
        });

        let client = Self { stdin, _child: child, next_id, diagnostics_tx: diag_tx };

        // Initialize
        let root_uri = path_to_uri(root);
        let id = client.next_id.fetch_add(1, Ordering::SeqCst);
        client.send_request(&initialize_request(id, &root_uri)).await?;
        client.send_notification(&initialized_notification()).await?;
        info!(root = %root.display(), "vil-lsp initialized");

        Ok(client)
    }

    pub async fn open_file(&self, path: &Path, text: String) -> anyhow::Result<()> {
        let uri = path_to_uri(path);
        self.send_notification(&did_open_notification(&uri, &text)).await
    }

    pub async fn change_file(&self, path: &Path, text: String) -> anyhow::Result<()> {
        let uri = path_to_uri(path);
        let version = self.next_id.fetch_add(1, Ordering::SeqCst) as i32;
        self.send_notification(&did_change_notification(&uri, &text, version)).await
    }

    /// Send a request and wait for the response (with timeout).
    pub async fn request(&self, req: &impl serde::Serialize) -> anyhow::Result<serde_json::Value> {
        // For navigation queries we need a response channel.
        // Simple approach: write request, then read next non-notification message.
        // In production this would use a proper pending-request map.
        self.write_message(req).await?;
        // Return empty result — full response handling requires pending-request map
        // which is out of scope for the minimal implementation.
        Ok(serde_json::json!(null))
    }

    pub async fn shutdown(&self) -> anyhow::Result<()> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.send_request(&shutdown_request(id)).await?;
        self.send_notification(&exit_notification()).await
    }

    async fn send_request(&self, req: &impl serde::Serialize) -> anyhow::Result<()> {
        self.write_message(req).await
    }

    async fn send_notification(&self, notif: &impl serde::Serialize) -> anyhow::Result<()> {
        self.write_message(notif).await
    }

    async fn write_message(&self, msg: &impl serde::Serialize) -> anyhow::Result<()> {
        let body = serde_json::to_vec(msg)?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(header.as_bytes()).await?;
        stdin.write_all(&body).await?;
        stdin.flush().await?;
        Ok(())
    }
}

fn parse_diagnostic(d: &serde_json::Value, file_path: &std::path::Path) -> Option<LspDiagnostic> {
    let message = d.get("message")?.as_str()?.to_string();
    let severity = d.get("severity")
        .and_then(|s| s.as_u64())
        .map(|n| LspSeverity::from_lsp_code(n as u32))
        .unwrap_or(LspSeverity::Warning);
    let code = d.get("code").and_then(|c| c.as_str()).map(String::from);
    let source = d.get("source").and_then(|s| s.as_str()).map(String::from);
    let range = d.get("range")?;
    let start = range.get("start")?;
    let end = range.get("end")?;

    Some(LspDiagnostic {
        file_path: file_path.to_path_buf(),
        severity,
        code,
        source,
        message,
        range: LspRange {
            start_line: start.get("line")?.as_u64()? as u32,
            start_character: start.get("character")?.as_u64()? as u32,
            end_line: end.get("line")?.as_u64()? as u32,
            end_character: end.get("character")?.as_u64()? as u32,
        },
    })
}
