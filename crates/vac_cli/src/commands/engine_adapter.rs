//! R0.a — `VacEngineAdapter`.
//!
//! Bridges the existing `vac_core::VacEngine` orchestrator (which
//! owns tool dispatch, vil_swarm, approval plumbing) into
//! `vac_session_engine::LlmAdapter`. The result: `submit_one` becomes
//! the single durable spine for every `vac run`-style flow while
//! VacEngine continues to do the heavy lifting behind the trait.
//!
//! ## What the adapter does
//!
//! 1. Receives the submit prompt as an `LlmRequest` from `submit_one`.
//! 2. Drives `engine.run_task_with_updates(prompt, Some(rt_tx))`.
//! 3. Translates each `RuntimeUpdate` into a `SubmitEvent` and
//!    forwards it to the caller's outbound channel, so the TUI /
//!    stdout renderer sees the same event stream it saw before.
//! 4. Waits for the final `TaskResult`, maps it to an `LlmResponse`
//!    the engine can persist into the `LlmResponse` transcript row.
//!
//! ## What's explicitly NOT done here
//!
//! - Streaming fidelity is a best-effort: VacEngine is an orchestrator
//!   not a token streamer, so the `LlmResponse.content` is the task
//!   summary, not a character-by-character stream. Tool-call detail
//!   arrives via `SubmitEvent::ToolRequested` / `ToolResult` so
//!   operators still see every action.
//! - Approval channel semantics are untouched. VacEngine's existing
//!   `ApprovalHandle` is what the caller wires; the adapter just
//!   passes updates through.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{Mutex, mpsc};
use vac_core::engine::{RuntimeUpdate, VacEngine};
use vac_session_engine::{
    EngineError, EngineResult, LlmAdapter, LlmRequest, LlmResponse, SubmitEvent,
};

/// Adapter that makes `VacEngine` appear as an `LlmAdapter` to
/// `submit_one`. Optional `event_forward` lets the adapter surface
/// `RuntimeUpdate`s as `SubmitEvent`s on the submit's outbound
/// channel — pass a clone of the same sender `submit_one` receives.
pub struct VacEngineAdapter {
    engine: Arc<Mutex<VacEngine>>,
    event_forward: Option<mpsc::UnboundedSender<SubmitEvent>>,
}

impl VacEngineAdapter {
    #[must_use]
    pub fn new(engine: Arc<Mutex<VacEngine>>) -> Self {
        Self {
            engine,
            event_forward: None,
        }
    }

    #[must_use]
    pub fn with_event_forward(
        engine: Arc<Mutex<VacEngine>>,
        tx: mpsc::UnboundedSender<SubmitEvent>,
    ) -> Self {
        Self {
            engine,
            event_forward: Some(tx),
        }
    }
}

/// Translate one `RuntimeUpdate` into an equivalent `SubmitEvent`.
/// Returns `None` for updates that have no meaningful session-engine
/// mapping (LSP status, validation score) — those are already
/// surfaced via the TUI's own channels and would pollute the
/// transcript if duplicated.
fn translate(update: RuntimeUpdate) -> Option<SubmitEvent> {
    match update {
        RuntimeUpdate::ModelInfo { provider, model } => {
            Some(SubmitEvent::LlmRequested { provider, model })
        }
        RuntimeUpdate::AssistantChunk(text) => Some(SubmitEvent::LlmChunk { text }),
        RuntimeUpdate::ToolCall {
            id,
            name,
            arguments,
        } => Some(SubmitEvent::ToolRequested {
            id,
            name,
            arguments,
        }),
        RuntimeUpdate::ToolResult {
            id,
            name,
            content,
            success,
        } => Some(SubmitEvent::ToolResult {
            id,
            name,
            success,
            summary: content,
        }),
        RuntimeUpdate::Failed(reason) => Some(SubmitEvent::Aborted { reason }),
        RuntimeUpdate::Cancelled => Some(SubmitEvent::Aborted {
            reason: "cancelled".into(),
        }),
        // Completed is handled by submit_one's own Finished emission.
        // Status / LspStatus / LspDiagnostics / ValidationResult /
        // ApprovalRequired are surfaced through other channels (the
        // TUI's update/approval queues) and would distort the
        // transcript if copied here.
        _ => None,
    }
}

#[async_trait]
impl LlmAdapter for VacEngineAdapter {
    async fn complete(&self, req: LlmRequest) -> EngineResult<LlmResponse> {
        let (rt_tx, mut rt_rx) = mpsc::unbounded_channel::<RuntimeUpdate>();

        // Forwarder: pull RuntimeUpdates and fan them to the submit
        // channel as SubmitEvents. Drops silently if the forward
        // channel isn't attached.
        let forward = self.event_forward.clone();
        let translator = tokio::spawn(async move {
            while let Some(update) = rt_rx.recv().await {
                if let Some(ev) = translate(update) {
                    if let Some(tx) = &forward {
                        let _ = tx.send(ev);
                    }
                }
            }
        });

        let result = {
            let mut engine = self.engine.lock().await;
            engine
                .run_task_with_updates(&req.prompt, Some(rt_tx))
                .await
                .map_err(|e| EngineError::Other(format!("vac_core engine: {e}")))?
        };

        // Drop the Sender by dropping the engine guard already happened;
        // drop by letting `rt_tx` go out of scope — the translator
        // loop will exit once the channel closes.
        if let Err(e) = translator.await {
            tracing::warn!(
                target: "vac_cli::engine_adapter",
                "translator task join error: {e}",
            );
        }

        // VacEngine doesn't surface the concrete provider/model in
        // the TaskResult today — the TUI reads it via a separate
        // ModelInfo event that we've already forwarded above. Use a
        // stable label here so the transcript row is readable; a
        // future refactor can lift the real ids out of VacEngine.
        Ok(LlmResponse {
            provider: "vac_core".into(),
            model: result
                .agent_contributions
                .first()
                .map(|c| c.agent_role.clone())
                .unwrap_or_else(|| "unknown".into()),
            content: result.summary,
            input_tokens: req.prompt.split_whitespace().count() as u64,
            output_tokens: result.total_tokens_used,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_preserves_core_surface() {
        assert!(matches!(
            translate(RuntimeUpdate::ModelInfo {
                provider: "anthropic".into(),
                model: "claude-sonnet-4-6".into()
            }),
            Some(SubmitEvent::LlmRequested { .. })
        ));
        assert!(matches!(
            translate(RuntimeUpdate::AssistantChunk("hi".into())),
            Some(SubmitEvent::LlmChunk { .. })
        ));
        assert!(matches!(
            translate(RuntimeUpdate::ToolCall {
                id: "1".into(),
                name: "file_read".into(),
                arguments: serde_json::Value::Null
            }),
            Some(SubmitEvent::ToolRequested { .. })
        ));
        assert!(matches!(
            translate(RuntimeUpdate::ToolResult {
                id: "1".into(),
                name: "file_read".into(),
                content: "ok".into(),
                success: true
            }),
            Some(SubmitEvent::ToolResult { success: true, .. })
        ));
        assert!(matches!(
            translate(RuntimeUpdate::Cancelled),
            Some(SubmitEvent::Aborted { .. })
        ));
    }

    #[test]
    fn translate_drops_unmapped_updates() {
        // Status / LspStatus / ValidationResult / ApprovalRequired
        // are driver-surface and must not land on the transcript.
        assert!(translate(RuntimeUpdate::Status("init".into())).is_none());
        assert!(
            translate(RuntimeUpdate::LspStatus {
                available: false,
                binary_path: "".into()
            })
            .is_none()
        );
        assert!(
            translate(RuntimeUpdate::ValidationResult {
                score: 1.0,
                issues: vec![]
            })
            .is_none()
        );
        assert!(
            translate(RuntimeUpdate::ApprovalRequired {
                tool_call_id: "1".into(),
                tool_name: "x".into(),
                arguments: serde_json::Value::Null,
                explanation: None
            })
            .is_none()
        );
    }
}
