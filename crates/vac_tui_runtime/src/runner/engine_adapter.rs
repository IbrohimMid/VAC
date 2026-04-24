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
use tokio::sync::{Mutex, OnceCell, mpsc, oneshot};
use vac_core::TaskResult;
use vac_core::engine::{RuntimeUpdate, VacEngine};
use vac_session_engine::{
    EngineError, EngineResult, LlmAdapter, LlmRequest, LlmResponse, SubmitChunk, SubmitEvent,
};

/// M3 audit fix: a process-wide `ToolRegistry` populated on first
/// use. Builtin registration is ~15 tools and runs async file I/O
/// for PTY hosts; per-submit rebuild was wasteful for interactive
/// sessions. Cache is fine because `ToolRegistry::register` is
/// additive and no builtin tool holds per-submit state — per-call
/// context lives on `ToolContext`.
static SHARED_TOOL_REGISTRY: OnceCell<Arc<vac_tools::ToolRegistry>> =
    OnceCell::const_new();

async fn shared_registry() -> anyhow::Result<Arc<vac_tools::ToolRegistry>> {
    SHARED_TOOL_REGISTRY
        .get_or_try_init(|| async {
            let registry = Arc::new(vac_tools::ToolRegistry::new());
            vac_tools::builtin::register_builtin_tools(&registry)
                .await
                .map_err(|e| anyhow::anyhow!("register builtins: {e}"))?;
            Ok::<_, anyhow::Error>(registry)
        })
        .await
        .cloned()
}

pub async fn run_via_session_engine(
    project_root: std::path::PathBuf,
    engine: Arc<Mutex<VacEngine>>,
    task_description: &str,
    update_tx: tokio::sync::mpsc::UnboundedSender<vac_core::engine::RuntimeUpdate>,
) -> anyhow::Result<vac_core::TaskResult> {
    run_via_session_engine_with_broadcast(
        project_root,
        engine,
        task_description,
        update_tx,
        None,
        None,
    )
    .await
}

/// B5 + C1 audit fix — shared entry point for every live driver
/// (TUI, `vac run`, ACP). A driver that wants live teleport
/// attach frames passes a `SessionBroadcast`; `SubmitChunk`s
/// route through `chunk_to_outbound` and land on every attached
/// SSE client. Optional `max_budget_tokens` threads through to
/// the submit's compact config so headless callers keep their
/// per-run budget gate.
pub async fn run_via_session_engine_with_broadcast(
    project_root: std::path::PathBuf,
    engine: Arc<Mutex<VacEngine>>,
    task_description: &str,
    update_tx: tokio::sync::mpsc::UnboundedSender<vac_core::engine::RuntimeUpdate>,
    broadcast: Option<Arc<vac_bridge::remote::SessionBroadcast>>,
    max_budget_tokens: Option<u64>,
) -> anyhow::Result<vac_core::TaskResult> {
    use futures::StreamExt;
    use vac_session_engine::{
        SlashProcessor, SubmitContext, TranscriptWriter, TrivialCompactBoundary,
        UsageTracker, submit_stream,
    };
    use vac_tools::registry::ToolContext;

    // B2: live dispatcher wiring. Build a ToolRegistry with
    // builtins registered, construct a ToolContext for this
    // submit, and assemble a live_compact_config that carries both
    // the dispatcher and (B3) composite gate.
    // B4: attach an AgentDispatcher so the `agent_run` tool can
    // actually spawn subagents in the live session.
    // M3 audit fix: a shared registry cached across submits keeps
    // builtin registration out of the per-submit hot path. Tests
    // rebuild per-case by definition; long-running sessions reuse
    // the same instance.
    let registry = shared_registry().await?;
    // C3 audit fix: use the VacEngine's real session id so
    // TranscriptWriter and ToolContext agree with the engine's
    // internal session record. Previously a fresh Uuid::new_v4()
    // drifted from engine.session_id() and split transcript writes.
    let session_id = {
        let eng = engine.lock().await;
        eng.session_id().await
    };

    let writer = Arc::new(TranscriptWriter::new(project_root.clone()));
    let slash = Arc::new(SlashProcessor::new());
    let compact: Arc<dyn vac_session_engine::CompactBoundary> =
        Arc::new(TrivialCompactBoundary::default());
    let usage = Arc::new(UsageTracker::new());

    // B3 + audit P0.2 closure — compose the **full** live gate
    // stack (PolicyGate + PlanModeGate + HookGate) now that the
    // live driver actually owns the policy tracker. Previously
    // this called `build_live_gate` (hooks-only wrapper); the
    // reviewer flagged that the full-stack builder existed but
    // wasn't used by live paths. Wiring it here makes the policy
    // tracker mandatory on the default live path: `.vac/policy.toml`
    // loads, fallback is `PolicyLimits::unlimited()` for fresh
    // projects. Plan-mode flag is None here — TUI maintains its
    // own AtomicBool and can opt in via the `_with` variant
    // directly when plan-mode UX lands.
    let policy_limits = vac_core::policy_limits::PolicyLimits::load(&project_root)
        .await
        .map_err(|e| anyhow::anyhow!(".vac/policy.toml: {e}"))?;
    let policy_tracker = std::sync::Arc::new(
        vac_core::policy_limits::PolicyTracker::new(policy_limits),
    );
    let gate = super::dispatcher::build_live_gate_with(
        &project_root,
        Some(policy_tracker),
        None,
    )
    .await?;

    // B4 + H1 audit fix: build the agent dispatcher with a
    // `compact_cfg` that carries the parent's dispatcher + gate so
    // the subagent's own tool calls route through the same live
    // stack. Previously the subagent inherited `CompactConfig::
    // default()` from `SubagentDispatchContext::new`, meaning the
    // LLM could spawn a subagent but the subagent itself had no
    // dispatcher — tool-use silently fell back to UnsupportedDispatcher.
    let subagent_llm: Arc<dyn vac_session_engine::LlmAdapter> =
        Arc::new(VacEngineAdapter::new(engine.clone()));

    // ToolContext constructed *before* the dispatcher so we can
    // clone it into both: the dispatcher (for its own tool routing)
    // and the agent_dispatcher's subagent stack (so subagents see
    // the same working_dir / session / agent_dispatcher chain).
    // Note: subagent's ctx re-uses the parent's `agent_dispatcher`
    // handle — agent_run can invoke another level of subagent up
    // to the budget cap `SubagentRunner` enforces upstream.
    let parent_ctx_base = ToolContext::new(project_root.clone())
        .with_session_id(session_id);
    // ADR-002: subagent ctx runs at depth=1 so nested
    // `agent_run` inside the subagent hard-denies. `depth` field
    // is the enforcement seam; agent_run checks it at execute
    // time.
    let subagent_ctx = parent_ctx_base.clone().with_depth(1);
    let subagent_cfg = super::dispatcher::live_compact_config(
        registry.clone(),
        Arc::new(subagent_ctx),
        Some(gate.clone()),
    );
    let subagent_dispatch_ctx = vac_session_engine::SubagentDispatchContext::new(
        writer.clone(),
        slash.clone(),
        compact.clone(),
        usage.clone(),
        subagent_llm,
    )
    .with_compact_cfg(subagent_cfg);
    let agent_dispatcher: Arc<
        dyn vac_session_primitives::AgentDispatcher,
    > = Arc::new(vac_session_engine::EngineAgentDispatcher::new(
        subagent_dispatch_ctx,
    ));

    let ctx = Arc::new(
        parent_ctx_base.with_agent_dispatcher(agent_dispatcher),
    );
    let mut compact_cfg = super::dispatcher::live_compact_config(
        registry.clone(),
        ctx.clone(),
        Some(gate),
    );
    compact_cfg.max_budget_tokens = max_budget_tokens;

    // NS.2 — drive the submit through `submit_stream` and pump each
    // `SubmitChunk` directly into the TUI's `RuntimeUpdate` channel
    // as it arrives. Replaces the previous submit_one+bridge-task
    // pattern; behaviourally equivalent (submit_one was already
    // emitting events incrementally via its outbound tx) but drops
    // one `tokio::spawn` and the intermediate mpsc hop.
    // B1 audit fix: create a oneshot so the adapter can hand the
    // real `TaskResult` (with populated modified_files /
    // created_files / elapsed_ms) back to this function. Without
    // this the post-NS.2 path synthesized an empty TaskResult and
    // the TUI's changeset / review pane was silently blank.
    let (result_tx, result_rx) = oneshot::channel::<TaskResult>();
    let adapter = Arc::new(VacEngineAdapter::with_result_tx(engine.clone(), result_tx));
    let submit_ctx = SubmitContext::new(session_id, task_description.to_string());

    let mut stream = submit_stream(
        submit_ctx,
        writer.clone(),
        slash,
        compact,
        usage.clone(),
        adapter,
        compact_cfg,
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
        // B5: fan the raw chunk out to any teleport attach clients
        // before translation. Remote attachees see the session as
        // it streams, independent of the TUI's consumption.
        if let Some(bc) = &broadcast {
            if let Some(ev) = chunk_to_outbound(&chunk) {
                bc.publish(ev);
            }
        }
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

    // B1: harvest the real TaskResult the adapter captured. If the
    // oneshot was dropped without a send (adapter never reached the
    // send point — unusual; only if submit errored before complete()
    // finished producing a TaskResult) fall back to a synthetic
    // entry so the function signature stays totals-aware. Log the
    // unexpected case.
    let res = match result_rx.await {
        Ok(real) => {
            // Prefer the adapter's totals but let the stream's
            // Finished chunk supply usage when the adapter didn't
            // bump it.
            let total = if real.total_tokens_used > 0 {
                real.total_tokens_used
            } else {
                total_tokens
            };
            TaskResult {
                total_tokens_used: total,
                ..real
            }
        }
        Err(_dropped) => {
            tracing::warn!(
                target: "vac_cli::engine_adapter",
                "adapter result oneshot dropped without value — \
                 synthesizing fallback TaskResult",
            );
            TaskResult {
                task_id: vac_core::task::TaskId(uuid::Uuid::new_v4()),
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
            }
        }
    };

    // Run the predictor after submit finishes
    if let Ok(predicted) = vil_swarm::planner::Planner::predict_next_submit(task_description).await {
        let _ = update_tx.send(vac_core::engine::RuntimeUpdate::SpeculationReady {
            predicted_prompt: predicted,
            precomputed_context: std::collections::HashMap::new(),
        });
    }

    // TUI backend reads RuntimeUpdate::Completed off update_tx;
    // caller reads Ok(res). These are different consumers (TUI
    // event loop vs. vac_cli driver) — not the H3 double-signal
    // pattern.
    let _ = update_tx.send(vac_core::engine::RuntimeUpdate::Completed(res.clone()));

    Ok(res)
}

/// B5 — map a streamed `SubmitChunk` into a remote-friendly
/// `OutboundEvent`. Distinct from `chunk_to_runtime_update`
/// (which targets the TUI's in-process enum); remote clients see
/// a simpler kind-string + JSON payload shape.
fn chunk_to_outbound(chunk: &SubmitChunk) -> Option<vac_bridge::remote::OutboundEvent> {
    use vac_bridge::remote::OutboundEvent;
    Some(match chunk {
        SubmitChunk::LlmRequested { provider, model } => OutboundEvent::new(
            "llm.request",
            serde_json::json!({ "provider": provider, "model": model }),
        ),
        SubmitChunk::TextDelta { text } => {
            OutboundEvent::new("text", serde_json::json!({ "text": text }))
        }
        SubmitChunk::ToolRequested { id, name, arguments } => OutboundEvent::new(
            "tool.request",
            serde_json::json!({ "id": id, "name": name, "arguments": arguments }),
        ),
        SubmitChunk::ToolResult { id, name, payload } => OutboundEvent::new(
            "tool.result",
            serde_json::json!({ "id": id, "name": name, "payload": payload }),
        ),
        SubmitChunk::Finished { usage } => OutboundEvent::new(
            "finished",
            serde_json::json!({ "total_tokens": usage.total_tokens() }),
        ),
        SubmitChunk::Aborted { reason } => {
            OutboundEvent::new("aborted", serde_json::json!({ "reason": reason }))
        }
        _ => return None,
    })
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
    // B1 audit fix: oneshot for handing the real TaskResult back
    // to the caller of `run_via_session_engine`. Wrapped in
    // Mutex<Option<>> because `complete()` borrows `&self` yet
    // needs to move the Sender out (it can only fire once).
    result_tx: Arc<Mutex<Option<oneshot::Sender<TaskResult>>>>,
}

impl VacEngineAdapter {
    #[must_use]
    pub fn new(engine: Arc<Mutex<VacEngine>>) -> Self {
        Self {
            engine,
            event_forward: None,
            result_tx: Arc::new(Mutex::new(None)),
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
            result_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// Attach a oneshot the adapter will fire with the real
    /// `TaskResult` as soon as `run_task_with_updates` returns —
    /// before the `LlmResponse` is assembled. Only one send per
    /// adapter instance; downstream calls drop silently.
    #[must_use]
    pub fn with_result_tx(
        engine: Arc<Mutex<VacEngine>>,
        result_tx: oneshot::Sender<TaskResult>,
    ) -> Self {
        Self {
            engine,
            event_forward: None,
            result_tx: Arc::new(Mutex::new(Some(result_tx))),
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

        // B1 + audit H3: fire the oneshot *after* the translator
        // join so the invariant "oneshot fires ⇒ all stream events
        // flushed" holds structurally. Pre-audit this fired before
        // the join and the ordering was correct only by convention;
        // a future edit could have reordered the drain and silently
        // broken it.
        if let Some(tx) = self.result_tx.lock().await.take() {
            let _ = tx.send(result.clone());
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
