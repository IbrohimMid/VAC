//! Submit orchestration — binds transcript, slash, compact, usage, and
//! LLM adapter into one durable submit lifecycle.
//!
//! Order of operations (transcript-before-query):
//!   1. Open transcript for session.
//!   2. Append `Accepted` row (fsynced) — durability checkpoint.
//!   3. If slash: dispatch, append `Slash` row, emit `SlashHandled` +
//!      `Finished`, done.
//!   4. Otherwise: run compact boundary; if hint ≠ Keep, append
//!      `CompactBoundary` row and emit `Compacted`.
//!   5. Append `LlmRequest` row, emit `LlmRequested`, call adapter.
//!   6. On success: append `LlmResponse` + `Finished`, emit
//!      `LlmChunk` (one shot) + `Finished`.
//!   7. On error: append `Aborted`, emit `Aborted`.

use std::sync::Arc;

use tokio::sync::mpsc;

use vac_core::policy_limits::{PolicyDecision, PolicyTracker, SubmitIntent};

use crate::compact::{CompactBoundary, CompactHint, CompactInput};
use crate::error::EngineResult;
use crate::event::{SubmitContext, SubmitEvent};
use crate::llm::{LlmAdapter, LlmRequest};
use crate::slash::SlashProcessor;
use crate::transcript::{TranscriptEntry, TranscriptKind, TranscriptWriter};
use crate::usage::{UsageSnapshot, UsageTracker};

/// Knobs the caller can pass to `submit_one` without constructing a
/// full compact input themselves.
#[derive(Clone)]
pub struct CompactConfig {
    pub message_count: usize,
    pub approx_tokens: u64,
    pub context_window_tokens: u64,
    pub max_budget_tokens: Option<u64>,
    pub max_turns: Option<u32>,
    /// C1 — optional PolicyTracker. When set, `submit_one` calls
    /// `check` before the LLM round-trip (deny → EngineError::Other),
    /// then `record_submit` + `record_tokens` after a successful
    /// response. Kept as Option so the 29 existing call sites that
    /// `CompactConfig::default()` continue to work unchanged.
    pub policy: Option<Arc<PolicyTracker>>,
    /// A.2 — composite gate the per-tool-use loop consults. When
    /// `None`, all tool calls pass freely (legacy behaviour). When
    /// set, each tool call in `LlmResponse.tool_calls` runs through
    /// `CompositeGate::check` before dispatch; Deny → the call is
    /// skipped, a `ToolResult` envelope with error is recorded,
    /// and the gate's trace entry drives the A1 bridge.
    pub gate: Option<Arc<crate::gate::CompositeGate>>,
    /// A.3 — dispatches tool-use blocks. When `None`, any
    /// non-empty `tool_calls` from the LLM response surface as
    /// failed `ToolResult`s via `UnsupportedDispatcher` so the
    /// transcript stays consistent without requiring every caller
    /// to provide a dispatcher today.
    pub dispatcher: Option<Arc<dyn crate::llm::ToolDispatcher>>,
    /// A.4 — auto-compaction trigger. When set, `submit_one`
    /// preemptively fires an extra compact boundary if the
    /// session's token usage is within `safety_margin` of the
    /// context window ceiling. CC fires at roughly
    /// `context_window - 13000`; VAC keeps the margin caller-
    /// configurable for heterogeneous providers.
    pub auto_compact: Option<AutoCompactConfig>,
}

/// A.4 — auto-compaction knobs. The in-session failure counter
/// lives on a caller-supplied `Arc<AtomicU8>` so the circuit
/// breaker state survives across submits; set to None to disable
/// the breaker.
#[derive(Clone)]
pub struct AutoCompactConfig {
    /// Effective context window for the provider/model in play.
    pub context_window_tokens: u64,
    /// How many tokens to keep as headroom above the ceiling. A
    /// conservative 13 000 matches CC's default; tighten when the
    /// operator knows the expected response size.
    pub safety_margin: u64,
    /// Consecutive-failure counter for the circuit breaker.
    /// After 3 strikes the breaker trips and the trigger
    /// disables itself for the session (no more auto-compact
    /// attempts; the normal compact boundary still runs).
    pub failure_counter: Option<Arc<std::sync::atomic::AtomicU8>>,
}

impl std::fmt::Debug for AutoCompactConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AutoCompactConfig")
            .field("context_window_tokens", &self.context_window_tokens)
            .field("safety_margin", &self.safety_margin)
            .field("failure_counter_attached", &self.failure_counter.is_some())
            .finish()
    }
}

impl Default for CompactConfig {
    fn default() -> Self {
        Self {
            message_count: 0,
            approx_tokens: 0,
            context_window_tokens: 200_000,
            max_budget_tokens: None,
            max_turns: None,
            policy: None,
            gate: None,
            dispatcher: None,
            auto_compact: None,
        }
    }
}

impl std::fmt::Debug for CompactConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompactConfig")
            .field("message_count", &self.message_count)
            .field("approx_tokens", &self.approx_tokens)
            .field("context_window_tokens", &self.context_window_tokens)
            .field("max_budget_tokens", &self.max_budget_tokens)
            .field("max_turns", &self.max_turns)
            .field("policy", &self.policy.as_ref().map(|_| "…"))
            .finish()
    }
}

fn emit(ch: &Option<mpsc::UnboundedSender<SubmitEvent>>, ev: SubmitEvent) {
    if let Some(tx) = ch {
        if let Err(e) = tx.send(ev) {
            tracing::trace!(target: "vac_session_engine", "event receiver dropped: {e}");
        }
    }
}

/// Post-Accepted body. Extracted so `submit_one` can wrap any error
/// with a best-effort `Aborted` transcript row + `Aborted` event — the
/// contract is: once `Accepted` is durable, every terminal path must
/// end in either `Finished` or `Aborted`.
#[allow(clippy::too_many_arguments)]
async fn submit_after_accepted(
    submit: &SubmitContext,
    handle: &crate::transcript::TranscriptHandle,
    transcript: &TranscriptWriter,
    slash: &SlashProcessor,
    compact: &dyn CompactBoundary,
    usage: &UsageTracker,
    llm: &dyn LlmAdapter,
    compact_cfg: &CompactConfig,
    events: &Option<mpsc::UnboundedSender<SubmitEvent>>,
) -> EngineResult<UsageSnapshot> {
    // Slash short-circuit.
    if let Some(inv) = slash.detect(&submit.input) {
        if let Some(result) = slash.dispatch(&inv).await? {
            let row = TranscriptEntry::new(
                submit.session_id,
                TranscriptKind::Slash,
                serde_json::json!({
                    "command": inv.command,
                    "args": inv.args,
                    "summary": result.summary,
                    "payload": result.payload,
                }),
            );
            transcript.append(handle, &row).await?;
            emit(
                events,
                SubmitEvent::SlashHandled {
                    command: inv.command.clone(),
                    payload: result.payload.clone(),
                },
            );
            let snap = usage.snapshot();
            let finished = TranscriptEntry::new(
                submit.session_id,
                TranscriptKind::Finished,
                serde_json::json!({ "via": "slash", "usage": snap }),
            );
            transcript.append(handle, &finished).await?;
            emit(events, SubmitEvent::Finished { usage: snap });
            return Ok(snap);
        }
        // Unknown slash → fall through to LLM path.
    }

    // Compact boundary.
    let hint = compact
        .decide(&CompactInput {
            message_count: compact_cfg.message_count,
            approx_tokens: compact_cfg.approx_tokens,
            context_window_tokens: compact_cfg.context_window_tokens,
        })
        .await?;
    let compact_payload = match &hint {
        CompactHint::Keep => None,
        CompactHint::DropOldest { n } if *n == 0 => None,
        CompactHint::Summarise { n, .. } if *n == 0 => None,
        CompactHint::DropOldest { n } => Some((
            compact_cfg.message_count.saturating_sub(*n),
            *n,
            serde_json::json!({ "kind": "drop_oldest", "n": n }),
        )),
        CompactHint::Summarise { n, summary } => Some((
            compact_cfg.message_count.saturating_sub(*n).saturating_add(1),
            *n,
            serde_json::json!({ "kind": "summarise", "n": n, "summary": summary }),
        )),
    };
    if let Some((kept, dropped, payload)) = compact_payload {
        let row = TranscriptEntry::new(
            submit.session_id,
            TranscriptKind::CompactBoundary,
            payload,
        );
        transcript.append(handle, &row).await?;
        emit(events, SubmitEvent::Compacted { kept, dropped });
    }

    if let Some(budget) = compact_cfg.max_budget_tokens {
        let used = usage.snapshot().total_tokens();
        if used >= budget {
            // C1 partial — emit on the policy target so the TUI A1
            // tracing bridge surfaces the denial as a critical
            // banner + activity row. Full PolicyTracker wiring
            // (hourly submit cap, per-call token cap) still needs
            // a tracker instance threaded through the engine; this
            // covers the existing budget gate without refactoring
            // all 29 submit_one call sites.
            tracing::error!(
                target: "vac_core::policy_limits",
                rule = "budget_exceeded",
                reason = %format!("tokens_used={used} >= budget={budget}"),
                "submit denied: session token budget exhausted",
            );
            return Err(crate::error::EngineError::BudgetExceeded {
                tokens_used: used,
                budget,
            });
        }
    }

    // A.4 — auto-compaction preemption. Check total tokens
    // consumed so far against the configured ceiling; if within
    // the safety margin, force an extra compact attempt. Circuit
    // breaker trips after 3 consecutive failures.
    if let Some(auto) = compact_cfg.auto_compact.as_ref() {
        let tripped = auto
            .failure_counter
            .as_ref()
            .map(|c| c.load(std::sync::atomic::Ordering::SeqCst) >= 3)
            .unwrap_or(false);
        if !tripped {
            let used = usage.snapshot().total_tokens();
            let ceiling = auto
                .context_window_tokens
                .saturating_sub(auto.safety_margin);
            if used >= ceiling {
                let forced = CompactInput {
                    message_count: compact_cfg.message_count,
                    approx_tokens: used,
                    context_window_tokens: auto.context_window_tokens,
                };
                match compact.decide(&forced).await {
                    Ok(hint) => {
                        if let Some(c) = auto.failure_counter.as_ref() {
                            c.store(0, std::sync::atomic::Ordering::SeqCst);
                        }
                        if let CompactHint::DropOldest { n } | CompactHint::Summarise { n, .. } =
                            &hint
                        {
                            if *n > 0 {
                                let row = TranscriptEntry::new(
                                    submit.session_id,
                                    TranscriptKind::CompactBoundary,
                                    serde_json::json!({
                                        "trigger": "auto_compact",
                                        "used": used,
                                        "ceiling": ceiling,
                                        "hint": format!("{hint:?}"),
                                    }),
                                );
                                transcript.append(handle, &row).await?;
                                emit(
                                    events,
                                    SubmitEvent::Compacted {
                                        kept: compact_cfg.message_count.saturating_sub(*n),
                                        dropped: *n,
                                    },
                                );
                            }
                        }
                    }
                    Err(e) => {
                        if let Some(c) = auto.failure_counter.as_ref() {
                            let prev = c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            if prev + 1 >= 3 {
                                tracing::warn!(
                                    target: "vac_session_engine::auto_compact",
                                    strikes = prev + 1,
                                    last_error = %e,
                                    "auto-compact circuit breaker tripped — disabling for session",
                                );
                            }
                        }
                        tracing::warn!(
                            target: "vac_session_engine::auto_compact",
                            error = %e,
                            "auto-compact attempt failed",
                        );
                    }
                }
            }
        }
    }

    // C1 — PolicyTracker gate. Deny here surfaces as a
    // tracing::warn on target `vac_core::policy_limits` (emitted
    // by the tracker itself) → A1 bridge renders critical banner.
    if let Some(policy) = compact_cfg.policy.as_ref() {
        let decision = policy
            .check(&SubmitIntent {
                tool: None,
                additional_tokens: 0,
            })
            .await;
        if let PolicyDecision::Deny(reason) = decision {
            return Err(crate::error::EngineError::Other(reason));
        }
    }

    // LLM round-trip.
    let req = LlmRequest {
        prompt: submit.input.clone(),
        context: Vec::new(),
    };
    let req_row = TranscriptEntry::new(
        submit.session_id,
        TranscriptKind::LlmRequest,
        serde_json::json!({ "prompt": req.prompt }),
    );
    transcript.append(handle, &req_row).await?;

    let resp = llm.complete(req).await?;
    if let Some(policy) = compact_cfg.policy.as_ref() {
        policy.record_submit().await;
        policy
            .record_tokens(resp.input_tokens.saturating_add(resp.output_tokens))
            .await;
    }
    emit(
        events,
        SubmitEvent::LlmRequested {
            provider: resp.provider.clone(),
            model: resp.model.clone(),
        },
    );
    usage.add_input_tokens(resp.input_tokens);
    usage.add_output_tokens(resp.output_tokens);
    emit(
        events,
        SubmitEvent::LlmChunk {
            text: resp.content.clone(),
        },
    );
    let resp_row = TranscriptEntry::new(
        submit.session_id,
        TranscriptKind::LlmResponse,
        serde_json::json!({
            "provider": resp.provider,
            "model": resp.model,
            "content": resp.content,
            "input_tokens": resp.input_tokens,
            "output_tokens": resp.output_tokens,
        }),
    );
    transcript.append(handle, &resp_row).await?;

    // A.3 — tool-use block iteration. For each tool call the LLM
    // emitted, run CompositeGate; Allow → dispatch → ToolResult.
    // Deny → synthesise a failure envelope so the transcript stays
    // consistent. Today's EchoAdapter emits zero tool calls, so
    // this loop is a no-op for the legacy path.
    for call in &resp.tool_calls {
        emit(
            events,
            SubmitEvent::ToolRequested {
                id: call.id.clone(),
                name: call.name.clone(),
                arguments: call.arguments.clone(),
            },
        );
        let gate_decision = if let Some(gate) = compact_cfg.gate.as_ref() {
            let mut ctx = crate::gate::ToolCheckCtx::new(
                call.name.clone(),
                submit.session_id,
            )
            .with_arguments(call.arguments.clone())
            .with_estimated_tokens(call.estimated_tokens);
            if let Some(reason) = call.reason.as_ref() {
                ctx = ctx.with_reason(reason.clone());
            }
            gate.check(&ctx).await
        } else {
            crate::gate::GateDecision::allow()
        };
        let envelope: vac_tool_core::ToolResultEnvelope = match gate_decision {
            crate::gate::GateDecision::Allow { .. } => {
                let dispatcher = compact_cfg.dispatcher.clone().unwrap_or_else(|| {
                    Arc::new(crate::llm::UnsupportedDispatcher)
                        as Arc<dyn crate::llm::ToolDispatcher>
                });
                match dispatcher.dispatch(call).await {
                    Ok(env) => env,
                    Err(e) => vac_tool_core::ToolResultEnvelope::error(
                        format!("dispatch error for '{}'", call.name),
                        e.to_string(),
                    ),
                }
            }
            crate::gate::GateDecision::Deny { reason }
            | crate::gate::GateDecision::NeedsApproval { reason } => {
                vac_tool_core::ToolResultEnvelope::error(
                    format!("tool '{}' blocked by gate", call.name),
                    reason,
                )
            }
        };
        emit(
            events,
            SubmitEvent::ToolResult {
                id: call.id.clone(),
                name: call.name.clone(),
                payload: envelope,
            },
        );
    }

    let snap = usage.snapshot();
    let finished = TranscriptEntry::new(
        submit.session_id,
        TranscriptKind::Finished,
        serde_json::json!({ "via": "llm", "usage": snap }),
    );
    transcript.append(handle, &finished).await?;
    emit(events, SubmitEvent::Finished { usage: snap });
    Ok(snap)
}

/// Drive a single submit. Returns the final usage snapshot.
#[allow(clippy::too_many_arguments)]
#[tracing::instrument(
    target = "vac_session_engine",
    name = "submit_one",
    skip_all,
    fields(session_id = %submit.session_id, input_len = submit.input.len()),
)]
pub async fn submit_one(
    submit: SubmitContext,
    transcript: &TranscriptWriter,
    slash: &SlashProcessor,
    compact: &dyn CompactBoundary,
    usage: &UsageTracker,
    llm: &dyn LlmAdapter,
    compact_cfg: CompactConfig,
    events: Option<mpsc::UnboundedSender<SubmitEvent>>,
) -> EngineResult<UsageSnapshot> {
    let handle = transcript.open(submit.session_id).await?;

    // 1. Accepted — durability checkpoint.
    let accepted = TranscriptEntry::new(
        submit.session_id,
        TranscriptKind::Accepted,
        serde_json::json!({
            "input": submit.input,
            "submitted_at": submit.submitted_at,
            "metadata": submit.metadata,
        }),
    );
    let accepted_id = accepted.id;
    transcript.append(&handle, &accepted).await?;
    emit(&events, SubmitEvent::Accepted { entry_id: accepted_id });

    // 2. Everything after Accepted must end in Finished or Aborted.
    // Wrap the body so any `?` error path still gets a best-effort
    // Aborted row + event before propagating the error up.
    match submit_after_accepted(
        &submit,
        &handle,
        transcript,
        slash,
        compact,
        usage,
        llm,
        &compact_cfg,
        &events,
    )
    .await
    {
        Ok(snap) => Ok(snap),
        Err(e) => {
            let reason = e.to_string();
            let kind = match &e {
                crate::error::EngineError::BudgetExceeded { .. } => "budget_exceeded",
                crate::error::EngineError::Cancelled => "cancelled",
                _ => "error",
            };
            // Best-effort: if the transcript itself is the thing that
            // failed, this append will fail too. Log and continue so
            // the original error is what surfaces to the caller.
            let aborted = TranscriptEntry::new(
                submit.session_id,
                TranscriptKind::Aborted,
                serde_json::json!({ "reason": reason, "kind": kind }),
            );
            if let Err(append_err) = transcript.append(&handle, &aborted).await {
                tracing::warn!(
                    target: "vac_session_engine",
                    "aborted row append failed after submit error: {append_err}",
                );
            }
            emit(
                &events,
                SubmitEvent::Aborted {
                    reason: reason.clone(),
                },
            );
            Err(e)
        }
    }
}

// Helper wires args through; real work lives above in
// submit_after_accepted. `usage` is passed separately because it is
// accumulated into the Finished row.
#[allow(dead_code)]
async fn _doc_helper_signature_matches() {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compact::TrivialCompactBoundary;
    use crate::llm::EchoAdapter;
    use crate::slash::{SlashCommand, SlashResult};
    use async_trait::async_trait;
    use std::sync::Arc;
    use uuid::Uuid;

    fn ctx(input: &str) -> (tempfile::TempDir, SubmitContext, TranscriptWriter) {
        let tmp = tempfile::tempdir().unwrap();
        let w = TranscriptWriter::new(tmp.path().to_path_buf());
        let c = SubmitContext::new(Uuid::new_v4(), input);
        (tmp, c, w)
    }

    #[tokio::test]
    async fn submit_one_writes_accepted_before_llm_response() {
        let (_t, c, w) = ctx("hi");
        let sid = c.session_id;
        let slash = SlashProcessor::new();
        let compact = TrivialCompactBoundary::default();
        let usage = UsageTracker::new();
        let llm = EchoAdapter;
        let snap = submit_one(
            c,
            &w,
            &slash,
            &compact,
            &usage,
            &llm,
            CompactConfig::default(),
            None,
        )
        .await
        .unwrap();
        assert!(snap.output_tokens >= 1);
        let rows = w.read(sid).await.unwrap();
        let kinds: Vec<_> = rows.iter().map(|r| r.kind).collect();
        assert_eq!(kinds[0], TranscriptKind::Accepted);
        assert!(kinds.contains(&TranscriptKind::LlmRequest));
        assert!(kinds.contains(&TranscriptKind::LlmResponse));
        assert_eq!(*kinds.last().unwrap(), TranscriptKind::Finished);
    }

    struct Noop;
    #[async_trait]
    impl SlashCommand for Noop {
        fn name(&self) -> &str {
            "noop"
        }
        fn description(&self) -> &str {
            "noop"
        }
        async fn handle(&self, _: &str) -> EngineResult<SlashResult> {
            Ok(SlashResult {
                summary: "ok".into(),
                payload: serde_json::json!({ "noop": true }),
            })
        }
    }

    #[tokio::test]
    async fn slash_command_short_circuits_llm() {
        let (_t, c, w) = ctx("/noop");
        let sid = c.session_id;
        let mut slash = SlashProcessor::new();
        slash.register(Arc::new(Noop));
        let compact = TrivialCompactBoundary::default();
        let usage = UsageTracker::new();
        let llm = EchoAdapter;
        submit_one(
            c,
            &w,
            &slash,
            &compact,
            &usage,
            &llm,
            CompactConfig::default(),
            None,
        )
        .await
        .unwrap();
        let rows = w.read(sid).await.unwrap();
        let kinds: Vec<_> = rows.iter().map(|r| r.kind).collect();
        assert_eq!(kinds[0], TranscriptKind::Accepted);
        assert!(kinds.contains(&TranscriptKind::Slash));
        assert!(!kinds.contains(&TranscriptKind::LlmRequest));
        assert_eq!(*kinds.last().unwrap(), TranscriptKind::Finished);
    }

    #[tokio::test]
    async fn event_stream_matches_transcript_kinds() {
        let (_t, c, w) = ctx("hello");
        let slash = SlashProcessor::new();
        let compact = TrivialCompactBoundary::default();
        let usage = UsageTracker::new();
        let llm = EchoAdapter;
        let (tx, mut rx) = mpsc::unbounded_channel();
        submit_one(
            c,
            &w,
            &slash,
            &compact,
            &usage,
            &llm,
            CompactConfig::default(),
            Some(tx),
        )
        .await
        .unwrap();
        let mut labels = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            labels.push(ev.label());
        }
        assert_eq!(labels.first().copied(), Some("accepted"));
        assert_eq!(labels.last().copied(), Some("finished"));
        assert!(labels.contains(&"llm.request"));
        assert!(labels.contains(&"llm.chunk"));
    }

    #[tokio::test]
    async fn crash_after_accepted_leaves_pending_signature() {
        // Simulate crash by writing only the Accepted row and not
        // running the rest of submit_one. `last_pending_submit` should
        // flag the session for resume.
        let tmp = tempfile::tempdir().unwrap();
        let w = TranscriptWriter::new(tmp.path().to_path_buf());
        let sid = Uuid::new_v4();
        let h = w.open(sid).await.unwrap();
        let acc = TranscriptEntry::new(
            sid,
            TranscriptKind::Accepted,
            serde_json::json!({ "input": "crashy" }),
        );
        w.append(&h, &acc).await.unwrap();
        let pending = w.last_pending_submit(sid).await.unwrap();
        assert_eq!(pending, Some(acc.id));
    }
}
