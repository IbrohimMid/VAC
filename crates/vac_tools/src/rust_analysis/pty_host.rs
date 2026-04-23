use crate::rust_analysis::host::{AnalysisError, AnalysisHost, AnalysisRequest, AnalysisResponse, AnalysisResult, Symbol};
use async_trait::async_trait;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use tokio::sync::{mpsc, oneshot};

#[derive(Clone)]
pub struct PortablePtyHost {
    req_tx: mpsc::Sender<(Value, oneshot::Sender<Value>)>,
    next_id: Arc<AtomicU64>,
}

impl PortablePtyHost {
    pub fn new() -> anyhow::Result<Self> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new("rust-analyzer");
        // Don't set envs that break LSP in PTY if possible
        
        let mut child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let reader = pair.master.try_clone_reader()?;
        let mut writer = pair.master.take_writer()?;

        let (req_tx, mut req_rx) = mpsc::channel::<(Value, oneshot::Sender<Value>)>(32);
        let pending: Arc<tokio::sync::Mutex<HashMap<u64, oneshot::Sender<Value>>>> = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
        let pending_read = pending.clone();

        // Write task
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();
            while let Some((mut req, reply_tx)) = rt.block_on(req_rx.recv()) {
                let id = req["id"].as_u64().unwrap();
                rt.block_on(async {
                    pending.lock().await.insert(id, reply_tx);
                });

                let msg = serde_json::to_string(&req).unwrap();
                let payload = format!("Content-Length: {}\r\n\r\n{}", msg.len(), msg);
                if writer.write_all(payload.as_bytes()).is_err() {
                    break;
                }
                writer.flush().ok();
            }
        });

        // Read task
        tokio::task::spawn_blocking(move || {
            let mut buf_reader = BufReader::new(reader);
            let rt = tokio::runtime::Handle::current();
            loop {
                let mut line = String::new();
                if buf_reader.read_line(&mut line).is_err() || line.is_empty() {
                    break; // EOF
                }
                if line.starts_with("Content-Length: ") {
                    let len_str = line.trim_start_matches("Content-Length: ").trim();
                    if let Ok(len) = len_str.parse::<usize>() {
                        // read empty line
                        buf_reader.read_line(&mut line).ok();
                        let mut buf = vec![0u8; len];
                        if buf_reader.read_exact(&mut buf).is_err() {
                            break;
                        }
                        if let Ok(val) = serde_json::from_slice::<Value>(&buf) {
                            if let Some(id) = val.get("id").and_then(|i| i.as_u64()) {
                                rt.block_on(async {
                                    if let Some(tx) = pending_read.lock().await.remove(&id) {
                                        let _ = tx.send(val);
                                    }
                                });
                            }
                        }
                    }
                }
            }
        });

        let host = Self {
            req_tx,
            next_id: Arc::new(AtomicU64::new(1)),
        };

        // Send initialize
        let id = host.next_id.fetch_add(1, Ordering::SeqCst);
        let init_req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "processId": null,
                "rootUri": null,
                "capabilities": {}
            }
        });
        // We do this sync-ish
        let host_clone = host.clone();
        tokio::spawn(async move {
            if let Ok(res) = host_clone.call(init_req).await {
                // Send initialized
                let notif = json!({
                    "jsonrpc": "2.0",
                    "method": "initialized",
                    "params": {}
                });
                let _ = host_clone.notify(notif).await;
            }
        });

        Ok(host)
    }

    async fn call(&self, mut req: Value) -> anyhow::Result<Value> {
        let (tx, rx) = oneshot::channel();
        if !req.as_object().unwrap().contains_key("id") {
            req["id"] = json!(self.next_id.fetch_add(1, Ordering::SeqCst));
        }
        self.req_tx.send((req, tx)).await?;
        Ok(rx.await?)
    }

    async fn notify(&self, req: Value) -> anyhow::Result<()> {
        // Just send with a dummy channel that we drop
        let (tx, _) = oneshot::channel();
        self.req_tx.send((req, tx)).await?;
        Ok(())
    }
}

#[async_trait]
impl AnalysisHost for PortablePtyHost {
    async fn analyze(&self, request: AnalysisRequest) -> AnalysisResult<AnalysisResponse> {
        match request {
            AnalysisRequest::ResolveSymbol { name } => {
                let req = json!({
                    "jsonrpc": "2.0",
                    "method": "workspace/symbol",
                    "params": {
                        "query": name
                    }
                });
                let res = self.call(req).await.map_err(|e| AnalysisError::Failed(e.to_string()))?;
                let mut symbols = Vec::new();
                if let Some(arr) = res.get("result").and_then(|r| r.as_array()) {
                    for sym in arr {
                        if let (Some(n), Some(kind), Some(loc)) = (
                            sym.get("name").and_then(|n| n.as_str()),
                            sym.get("kind").and_then(|k| k.as_u64()),
                            sym.get("location")
                        ) {
                            let uri = loc.get("uri").and_then(|u| u.as_str()).unwrap_or("");
                            let path = uri.strip_prefix("file://").unwrap_or(uri);
                            let line = loc.get("range").and_then(|r| r.get("start")).and_then(|s| s.get("line")).and_then(|l| l.as_u64()).unwrap_or(0);
                            let col = loc.get("range").and_then(|r| r.get("start")).and_then(|s| s.get("character")).and_then(|c| c.as_u64()).unwrap_or(0);
                            symbols.push(Symbol {
                                name: n.to_string(),
                                kind: kind.to_string(),
                                file: PathBuf::from(path),
                                line: line as u32,
                                column: col as u32,
                            });
                        }
                    }
                }
                Ok(AnalysisResponse::ResolveSymbol(symbols))
            }
            AnalysisRequest::FileSymbols { path } => {
                let req = json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/documentSymbol",
                    "params": {
                        "textDocument": {
                            "uri": format!("file://{}", path.display())
                        }
                    }
                });
                let res = self.call(req).await.map_err(|e| AnalysisError::Failed(e.to_string()))?;
                let mut symbols = Vec::new();
                if let Some(arr) = res.get("result").and_then(|r| r.as_array()) {
                    for sym in arr {
                        if let (Some(n), Some(kind), Some(range)) = (
                            sym.get("name").and_then(|n| n.as_str()),
                            sym.get("kind").and_then(|k| k.as_u64()),
                            sym.get("range")
                        ) {
                            let line = range.get("start").and_then(|s| s.get("line")).and_then(|l| l.as_u64()).unwrap_or(0);
                            let col = range.get("start").and_then(|s| s.get("character")).and_then(|c| c.as_u64()).unwrap_or(0);
                            symbols.push(Symbol {
                                name: n.to_string(),
                                kind: kind.to_string(),
                                file: path.clone(),
                                line: line as u32,
                                column: col as u32,
                            });
                        }
                    }
                }
                Ok(AnalysisResponse::FileSymbols(symbols))
            }
            AnalysisRequest::WorkspaceDiagnostics => {
                // Return empty diagnostics or fetch from rust-analyzer
                Ok(AnalysisResponse::WorkspaceDiagnostics(Vec::new()))
            }
            AnalysisRequest::Hover { file, line, column } => {
                let req = json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/hover",
                    "params": {
                        "textDocument": {
                            "uri": format!("file://{}", file.display())
                        },
                        "position": {
                            "line": line,
                            "character": column
                        }
                    }
                });
                let res = self.call(req).await.map_err(|e| AnalysisError::Failed(e.to_string()))?;
                let content = res.get("result")
                    .and_then(|r| r.get("contents"))
                    .and_then(|c| c.get("value"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Ok(AnalysisResponse::Hover(content))
            }
            AnalysisRequest::GotoDefinition { file, line, column } => {
                let req = json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/definition",
                    "params": {
                        "textDocument": {
                            "uri": format!("file://{}", file.display())
                        },
                        "position": {
                            "line": line,
                            "character": column
                        }
                    }
                });
                let res = self.call(req).await.map_err(|e| AnalysisError::Failed(e.to_string()))?;
                let mut symbols = Vec::new();
                if let Some(arr) = res.get("result").and_then(|r| r.as_array()) {
                    for loc in arr {
                        let uri = loc.get("uri").and_then(|u| u.as_str()).unwrap_or("");
                        let path = uri.strip_prefix("file://").unwrap_or(uri);
                        let def_line = loc.get("range").and_then(|r| r.get("start")).and_then(|s| s.get("line")).and_then(|l| l.as_u64()).unwrap_or(0);
                        let def_col = loc.get("range").and_then(|r| r.get("start")).and_then(|s| s.get("character")).and_then(|c| c.as_u64()).unwrap_or(0);
                        symbols.push(Symbol {
                            name: "".to_string(),
                            kind: "".to_string(),
                            file: PathBuf::from(path),
                            line: def_line as u32,
                            column: def_col as u32,
                        });
                    }
                }
                Ok(AnalysisResponse::GotoDefinition(symbols))
            }
            AnalysisRequest::ExplainLifetime { .. } => {
                Err(AnalysisError::Unsupported("ExplainLifetime not supported via LSP".into()))
            }
        }
    }

    fn is_real(&self) -> bool {
        true
    }
}
