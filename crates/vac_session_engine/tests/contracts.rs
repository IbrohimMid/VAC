//! Contract tests — durability, resume, slash.
//!
//! These exercise the engine from the outside, via the public API only,
//! to catch regressions in the invariants drivers rely on:
//! - Accepted row is persisted (fsynced) before the LLM is contacted.
//! - A crash between Accepted and Finished leaves a pending signature
//!   that `last_pending_submit` reports.
//! - Slash commands never hit the LLM adapter.
//! - Event stream and transcript stay in sync.

use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use uuid::Uuid;
use vac_session_engine::{
    CompactBoundary, CompactConfig, CompactHint, CompactInput, EngineError, EngineResult,
    LlmAdapter, LlmRequest, LlmResponse, SlashCommand, SlashProcessor, SubmitContext,
    SubmitEvent, TranscriptKind, TranscriptWriter, TrivialCompactBoundary, UsageTracker,
    submit_one,
};

/// LLM adapter that records whether it was called and asserts the
/// Accepted row already exists on disk at call time.
struct RecordingAdapter {
    called: AtomicBool,
    transcript_path: std::path::PathBuf,
    session_id: Uuid,
}

#[async_trait]
impl LlmAdapter for RecordingAdapter {
    async fn complete(&self, req: LlmRequest) -> EngineResult<LlmResponse> {
        self.called.store(true, Ordering::SeqCst);
        // By contract: when the adapter is invoked, an Accepted row for
        // this session must already be durable.
        let content = tokio::fs::read_to_string(&self.transcript_path)
            .await
            .expect("transcript file exists at LLM call time");
        let mut saw_accepted = false;
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            if v["kind"] == "accepted" && v["session_id"] == self.session_id.to_string() {
                saw_accepted = true;
            }
        }
        assert!(saw_accepted, "accepted row missing when adapter invoked");
        Ok(LlmResponse {
            provider: "rec".into(),
            model: "rec-1".into(),
            content: format!("ack: {}", req.prompt),
            input_tokens: 1,
            output_tokens: 1,
        tool_calls: Vec::new(),
        })
    }
}

/// Slash command that bumps a counter; verifies slashes don't touch
/// the LLM.
struct Counter {
    hits: Arc<AtomicUsize>,
}

#[async_trait]
impl SlashCommand for Counter {
    fn name(&self) -> &str {
        "ping"
    }
    fn description(&self) -> &str {
        "test"
    }
    async fn handle(
        &self,
        _args: &str,
    ) -> EngineResult<vac_session_engine::slash::SlashResult> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        Ok(vac_session_engine::slash::SlashResult {
            summary: "pong".into(),
            payload: serde_json::json!({ "pong": true }),
        })
    }
}

#[tokio::test]
async fn accepted_row_durable_before_llm_contact() {
    let tmp = tempfile::tempdir().unwrap();
    let writer = TranscriptWriter::new(tmp.path().to_path_buf());
    let ctx = SubmitContext::new(Uuid::new_v4(), "hi");
    let sid = ctx.session_id;
    let adapter = RecordingAdapter {
        called: AtomicBool::new(false),
        transcript_path: writer.sessions_dir().join(format!("{sid}.jsonl")),
        session_id: sid,
    };
    let slash = SlashProcessor::new();
    let compact = TrivialCompactBoundary::default();
    let usage = UsageTracker::new();
    submit_one(
        ctx,
        &writer,
        &slash,
        &compact,
        &usage,
        &adapter,
        CompactConfig::default(),
        None,
    )
    .await
    .unwrap();
    assert!(adapter.called.load(Ordering::SeqCst));
}

#[tokio::test]
async fn resume_detects_only_crashed_sessions() {
    let tmp = tempfile::tempdir().unwrap();
    let writer = TranscriptWriter::new(tmp.path().to_path_buf());

    // Clean session: Accepted + Finished → no pending.
    let sid_clean = Uuid::new_v4();
    let ctx = SubmitContext::new(sid_clean, "clean");
    submit_one(
        ctx,
        &writer,
        &SlashProcessor::new(),
        &TrivialCompactBoundary::default(),
        &UsageTracker::new(),
        &vac_session_engine::EchoAdapter,
        CompactConfig::default(),
        None,
    )
    .await
    .unwrap();
    assert!(writer.last_pending_submit(sid_clean).await.unwrap().is_none());

    // Crashed session: Accepted only.
    let sid_crash = Uuid::new_v4();
    let handle = writer.open(sid_crash).await.unwrap();
    let entry = vac_session_engine::TranscriptEntry::new(
        sid_crash,
        TranscriptKind::Accepted,
        serde_json::json!({ "input": "oops" }),
    );
    let eid = entry.id;
    writer.append(&handle, &entry).await.unwrap();
    assert_eq!(
        writer.last_pending_submit(sid_crash).await.unwrap(),
        Some(eid)
    );
}

#[tokio::test]
async fn slash_never_invokes_llm() {
    let tmp = tempfile::tempdir().unwrap();
    let writer = TranscriptWriter::new(tmp.path().to_path_buf());
    let adapter = RecordingAdapter {
        called: AtomicBool::new(false),
        transcript_path: writer.sessions_dir().join("ignored.jsonl"),
        session_id: Uuid::nil(),
    };
    let hits = Arc::new(AtomicUsize::new(0));
    let mut slash = SlashProcessor::new();
    slash.register(Arc::new(Counter { hits: hits.clone() }));

    submit_one(
        SubmitContext::new(Uuid::new_v4(), "/ping"),
        &writer,
        &slash,
        &TrivialCompactBoundary::default(),
        &UsageTracker::new(),
        &adapter,
        CompactConfig::default(),
        None,
    )
    .await
    .unwrap();

    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert!(!adapter.called.load(Ordering::SeqCst));
}

#[tokio::test]
async fn event_stream_and_transcript_match_for_llm_path() {
    let tmp = tempfile::tempdir().unwrap();
    let writer = TranscriptWriter::new(tmp.path().to_path_buf());
    let ctx = SubmitContext::new(Uuid::new_v4(), "hello");
    let sid = ctx.session_id;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    submit_one(
        ctx,
        &writer,
        &SlashProcessor::new(),
        &TrivialCompactBoundary::default(),
        &UsageTracker::new(),
        &vac_session_engine::EchoAdapter,
        CompactConfig::default(),
        Some(tx),
    )
    .await
    .unwrap();

    let mut events = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        events.push(ev);
    }
    // Must start with Accepted, end with Finished.
    assert!(matches!(events.first(), Some(SubmitEvent::Accepted { .. })));
    assert!(matches!(events.last(), Some(SubmitEvent::Finished { .. })));

    let rows = writer.read(sid).await.unwrap();
    assert_eq!(rows.first().map(|r| r.kind), Some(TranscriptKind::Accepted));
    assert_eq!(rows.last().map(|r| r.kind), Some(TranscriptKind::Finished));
}

/// Compact boundary that always requests a drop — lets us verify the
/// Compacted event + CompactBoundary row actually fire via submit_one.
struct AlwaysDrop;
#[async_trait]
impl CompactBoundary for AlwaysDrop {
    async fn decide(&self, _input: &CompactInput) -> EngineResult<CompactHint> {
        Ok(CompactHint::DropOldest { n: 3 })
    }
}

#[tokio::test]
async fn compact_boundary_fires_through_submit_one() {
    let tmp = tempfile::tempdir().unwrap();
    let writer = TranscriptWriter::new(tmp.path().to_path_buf());
    let ctx = SubmitContext::new(Uuid::new_v4(), "hi");
    let sid = ctx.session_id;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    submit_one(
        ctx,
        &writer,
        &SlashProcessor::new(),
        &AlwaysDrop,
        &UsageTracker::new(),
        &vac_session_engine::EchoAdapter,
        CompactConfig {
            message_count: 10,
            approx_tokens: 0,
            context_window_tokens: 200_000,
            ..Default::default()
        },
        Some(tx),
    )
    .await
    .unwrap();

    let mut events = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        events.push(ev);
    }
    assert!(
        events
            .iter()
            .any(|e| matches!(e, SubmitEvent::Compacted { dropped: 3, .. })),
        "Compacted event missing: {events:?}",
    );
    let rows = writer.read(sid).await.unwrap();
    assert!(rows.iter().any(|r| r.kind == TranscriptKind::CompactBoundary));
}

/// Adapter that always fails — used to exercise the Aborted path.
struct FailingAdapter;
#[async_trait]
impl LlmAdapter for FailingAdapter {
    async fn complete(&self, _req: LlmRequest) -> EngineResult<LlmResponse> {
        Err(EngineError::Other("boom".into()))
    }
}

#[tokio::test]
async fn aborted_path_writes_row_and_event_and_propagates_error() {
    let tmp = tempfile::tempdir().unwrap();
    let writer = TranscriptWriter::new(tmp.path().to_path_buf());
    let ctx = SubmitContext::new(Uuid::new_v4(), "hi");
    let sid = ctx.session_id;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let err = submit_one(
        ctx,
        &writer,
        &SlashProcessor::new(),
        &TrivialCompactBoundary::default(),
        &UsageTracker::new(),
        &FailingAdapter,
        CompactConfig::default(),
        Some(tx),
    )
    .await
    .unwrap_err();
    assert!(format!("{err}").contains("boom"));

    let mut labels = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        labels.push(ev.label());
    }
    assert_eq!(labels.first().copied(), Some("accepted"));
    assert_eq!(labels.last().copied(), Some("aborted"));

    let rows = writer.read(sid).await.unwrap();
    assert_eq!(rows.first().map(|r| r.kind), Some(TranscriptKind::Accepted));
    assert_eq!(rows.last().map(|r| r.kind), Some(TranscriptKind::Aborted));
}

/// Adapter that returns error AFTER the LlmRequest row has been
/// written — verifies Aborted still fires (post-Accepted invariant).
#[tokio::test]
async fn llm_error_after_request_row_still_writes_aborted() {
    let tmp = tempfile::tempdir().unwrap();
    let writer = TranscriptWriter::new(tmp.path().to_path_buf());
    let ctx = SubmitContext::new(Uuid::new_v4(), "hi");
    let sid = ctx.session_id;
    submit_one(
        ctx,
        &writer,
        &SlashProcessor::new(),
        &TrivialCompactBoundary::default(),
        &UsageTracker::new(),
        &FailingAdapter,
        CompactConfig::default(),
        None,
    )
    .await
    .unwrap_err();
    let rows = writer.read(sid).await.unwrap();
    let kinds: Vec<_> = rows.iter().map(|r| r.kind).collect();
    // Accepted → LlmRequest → Aborted (no LlmResponse).
    assert_eq!(kinds[0], TranscriptKind::Accepted);
    assert!(kinds.contains(&TranscriptKind::LlmRequest));
    assert!(!kinds.contains(&TranscriptKind::LlmResponse));
    assert_eq!(*kinds.last().unwrap(), TranscriptKind::Aborted);
}

#[tokio::test]
async fn budget_gate_aborts_submit_when_exceeded() {
    let tmp = tempfile::tempdir().unwrap();
    let writer = TranscriptWriter::new(tmp.path().to_path_buf());
    let ctx = SubmitContext::new(Uuid::new_v4(), "hi");
    let sid = ctx.session_id;

    let usage = UsageTracker::new();
    usage.add_input_tokens(10); // used 10
    
    let mut compact_cfg = CompactConfig::default();
    compact_cfg.max_budget_tokens = Some(10); // budget 10 -> exceeded

    let err = submit_one(
        ctx,
        &writer,
        &SlashProcessor::new(),
        &TrivialCompactBoundary::default(),
        &usage,
        &vac_session_engine::EchoAdapter,
        compact_cfg,
        None,
    )
    .await
    .unwrap_err();

    assert!(matches!(err, EngineError::BudgetExceeded { .. }));

    let rows = writer.read(sid).await.unwrap();
    let kinds: Vec<_> = rows.iter().map(|r| r.kind).collect();
    
    // Should be Accepted -> Aborted (never reached LLMRequest)
    assert_eq!(kinds[0], TranscriptKind::Accepted);
    assert_eq!(*kinds.last().unwrap(), TranscriptKind::Aborted);
    assert!(!kinds.contains(&TranscriptKind::LlmRequest));
    
    let aborted = rows.last().unwrap();
    assert_eq!(aborted.content["kind"], "budget_exceeded");
}

/// C1 — PolicyTracker gate blocks the second submit when
/// `max_submits_per_hour=1`.
#[tokio::test]
async fn policy_tracker_caps_submits_per_hour() {
    use std::sync::Arc;
    use vac_core::policy_limits::{PolicyLimits, PolicyTracker};

    let tmp = tempfile::tempdir().unwrap();
    let writer = TranscriptWriter::new(tmp.path().to_path_buf());
    let tracker = Arc::new(PolicyTracker::new(PolicyLimits {
        max_submits_per_hour: Some(1),
        ..Default::default()
    }));

    let cfg = || CompactConfig {
        policy: Some(tracker.clone()),
        ..Default::default()
    };

    // First submit succeeds.
    submit_one(
        SubmitContext::new(Uuid::new_v4(), "one"),
        &writer,
        &SlashProcessor::new(),
        &TrivialCompactBoundary::default(),
        &UsageTracker::new(),
        &vac_session_engine::EchoAdapter,
        cfg(),
        None,
    )
    .await
    .expect("first submit under cap");

    // Second submit denied.
    let err = submit_one(
        SubmitContext::new(Uuid::new_v4(), "two"),
        &writer,
        &SlashProcessor::new(),
        &TrivialCompactBoundary::default(),
        &UsageTracker::new(),
        &vac_session_engine::EchoAdapter,
        cfg(),
        None,
    )
    .await
    .unwrap_err();

    match err {
        EngineError::Other(reason) => {
            assert!(reason.contains("max_submits_per_hour"), "{reason}");
        }
        other => panic!("expected EngineError::Other, got {other:?}"),
    }
}

// A.3 — tool dispatch through CompositeGate.

struct ToolCallingAdapter;
#[async_trait]
impl LlmAdapter for ToolCallingAdapter {
    async fn complete(&self, _req: LlmRequest) -> EngineResult<vac_session_engine::LlmResponse> {
        Ok(vac_session_engine::LlmResponse {
            provider: "tc".into(),
            model: "tc-1".into(),
            content: "running tool".into(),
            input_tokens: 1,
            output_tokens: 1,
            tool_calls: vec![vac_session_engine::llm::ToolCallRequest {
                id: "call-1".into(),
                name: "echo".into(),
                arguments: serde_json::json!({"msg": "hi"}),
                reason: Some("test".into()),
                estimated_tokens: 10,
            }],
        })
    }
}

#[derive(Debug)]
struct RecordingDispatcher {
    calls: Arc<AtomicUsize>,
}
#[async_trait]
impl vac_session_engine::llm::ToolDispatcher for RecordingDispatcher {
    async fn dispatch(
        &self,
        _call: &vac_session_engine::llm::ToolCallRequest,
    ) -> EngineResult<vac_tool_core::ToolResultEnvelope> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(vac_tool_core::ToolResultEnvelope::ok(
            "echoed",
            serde_json::json!({"ok": true}),
        ))
    }
}

#[tokio::test]
async fn tool_dispatch_runs_through_composite_gate_allow_path() {
    let tmp = tempfile::tempdir().unwrap();
    let writer = TranscriptWriter::new(tmp.path().to_path_buf());
    let calls = Arc::new(AtomicUsize::new(0));
    let dispatcher = Arc::new(RecordingDispatcher {
        calls: calls.clone(),
    });
    let gate = Arc::new(vac_session_engine::CompositeGate::new());

    let cfg = CompactConfig {
        gate: Some(gate.clone()),
        dispatcher: Some(dispatcher.clone() as Arc<dyn vac_session_engine::llm::ToolDispatcher>),
        ..Default::default()
    };

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    submit_one(
        SubmitContext::new(Uuid::new_v4(), "hi"),
        &writer,
        &SlashProcessor::new(),
        &TrivialCompactBoundary::default(),
        &UsageTracker::new(),
        &ToolCallingAdapter,
        cfg,
        Some(tx),
    )
    .await
    .unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 1, "dispatcher invoked once");

    let labels: Vec<&'static str> = {
        let mut v = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            v.push(ev.label());
        }
        v
    };
    assert!(labels.contains(&"tool.request"), "ToolRequested emitted: {labels:?}");
    assert!(labels.contains(&"tool.result"), "ToolResult emitted: {labels:?}");
    assert_eq!(labels.last().copied(), Some("finished"));
}
