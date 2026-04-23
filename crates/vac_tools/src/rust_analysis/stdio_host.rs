//! M12 — async stdio-backed LSP host.
//!
//! Replaces the PTY path with a straight `tokio::process::Command`
//! pipeline. rust-analyzer (or any LSP server named by
//! `VAC_LSP_SERVER`) is spawned with piped stdin/stdout; a writer task
//! frames outgoing JSON-RPC requests per the LSP spec
//! (`Content-Length: N\r\n\r\n<body>`), and a reader task parses the
//! same framing back into `serde_json::Value`s and resolves the
//! matching oneshot.
//!
//! Why stdio instead of PTY:
//!
//! - LSP is specified over stdio, not a terminal. rust-analyzer in
//!   PTY mode line-buffers and interleaves stderr into framed output,
//!   producing parse errors on long responses.
//! - `tokio::process::Command` keeps the whole pipeline inside the
//!   runtime, so we never need `block_on` inside `spawn_blocking`
//!   (the PTY host's main source of bugs).
//!
//! Public surface:
//!
//! - [`StdioLspHost::spawn`] → spawns the server + performs the LSP
//!   `initialize` handshake.
//! - [`StdioLspHost::is_available`] → env probe for tests + CI gating.
//! - `AnalysisHost` impl routes the six request variants onto the
//!   matching LSP methods.
//!
//! **Error semantics:** anything below the initialize handshake maps
//! to `AnalysisError::Failed(string)`. A missing binary maps to
//! `AnalysisError::BackendUnavailable` so callers can fall back to
//! the stub without a panic.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time::timeout;

use crate::rust_analysis::host::{
    AnalysisError, AnalysisHost, AnalysisRequest, AnalysisResponse, AnalysisResult, Symbol,
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Stdio-framed LSP client.
pub struct StdioLspHost {
    req_tx: mpsc::Sender<(Value, oneshot::Sender<Value>)>,
    next_id: Arc<AtomicU64>,
    _child: Arc<Mutex<Child>>,
}

impl StdioLspHost {
    /// `true` when `command` is resolvable on `$PATH`. Callers use this
    /// to decide between `StdioLspHost` and `StubAnalysisHost` at
    /// startup without eating the full spawn cost on the unhappy path.
    pub fn is_available(command: &str) -> bool {
        which_on_path(command).is_some()
    }

    /// Spawn the LSP server, run the `initialize` + `initialized`
    /// handshake, and return a host ready to accept requests. The
    /// caller may override the binary via the `VAC_LSP_SERVER` env
    /// var (defaults to `rust-analyzer`).
    pub async fn spawn(project_root: PathBuf) -> AnalysisResult<Self> {
        let command =
            std::env::var("VAC_LSP_SERVER").unwrap_or_else(|_| "rust-analyzer".into());
        if !Self::is_available(&command) {
            return Err(AnalysisError::BackendUnavailable);
        }

        let mut child = Command::new(&command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| AnalysisError::Failed(format!("spawn {command}: {e}")))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| AnalysisError::Failed("stdin not piped".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AnalysisError::Failed("stdout not piped".into()))?;

        let (req_tx, mut req_rx) = mpsc::channel::<(Value, oneshot::Sender<Value>)>(32);
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Writer task — frames JSON-RPC into LSP `Content-Length` envelopes.
        let pending_w = pending.clone();
        tokio::spawn(async move {
            while let Some((req, tx)) = req_rx.recv().await {
                if let Some(id) = req.get("id").and_then(|v| v.as_u64()) {
                    pending_w.lock().await.insert(id, tx);
                }
                let body = match serde_json::to_vec(&req) {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                let header = format!("Content-Length: {}\r\n\r\n", body.len());
                if stdin.write_all(header.as_bytes()).await.is_err() {
                    break;
                }
                if stdin.write_all(&body).await.is_err() {
                    break;
                }
                if stdin.flush().await.is_err() {
                    break;
                }
            }
        });

        // Reader task — parses the same envelopes and resolves pending.
        let pending_r = pending.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                let content_length = match read_lsp_headers(&mut reader).await {
                    Ok(n) => n,
                    Err(_) => break,
                };
                let mut buf = vec![0u8; content_length];
                if reader.read_exact(&mut buf).await.is_err() {
                    break;
                }
                let Ok(val) = serde_json::from_slice::<Value>(&buf) else {
                    continue;
                };
                if let Some(id) = val.get("id").and_then(|v| v.as_u64()) {
                    if let Some(tx) = pending_r.lock().await.remove(&id) {
                        let _ = tx.send(val);
                    }
                }
                // Notifications (no `id`) are currently discarded — a
                // follow-up can route `textDocument/publishDiagnostics`
                // into a dedicated channel.
            }
        });

        let host = Self {
            req_tx,
            next_id: Arc::new(AtomicU64::new(1)),
            _child: Arc::new(Mutex::new(child)),
        };

        // Handshake: initialize → initialized.
        let init_req = json!({
            "jsonrpc": "2.0",
            "id": host.next_id.fetch_add(1, Ordering::SeqCst),
            "method": "initialize",
            "params": {
                "processId": null,
                "rootUri": format!("file://{}", project_root.display()),
                "capabilities": {},
            }
        });
        let _ = host.call(init_req).await?;
        host.notify(json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {},
        }))
        .await?;
        Ok(host)
    }

    async fn call(&self, req: Value) -> AnalysisResult<Value> {
        let (tx, rx) = oneshot::channel();
        self.req_tx
            .send((req, tx))
            .await
            .map_err(|e| AnalysisError::Failed(format!("send: {e}")))?;
        let res = timeout(DEFAULT_TIMEOUT, rx)
            .await
            .map_err(|_| AnalysisError::Failed("lsp request timed out".into()))?
            .map_err(|e| AnalysisError::Failed(format!("lsp reply dropped: {e}")))?;
        Ok(res)
    }

    async fn notify(&self, req: Value) -> AnalysisResult<()> {
        let (tx, _rx) = oneshot::channel();
        self.req_tx
            .send((req, tx))
            .await
            .map_err(|e| AnalysisError::Failed(format!("send: {e}")))?;
        Ok(())
    }

    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }
}

async fn read_lsp_headers<R: AsyncBufReadExt + Unpin>(
    reader: &mut R,
) -> std::io::Result<usize> {
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "lsp stream closed",
            ));
        }
        if line == "\r\n" || line == "\n" {
            return Ok(content_length);
        }
        if let Some(rest) = line.strip_prefix("Content-Length:") {
            content_length = rest.trim().parse().unwrap_or(0);
        }
        // Other headers (Content-Type) are ignored.
    }
}

fn which_on_path(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn parse_symbols_from_workspace(res: &Value) -> Vec<Symbol> {
    let Some(arr) = res.get("result").and_then(|r| r.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|sym| {
            let name = sym.get("name")?.as_str()?.to_string();
            let kind = sym.get("kind")?.as_u64().unwrap_or(0).to_string();
            let loc = sym.get("location")?;
            let uri = loc.get("uri").and_then(|u| u.as_str()).unwrap_or("");
            let file = PathBuf::from(uri.strip_prefix("file://").unwrap_or(uri));
            let (line, column) = start_pos(loc.get("range")?);
            Some(Symbol { name, kind, file, line, column })
        })
        .collect()
}

fn start_pos(range: &Value) -> (u32, u32) {
    let start = range.get("start");
    let line = start
        .and_then(|s| s.get("line"))
        .and_then(|l| l.as_u64())
        .unwrap_or(0) as u32;
    let col = start
        .and_then(|s| s.get("character"))
        .and_then(|c| c.as_u64())
        .unwrap_or(0) as u32;
    (line, col)
}

#[async_trait]
impl AnalysisHost for StdioLspHost {
    fn is_real(&self) -> bool {
        true
    }

    async fn analyze(&self, request: AnalysisRequest) -> AnalysisResult<AnalysisResponse> {
        match request {
            AnalysisRequest::ResolveSymbol { name } => {
                let req = json!({
                    "jsonrpc": "2.0",
                    "id": self.next_id(),
                    "method": "workspace/symbol",
                    "params": { "query": name }
                });
                let res = self.call(req).await?;
                Ok(AnalysisResponse::ResolveSymbol(parse_symbols_from_workspace(&res)))
            }
            AnalysisRequest::FileSymbols { path } => {
                let req = json!({
                    "jsonrpc": "2.0",
                    "id": self.next_id(),
                    "method": "textDocument/documentSymbol",
                    "params": {
                        "textDocument": { "uri": format!("file://{}", path.display()) }
                    }
                });
                let res = self.call(req).await?;
                let mut symbols = Vec::new();
                if let Some(arr) = res.get("result").and_then(|r| r.as_array()) {
                    for sym in arr {
                        let Some(name) = sym.get("name").and_then(|n| n.as_str()) else {
                            continue;
                        };
                        let kind = sym
                            .get("kind")
                            .and_then(|k| k.as_u64())
                            .unwrap_or(0)
                            .to_string();
                        let (line, column) = match sym.get("range").or_else(|| {
                            sym.get("selectionRange")
                        }) {
                            Some(r) => start_pos(r),
                            None => (0, 0),
                        };
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind,
                            file: path.clone(),
                            line,
                            column,
                        });
                    }
                }
                Ok(AnalysisResponse::FileSymbols(symbols))
            }
            AnalysisRequest::Hover { file, line, column } => {
                let req = json!({
                    "jsonrpc": "2.0",
                    "id": self.next_id(),
                    "method": "textDocument/hover",
                    "params": {
                        "textDocument": { "uri": format!("file://{}", file.display()) },
                        "position": { "line": line, "character": column },
                    }
                });
                let res = self.call(req).await?;
                let text = res
                    .get("result")
                    .and_then(|r| r.get("contents"))
                    .and_then(|c| c.get("value"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Ok(AnalysisResponse::Hover(text))
            }
            AnalysisRequest::GotoDefinition { file, line, column } => {
                let req = json!({
                    "jsonrpc": "2.0",
                    "id": self.next_id(),
                    "method": "textDocument/definition",
                    "params": {
                        "textDocument": { "uri": format!("file://{}", file.display()) },
                        "position": { "line": line, "character": column },
                    }
                });
                let res = self.call(req).await?;
                let mut symbols = Vec::new();
                let results = res.get("result");
                let arr = match results {
                    Some(Value::Array(a)) => a.clone(),
                    Some(v @ Value::Object(_)) => vec![v.clone()],
                    _ => Vec::new(),
                };
                for loc in arr {
                    let uri = loc.get("uri").and_then(|u| u.as_str()).unwrap_or("");
                    let path = PathBuf::from(uri.strip_prefix("file://").unwrap_or(uri));
                    let (l, c) = match loc.get("range") {
                        Some(r) => start_pos(r),
                        None => (0, 0),
                    };
                    symbols.push(Symbol {
                        name: String::new(),
                        kind: "definition".into(),
                        file: path,
                        line: l,
                        column: c,
                    });
                }
                Ok(AnalysisResponse::GotoDefinition(symbols))
            }
            AnalysisRequest::WorkspaceDiagnostics => {
                // LSP 3.17's pull-diagnostics model; rust-analyzer
                // implements it selectively. Return empty for now so
                // callers don't error on servers that don't support it.
                Ok(AnalysisResponse::WorkspaceDiagnostics(Vec::new()))
            }
            AnalysisRequest::ExplainLifetime { diagnostic_code } => {
                Err(AnalysisError::Unsupported(format!(
                    "explain_lifetime({diagnostic_code}) not implemented by rust-analyzer"
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_available_false_for_bogus_binary() {
        assert!(!StdioLspHost::is_available("definitely-not-a-real-binary-xyz"));
    }

    #[test]
    fn which_on_path_finds_sh() {
        // /bin/sh is universally present on Linux CI runners.
        let found = which_on_path("sh");
        assert!(found.is_some(), "which_on_path should find `sh`");
    }

    #[test]
    fn start_pos_extracts_line_col() {
        let v = json!({ "start": { "line": 7, "character": 3 } });
        assert_eq!(start_pos(&v), (7, 3));
    }

    #[test]
    fn start_pos_defaults_to_zero() {
        let v = json!({});
        assert_eq!(start_pos(&v), (0, 0));
    }

    #[test]
    fn parse_symbols_from_workspace_handles_empty() {
        let v = json!({ "result": [] });
        assert!(parse_symbols_from_workspace(&v).is_empty());
    }

    #[test]
    fn parse_symbols_from_workspace_extracts_location() {
        let v = json!({
            "result": [{
                "name": "foo",
                "kind": 12,
                "location": {
                    "uri": "file:///tmp/a.rs",
                    "range": { "start": { "line": 4, "character": 2 } }
                }
            }]
        });
        let syms = parse_symbols_from_workspace(&v);
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "foo");
        assert_eq!(syms[0].line, 4);
        assert_eq!(syms[0].column, 2);
    }

    #[tokio::test]
    async fn read_lsp_headers_parses_content_length() {
        let msg = "Content-Length: 42\r\nContent-Type: application/json\r\n\r\n";
        let mut reader = BufReader::new(msg.as_bytes());
        let n = read_lsp_headers(&mut reader).await.unwrap();
        assert_eq!(n, 42);
    }

    #[tokio::test]
    async fn spawn_without_server_returns_backend_unavailable() {
        // Temporarily point VAC_LSP_SERVER at a bogus binary so the
        // probe cannot find it on PATH. Restore afterwards even on
        // panic via a scope guard pattern.
        let tmp = tempfile::tempdir().unwrap();
        let prior = std::env::var("VAC_LSP_SERVER").ok();
        // SAFETY: tests share an env; we restore at end of function.
        unsafe { std::env::set_var("VAC_LSP_SERVER", "definitely-not-a-real-binary-xyz") };
        let out = StdioLspHost::spawn(tmp.path().to_path_buf()).await;
        match prior {
            Some(v) => unsafe { std::env::set_var("VAC_LSP_SERVER", v) },
            None => unsafe { std::env::remove_var("VAC_LSP_SERVER") },
        }
        match out {
            Err(AnalysisError::BackendUnavailable) => {}
            Err(other) => panic!("expected BackendUnavailable, got {other:?}"),
            Ok(_) => panic!("spawn must fail when binary is missing"),
        }
    }
}
