//! P2 closure — remote planner adapter.
//!
//! `RemoteSessionAdapter` wraps a `vac_bridge::RemoteSession` so the
//! session-engine's `submit_one` can route the prompt over the bridge
//! instead of the mock `EchoAdapter`. The adapter:
//!
//! 1. Sends an `InboundEvent::Submit { text }` for the prompt.
//! 2. Collects every `OutboundEvent::Chunk` until the stream
//!    terminates with `SubmitFinished` / `SubmitAborted` / `Error`.
//! 3. Returns the concatenated chunks as the `LlmResponse.content`.
//!
//! Two ways to obtain one:
//!
//! - [`RemoteSessionAdapter::from_channels`] — pre-wired channel ends.
//!   Tests and in-process integrations use this; it keeps the adapter
//!   independent of any transport.
//! - [`RemoteSessionAdapter::spawn_stdio`] — spawn a subprocess that
//!   speaks newline-delimited JSON (`InboundEvent` ↔ `OutboundEvent`)
//!   on its stdio. Used by `vac plan --remote stdio:///path/to/bin`.
//!
//! **Transport policy:** only `stdio://<absolute-path>` URIs are
//! accepted today. `ws://` / `http://` return an explicit
//! `unsupported` error so callers can't silently fall back to the
//! mock path.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, mpsc};
use tokio::time::timeout;

use vac_bridge::{InboundEvent, OutboundEvent, RemoteSession};
use vac_session_engine::{EngineError, EngineResult, LlmAdapter, LlmRequest, LlmResponse};

/// Hard cap on how long we wait for the remote planner to finish a
/// single submit. Stops a stuck subprocess from hanging the CLI.
const REMOTE_SUBMIT_TIMEOUT: Duration = Duration::from_secs(60);

/// LlmAdapter that rides a `RemoteSession`. Cheap to clone — shares
/// the outbound receiver behind a mutex so concurrent submits queue
/// naturally.
pub struct RemoteSessionAdapter {
    inbound_tx: mpsc::Sender<InboundEvent>,
    outbound_rx: Arc<Mutex<mpsc::Receiver<OutboundEvent>>>,
    /// Held so the subprocess stays alive as long as the adapter is
    /// referenced. Dropping the adapter kills the child via
    /// `kill_on_drop`.
    _child: Option<Arc<Mutex<Child>>>,
}

impl RemoteSessionAdapter {
    /// Build from raw channel ends — useful for tests or for a caller
    /// that already owns a transport.
    pub fn from_channels(
        inbound_tx: mpsc::Sender<InboundEvent>,
        outbound_rx: mpsc::Receiver<OutboundEvent>,
    ) -> Self {
        Self {
            inbound_tx,
            outbound_rx: Arc::new(Mutex::new(outbound_rx)),
            _child: None,
        }
    }

    /// Wrap a pre-built `RemoteSession`. Convenience for in-process
    /// wiring where the caller controls both sides of the bridge.
    #[allow(dead_code)] // kept as public API surface for future in-process bridges
    pub fn from_session(session: RemoteSession) -> Self {
        let handle = session.handle();
        let (_inbound_rx, outbound_rx) = session.split();
        // inbound_rx isn't needed on this side — the remote end
        // consumes inbound via its own transport loop.
        drop(_inbound_rx);
        Self::from_channels(handle.inbound_tx, outbound_rx)
    }

    /// Spawn an external planner via stdio. `uri` must be of the form
    /// `stdio://<absolute-or-relative-binary>` optionally followed by
    /// space-separated args. Returns an error for any other scheme.
    pub async fn spawn_stdio(uri: &str) -> EngineResult<Self> {
        let spec = uri
            .strip_prefix("stdio://")
            .ok_or_else(|| {
                EngineError::Other(format!(
                    "remote planner: only stdio:// is supported, got {uri:?}"
                ))
            })?;
        let mut parts = spec.split_whitespace();
        let program = parts.next().ok_or_else(|| {
            EngineError::Other(
                "remote planner: stdio:// must name a binary".into(),
            )
        })?;
        let args: Vec<String> = parts.map(|s| s.to_string()).collect();
        let mut child = Command::new(program)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                EngineError::Other(format!(
                    "remote planner: failed to spawn {program}: {e}"
                ))
            })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| EngineError::Other("stdin pipe missing".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| EngineError::Other("stdout pipe missing".into()))?;

        // Build a RemoteSession to own the in-process channels.
        let session = RemoteSession::new(16);
        let handle = session.handle();
        let (inbound_rx, outbound_rx) = session.split();

        // Writer: ingest the session's inbound stream → subprocess stdin.
        let mut inbound_rx = inbound_rx;
        let mut stdin_w = stdin;
        tokio::spawn(async move {
            while let Some(ev) = inbound_rx.recv().await {
                let Ok(mut s) = serde_json::to_string(&ev) else {
                    continue;
                };
                s.push('\n');
                if stdin_w.write_all(s.as_bytes()).await.is_err() {
                    break;
                }
                if stdin_w.flush().await.is_err() {
                    break;
                }
            }
        });

        // Reader: subprocess stdout → session's outbound_tx.
        let outbound_tx = handle.outbound_tx.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                let n = match reader.read_line(&mut line).await {
                    Ok(n) => n,
                    Err(_) => break,
                };
                if n == 0 {
                    break;
                }
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(ev) = serde_json::from_str::<OutboundEvent>(trimmed) {
                    if outbound_tx.send(ev).await.is_err() {
                        break;
                    }
                }
            }
        });

        Ok(Self {
            inbound_tx: handle.inbound_tx,
            outbound_rx: Arc::new(Mutex::new(outbound_rx)),
            _child: Some(Arc::new(Mutex::new(child))),
        })
    }
}

#[async_trait]
impl LlmAdapter for RemoteSessionAdapter {
    async fn complete(&self, request: LlmRequest) -> EngineResult<LlmResponse> {
        // Flatten prompt + any context lines into a single Submit body.
        // The remote planner is responsible for its own internal split.
        let mut body = request.prompt;
        for ctx in &request.context {
            body.push('\n');
            body.push_str(ctx);
        }
        self.inbound_tx
            .send(InboundEvent::Submit { text: body })
            .await
            .map_err(|e| {
                EngineError::Other(format!("remote submit channel closed: {e}"))
            })?;

        let rx = self.outbound_rx.clone();
        let mut chunks = String::new();
        let fut = async {
            let mut guard = rx.lock().await;
            loop {
                match guard.recv().await {
                    Some(OutboundEvent::Chunk { text }) => chunks.push_str(&text),
                    Some(OutboundEvent::SubmitFinished) => return Ok::<_, EngineError>(()),
                    Some(OutboundEvent::SubmitAborted { reason }) => {
                        return Err(EngineError::Other(format!(
                            "remote submit aborted: {reason}"
                        )));
                    }
                    Some(OutboundEvent::Error { reason }) => {
                        return Err(EngineError::Other(format!(
                            "remote error: {reason}"
                        )));
                    }
                    // Handshake / permission events are irrelevant for
                    // the planner submit; skip and keep reading.
                    Some(_) => continue,
                    None => {
                        return Err(EngineError::Other(
                            "remote outbound channel closed before finish".into(),
                        ));
                    }
                }
            }
        };

        timeout(REMOTE_SUBMIT_TIMEOUT, fut)
            .await
            .map_err(|_| {
                EngineError::Other("remote submit timed out".into())
            })??;

        Ok(LlmResponse {
            provider: "vac_bridge".into(),
            model: "remote".into(),
            content: chunks,
            input_tokens: 0,
            output_tokens: 0,
        })
    }
}

/// Explicit URL-scheme gate used by `plan.rs` so unsupported
/// transports surface a clear error instead of silently routing to
/// the mock adapter. Returned as `Ok(PathBuf)` for `stdio://<bin>`
/// callers that want to keep the parsed path, or an `Err` with a
/// human-readable reason otherwise.
pub fn parse_remote_uri(uri: &str) -> Result<PathBuf, String> {
    if let Some(rest) = uri.strip_prefix("stdio://") {
        let program = rest
            .split_whitespace()
            .next()
            .ok_or_else(|| "stdio:// URI must name a binary".to_string())?;
        return Ok(PathBuf::from(program));
    }
    Err(format!(
        "unsupported remote scheme {uri:?}; only stdio:// is implemented today"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vac_bridge::{InboundEvent, OutboundEvent, RemoteSession};

    #[test]
    fn parse_remote_uri_accepts_stdio() {
        let p = parse_remote_uri("stdio:///usr/bin/planner").unwrap();
        assert_eq!(p, PathBuf::from("/usr/bin/planner"));
    }

    #[test]
    fn parse_remote_uri_rejects_ws() {
        let err = parse_remote_uri("ws://example.com/planner").unwrap_err();
        assert!(err.contains("unsupported"));
    }

    #[test]
    fn parse_remote_uri_rejects_http() {
        let err = parse_remote_uri("http://example.com/plan").unwrap_err();
        assert!(err.contains("unsupported"));
    }

    #[test]
    fn parse_remote_uri_rejects_empty_stdio() {
        let err = parse_remote_uri("stdio://").unwrap_err();
        assert!(err.contains("must name"));
    }

    #[tokio::test]
    async fn spawn_stdio_rejects_missing_binary() {
        match RemoteSessionAdapter::spawn_stdio("stdio://definitely-not-a-real-bin-9999").await {
            Err(EngineError::Other(_)) => {}
            Err(other) => panic!("unexpected error: {other:?}"),
            Ok(_) => panic!("spawn must fail for missing binary"),
        }
    }

    #[tokio::test]
    async fn spawn_stdio_rejects_non_stdio_scheme() {
        match RemoteSessionAdapter::spawn_stdio("ws://example.com").await {
            Err(EngineError::Other(msg)) => assert!(msg.contains("stdio://")),
            Err(other) => panic!("unexpected error: {other:?}"),
            Ok(_) => panic!("spawn must fail for unsupported scheme"),
        }
    }

    #[tokio::test]
    async fn adapter_submits_prompt_and_collects_chunks() {
        // In-process bridge: the "remote side" reads inbound Submit
        // events off the same session's inbound_rx and pushes a
        // canned chunk + SubmitFinished onto outbound_tx.
        let session = RemoteSession::new(16);
        let handle = session.handle();
        let (mut inbound_rx, outbound_rx) = session.split();
        let outbound_tx = handle.outbound_tx.clone();

        let remote = tokio::spawn(async move {
            while let Some(ev) = inbound_rx.recv().await {
                if let InboundEvent::Submit { text } = ev {
                    let _ = outbound_tx
                        .send(OutboundEvent::Chunk {
                            text: format!("remote:{text}"),
                        })
                        .await;
                    let _ = outbound_tx.send(OutboundEvent::SubmitFinished).await;
                    break;
                }
            }
        });

        let adapter = RemoteSessionAdapter::from_channels(handle.inbound_tx, outbound_rx);
        let req = LlmRequest {
            prompt: "draft the plan".into(),
            context: Vec::new(),
        };
        let resp = adapter.complete(req).await.unwrap();
        assert_eq!(resp.content, "remote:draft the plan");
        assert_eq!(resp.provider, "vac_bridge");
        remote.await.unwrap();
    }

    #[tokio::test]
    async fn adapter_surfaces_abort_as_engine_error() {
        let session = RemoteSession::new(4);
        let handle = session.handle();
        let (mut inbound_rx, outbound_rx) = session.split();
        let outbound_tx = handle.outbound_tx.clone();

        tokio::spawn(async move {
            let _ = inbound_rx.recv().await;
            let _ = outbound_tx
                .send(OutboundEvent::SubmitAborted {
                    reason: "budget exceeded".into(),
                })
                .await;
        });

        let adapter = RemoteSessionAdapter::from_channels(handle.inbound_tx, outbound_rx);
        let req = LlmRequest {
            prompt: "hi".into(),
            context: Vec::new(),
        };
        let err = adapter.complete(req).await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("aborted"), "unexpected error: {msg}");
    }
}
