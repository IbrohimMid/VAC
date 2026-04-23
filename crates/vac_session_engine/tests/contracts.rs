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
    CompactConfig, EngineResult, LlmAdapter, LlmRequest, LlmResponse, SlashCommand,
    SlashProcessor, SubmitContext, SubmitEvent, TranscriptKind, TranscriptWriter,
    TrivialCompactBoundary, UsageTracker, submit_one,
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
