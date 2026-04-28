//! G1 — TUI elicitation overlay state + handler.
//!
//! When an MCP server issues an `elicitation/request`, the TUI parks
//! an [`ElicitationPrompt`] on `AppState.layout.elicitation` and
//! renders a modal. The operator hits Enter (open the URL in the
//! browser) or Esc (cancel); the overlay takes the oneshot sender
//! and ships back the corresponding [`ElicitationResult`].
//!
//! The MCP-facing [`TuiElicitationHandler`] is the other half — it
//! pushes the prompt and awaits the oneshot, with a 120-second
//! timeout that maps to `Cancelled`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{Mutex, oneshot};

use vac_mcp_core::{
    ElicitationHandler, ElicitationRequest, ElicitationResult,
    error::{McpCoreError, McpCoreResult},
};

use crate::app::AppState;

/// Parked prompt the view renders. `response_tx` is wrapped in an
/// Option so the LayoutState can stay non-Clone but the sender can
/// still be taken out when the operator resolves the prompt.
#[derive(Debug)]
pub struct ElicitationPrompt {
    pub url: String,
    pub prompt: Option<String>,
    pub response_tx: Option<oneshot::Sender<ElicitationResult>>,
}

impl ElicitationPrompt {
    pub fn new(
        url: impl Into<String>,
        prompt: Option<String>,
        tx: oneshot::Sender<ElicitationResult>,
    ) -> Self {
        Self {
            url: url.into(),
            prompt,
            response_tx: Some(tx),
        }
    }

    /// Resolve the prompt with a result. Returns `true` when the
    /// sender was still present. A dropped receiver is not an error;
    /// it just means the handler timed out before the operator
    /// reacted.
    pub fn resolve(&mut self, result: ElicitationResult) -> bool {
        match self.response_tx.take() {
            Some(tx) => tx.send(result).is_ok(),
            None => false,
        }
    }
}

/// Shared handle the handler uses to reach the TUI state.
pub type SharedState = Arc<Mutex<AppState>>;

/// G1b — TUI-facing MCP elicitation handler. Pushes a prompt onto
/// `AppState.layout.elicitation`, awaits the oneshot. Text /
/// Confirm are NOT yet rendered as modals — a follow-up landing
/// adds the input variant. Today those log + return `Cancelled`.
pub struct TuiElicitationHandler {
    state: SharedState,
}

impl std::fmt::Debug for TuiElicitationHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TuiElicitationHandler").finish()
    }
}

impl TuiElicitationHandler {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }

    /// Timeout before a dangling prompt maps to `Cancelled`.
    pub const TIMEOUT: Duration = Duration::from_secs(120);
}

#[async_trait]
impl ElicitationHandler for TuiElicitationHandler {
    async fn handle(&self, request: ElicitationRequest) -> McpCoreResult<ElicitationResult> {
        match request {
            ElicitationRequest::OpenUrl { url, prompt } => {
                let (tx, rx) = oneshot::channel();
                {
                    let mut guard = self.state.lock().await;
                    // Concurrency guard — if a prior elicitation is
                    // still parked, explicitly cancel its oneshot so
                    // the earlier server sees `Cancelled` with a
                    // matching audit log rather than a silent
                    // `RecvError` from dropped-sender. The operator
                    // then sees two denials in the A1 feed instead
                    // of one prompt magically replacing another.
                    if let Some(mut prior) = guard.layout.elicitation.take() {
                        let resolved = prior.resolve(ElicitationResult::Cancelled);
                        tracing::warn!(
                            target: "vac_mcp_core::channel",
                            new_url = %url,
                            prior_url = %prior.url,
                            prior_resolved = resolved,
                            "elicitation: pre-empting in-flight prompt with newer request",
                        );
                    }
                    guard.layout.elicitation = Some(ElicitationPrompt::new(url, prompt, tx));
                    crate::overlay::open_overlay(
                        &mut guard,
                        crate::overlay::OverlayId::Elicitation,
                    );
                }
                match tokio::time::timeout(Self::TIMEOUT, rx).await {
                    Ok(Ok(result)) => Ok(result),
                    Ok(Err(_recv_err)) => Ok(ElicitationResult::Cancelled),
                    Err(_) => {
                        // Timed out — clear the prompt so the
                        // overlay closes. Drop sender (no-op; we
                        // hold rx side). Emit on the A1 channel
                        // target so operator sees "elicitation
                        // timed out".
                        let mut guard = self.state.lock().await;
                        guard.layout.elicitation = None;
                        crate::overlay::close_overlay(
                            &mut guard,
                            crate::overlay::OverlayId::Elicitation,
                        );
                        tracing::warn!(
                            target: "vac_mcp_core::channel",
                            "elicitation OpenUrl timed out after {}s",
                            Self::TIMEOUT.as_secs(),
                        );
                        Ok(ElicitationResult::Cancelled)
                    }
                }
            }
            ElicitationRequest::Text { prompt, .. } | ElicitationRequest::Confirm { prompt } => {
                // TODO: render Text / Confirm modals. Current build
                // ships OpenUrl only; the other variants degrade
                // cleanly via Cancelled + a warn on the A1 channel.
                tracing::warn!(
                    target: "vac_mcp_core::channel",
                    prompt = %prompt,
                    "elicitation Text/Confirm not yet surfaced — returning Cancelled",
                );
                Ok(ElicitationResult::Cancelled)
            }
            _ => {
                // Future variants land degraded-Cancelled with a
                // warn so operators see "we saw it, we couldn't
                // handle it" rather than a silent stall.
                tracing::warn!(
                    target: "vac_mcp_core::channel",
                    "elicitation variant not recognised — returning Cancelled",
                );
                Ok(ElicitationResult::Cancelled)
            }
        }
    }
}

/// Resolve the currently-parked prompt as `Accepted` (empty values)
/// after firing the browser. Caller closes the overlay.
pub fn accept_current(state: &mut AppState) -> bool {
    if let Some(prompt) = state.layout.elicitation.as_mut() {
        prompt.resolve(ElicitationResult::Accepted {
            values: HashMap::new(),
        })
    } else {
        false
    }
}

/// Resolve the currently-parked prompt as `Cancelled` (Esc).
pub fn cancel_current(state: &mut AppState) -> bool {
    if let Some(prompt) = state.layout.elicitation.as_mut() {
        prompt.resolve(ElicitationResult::Cancelled)
    } else {
        false
    }
}

/// Best-effort browser launch. Returns Err only if `open::that`
/// reports a hard failure. Called from the Enter handler.
pub fn launch_url(url: &str) -> McpCoreResult<()> {
    open::that(url).map_err(|e| McpCoreError::Protocol(format!("failed to open url '{url}': {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppState, AppStateOptions};

    fn make_state() -> AppState {
        AppState::new(AppStateOptions {
            model: None,
            session_id: Some(uuid::Uuid::new_v4().to_string()),
            checkpoint_path: None,
            project_root: std::env::current_dir().unwrap(),
        })
    }

    #[tokio::test]
    async fn accept_current_delivers_oneshot() {
        let mut state = make_state();
        let (tx, rx) = oneshot::channel();
        state.layout.elicitation = Some(ElicitationPrompt::new(
            "https://example.test/auth",
            None,
            tx,
        ));
        assert!(accept_current(&mut state));
        let got = rx.await.unwrap();
        match got {
            ElicitationResult::Accepted { values } => {
                assert!(values.is_empty());
            }
            other => panic!("expected Accepted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn cancel_current_delivers_cancelled() {
        let mut state = make_state();
        let (tx, rx) = oneshot::channel();
        state.layout.elicitation = Some(ElicitationPrompt::new(
            "https://example.test/auth",
            None,
            tx,
        ));
        assert!(cancel_current(&mut state));
        assert_eq!(rx.await.unwrap(), ElicitationResult::Cancelled);
    }

    #[tokio::test]
    async fn handler_cancels_prior_prompt_when_preempted() {
        let state: SharedState = Arc::new(Mutex::new(make_state()));
        let handler = TuiElicitationHandler::new(state.clone());

        // Park a first prompt manually (as if an earlier handler
        // call was still awaiting) and capture its oneshot receiver
        // so we can assert it was resolved with Cancelled.
        let (tx, mut rx) = oneshot::channel();
        {
            let mut guard = state.lock().await;
            guard.layout.elicitation = Some(ElicitationPrompt::new("https://first.test", None, tx));
            crate::overlay::open_overlay(&mut guard, crate::overlay::OverlayId::Elicitation);
        }

        let task = tokio::spawn(async move {
            handler
                .handle(ElicitationRequest::OpenUrl {
                    url: "https://second.test".into(),
                    prompt: None,
                })
                .await
        });

        // First prompt's oneshot must resolve to Cancelled — not
        // RecvError from a dropped sender.
        let first = tokio::time::timeout(Duration::from_secs(2), &mut rx)
            .await
            .expect("first prompt resolved within timeout");
        assert_eq!(first.unwrap(), ElicitationResult::Cancelled);

        // Resolve the newer one so the task exits cleanly.
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            let guard = state.lock().await;
            if guard
                .layout
                .elicitation
                .as_ref()
                .map(|p| p.url == "https://second.test")
                .unwrap_or(false)
            {
                break;
            }
        }
        {
            let mut guard = state.lock().await;
            accept_current(&mut guard);
            crate::overlay::close_overlay(&mut guard, crate::overlay::OverlayId::Elicitation);
        }
        let _ = task.await;
    }

    #[tokio::test]
    async fn handler_open_url_pushes_overlay_and_resolves_on_accept() {
        let state: SharedState = Arc::new(Mutex::new(make_state()));
        let handler = TuiElicitationHandler::new(state.clone());

        // Drive the handler in the background.
        let task = tokio::spawn(async move {
            handler
                .handle(ElicitationRequest::OpenUrl {
                    url: "https://example.test/auth".into(),
                    prompt: Some("authorise".into()),
                })
                .await
        });

        // Wait for the prompt to appear (lock contention loop).
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            let guard = state.lock().await;
            if guard.layout.elicitation.is_some() {
                break;
            }
        }

        {
            let mut guard = state.lock().await;
            assert!(guard.layout.elicitation.is_some());
            assert!(accept_current(&mut guard));
            crate::overlay::close_overlay(&mut guard, crate::overlay::OverlayId::Elicitation);
        }

        let result = task.await.unwrap().unwrap();
        match result {
            ElicitationResult::Accepted { .. } => {}
            other => panic!("expected Accepted, got {other:?}"),
        }
    }
}
