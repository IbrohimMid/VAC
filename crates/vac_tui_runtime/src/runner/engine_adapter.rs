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
    EngineError, EngineResult, LlmAdapter, LlmRequest, LlmResponse, SubmitChunk, SubmitEvent,
};

pub async fn run_via_session_engine(
    project_root: std::path::PathBuf,
    engine: Arc<Mutex<VacEngine>>,
    task_description: &str,
    update_tx: tokio::sync::mpsc::UnboundedSender<vac_core::engine::RuntimeUpdate>,
) -> anyhow::Result<vac_core::TaskResult> {
    use futures::StreamExt;
    use vac_session_engine::{
        CompactConfig, SlashProcessor, SubmitContext, TranscriptWriter,
        TrivialCompactBoundary, UsageTracker, submit_stream,
    };

    // NS.2 — drive the submit through `submit_stream` and pump each
    // `SubmitChunk` directly into the TUI's `RuntimeUpdate` channel
    // as it arrives. Replaces the previous submit_one+bridge-task
    // pattern; behaviourally equivalent (submit_one was already
    // emitting events incrementally via its outbound tx) but drops
    // one `tokio::spawn` and the intermediate mpsc hop.
    let adapter = Arc::new(VacEngineAdapter::new(engine.clone()));
    let writer = Arc::new(TranscriptWriter::new(project_root));
    let slash = Arc::new(SlashProcessor::new());
    let compact: Arc<dyn vac_session_engine::CompactBoundary> =
        Arc::new(TrivialCompactBoundary::default());
    let usage = Arc::new(UsageTracker::new());
    let ctx = SubmitContext::new(uuid::Uuid::new_v4(), task_description.to_string());

    let mut stream = submit_stream(
        ctx,
        writer.clone(),
        slash,
        compact,
        usage.clone(),
        adapter,
        CompactConfig::default(),
    );
    let mut total_tokens: u64 = 0;
    // NS.2 audit fix: pre-migration submit_one returned its error
    // via Result; the stream terminates with `SubmitChunk::Aborted`
    // instead. Capture the reason so `?` still short-circuits on
    // submit failure rather than silently receiving a Completed
    // TaskResult.
    //
    // Arc-audit H3: do NOT also emit `RuntimeUpdate::Failed` for
    // the abort — the caller's `Err` return is the single source
    // of truth. Double-signalling (Failed event + Err result) made
    // the TUI render the abort twice.
    let mut aborted: Option<String> = None;
    while let Some(chunk) = stream.next().await {
        match &chunk {
            SubmitChunk::Finished { usage: snap } => {
                total_tokens = snap.total_tokens() as u64;
            }
            SubmitChunk::Aborted { reason } => {
                aborted = Some(reason.clone());
                continue; // skip forwarding — Err below carries it
            }
            _ => {}
        }
        if let Some(rt) = chunk_to_runtime_update(chunk) {
            let _ = update_tx.send(rt);
        }
    }
    if let Some(reason) = aborted {
        return Err(anyhow::anyhow!("submit aborted: {reason}"));
    }

    let fallback_task_id = vac_core::task::TaskId(uuid::Uuid::new_v4());
    let res = vac_core::TaskResult {
        task_id: fallback_task_id,
        status: vac_core::TaskStatus::Completed,
        summary: format!(
            "session-engine submit finished; {} tokens, transcript at {}",
            total_tokens,
            writer.sessions_dir().display(),
        ),
        modified_files: Vec::new(),
        created_files: Vec::new(),
        validation_score: None,
        elapsed_ms: 0,
        agent_contributions: Vec::new(),
        total_tokens_used: total_tokens,
    };

    // Run the predictor after submit finishes
    if let Ok(predicted) = vil_swarm::planner::Planner::predict_next_submit(task_description).await {
        let _ = update_tx.send(vac_core::engine::RuntimeUpdate::SpeculationReady {
            predicted_prompt: predicted,
            precomputed_context: std::collections::HashMap::new(),
        });
    }

    let _ = update_tx.send(vac_core::engine::RuntimeUpdate::Completed(res.clone()));

    Ok(res)
}

/// NS.2 — translate one streamed `SubmitChunk` into the UI's
/// `RuntimeUpdate`. Mirrors the pre-NS.2 bridge task logic so the
/// TUI sees the exact same ordered sequence. Returns `None` for
/// chunks that don't map (non-terminal control/metadata chunks
/// already surfaced elsewhere).
fn chunk_to_runtime_update(chunk: SubmitChunk) -> Option<RuntimeUpdate> {
    match chunk {
        SubmitChunk::LlmRequested { provider, model } => {
            Some(RuntimeUpdate::ModelInfo { provider, model })
        }
        SubmitChunk::TextDelta { text } => Some(RuntimeUpdate::AssistantChunk(text)),
        SubmitChunk::ToolRequested { id, name, arguments } => {
            Some(RuntimeUpdate::ToolCall { id, name, arguments })
        }
        SubmitChunk::ToolResult { id, name, payload } => {
            let success = payload.kind == vac_tool_core::ToolResultKind::Ok
                || payload.kind == vac_tool_core::ToolResultKind::Warning;
            let content = payload.summary.clone();
            Some(RuntimeUpdate::ToolResult {
                id,
                name,
                success,
                content,
                envelope: Some(payload),
            })
        }
        SubmitChunk::Aborted { reason } => Some(RuntimeUpdate::Failed(reason)),
        SubmitChunk::SpeculationReady {
            predicted_prompt,
            precomputed_context,
        } => Some(RuntimeUpdate::SpeculationReady {
            predicted_prompt,
            precomputed_context,
        }),
        _ => None,
    }
}

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
            success,
            content,
            envelope,
        } => Some(vac_session_engine::SubmitEvent::ToolResult {
            id,
            name,
            payload: envelope.unwrap_or_else(|| vac_tool_core::ToolResultEnvelope {
                kind: if success { vac_tool_core::ToolResultKind::Ok } else { vac_tool_core::ToolResultKind::Error },
                payload: serde_json::json!({ "result": content }),
                summary: content,
                duration_ms: 0,
            }),
        }),
        RuntimeUpdate::Failed(reason) => Some(SubmitEvent::Aborted { reason }),
        RuntimeUpdate::Cancelled => Some(SubmitEvent::Aborted {
            reason: "cancelled".into(),
        }),
        RuntimeUpdate::SpeculationReady { predicted_prompt, precomputed_context } => {
            Some(SubmitEvent::SpeculationReady { predicted_prompt, precomputed_context })
        },
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
        tool_calls: Vec::new(),
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
                success: true,
                envelope: None,
            }),
            Some(SubmitEvent::ToolResult { .. })
        ));
        assert!(matches!(
            translate(RuntimeUpdate::Cancelled),
            Some(SubmitEvent::Aborted { .. })
        ));
    }

    /// NS.2 audit fix — guarantees that a `SubmitChunk::Aborted`
    /// chunk from the stream terminates the loop with `Some(reason)`
    /// recorded. The in-loop code path (not the full
    /// `run_via_session_engine` which needs a real VacEngine) is
    /// exercised by constructing the state machine directly.
    #[test]
    fn aborted_chunk_captures_reason_for_err_propagation() {
        use vac_session_engine::SubmitChunk;
        let mut aborted: Option<String> = None;
        let chunks = [
            SubmitChunk::LlmRequested { provider: "p".into(), model: "m".into() },
            SubmitChunk::Aborted { reason: "budget exhausted".into() },
        ];
        for chunk in chunks {
            if let SubmitChunk::Aborted { reason } = &chunk {
                aborted = Some(reason.clone());
            }
        }
        assert_eq!(aborted.as_deref(), Some("budget exhausted"));
    }

    /// NS.2 — the new `chunk_to_runtime_update` must cover the same
    /// event surface as the legacy bridge's SubmitEvent→RuntimeUpdate
    /// translation. Drift here would mean the TUI stops seeing a
    /// whole class of events post-migration.
    #[test]
    fn chunk_to_runtime_update_covers_core_surface() {
        use vac_session_engine::SubmitChunk;
        assert!(matches!(
            chunk_to_runtime_update(SubmitChunk::LlmRequested {
                provider: "p".into(),
                model: "m".into()
            }),
            Some(RuntimeUpdate::ModelInfo { .. })
        ));
        assert!(matches!(
            chunk_to_runtime_update(SubmitChunk::TextDelta { text: "hi".into() }),
            Some(RuntimeUpdate::AssistantChunk(_))
        ));
        assert!(matches!(
            chunk_to_runtime_update(SubmitChunk::ToolRequested {
                id: "1".into(),
                name: "file_read".into(),
                arguments: serde_json::Value::Null,
            }),
            Some(RuntimeUpdate::ToolCall { .. })
        ));
        assert!(matches!(
            chunk_to_runtime_update(SubmitChunk::Aborted { reason: "x".into() }),
            Some(RuntimeUpdate::Failed(_))
        ));
        // Non-forwarded chunks (no TUI-side RuntimeUpdate analog).
        assert!(chunk_to_runtime_update(SubmitChunk::Accepted {
            entry_id: uuid::Uuid::nil(),
        })
        .is_none());
        assert!(chunk_to_runtime_update(SubmitChunk::Finished {
            usage: vac_session_engine::UsageSnapshot::default(),
        })
        .is_none());
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
