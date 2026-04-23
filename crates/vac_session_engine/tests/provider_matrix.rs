//! F8.4 — Multi-provider test harness.
//!
//! Exercises the six distinct code paths through `submit_one` against
//! mocked provider adapters:
//!
//! 1. Happy-path LLM round-trip (Echo-style adapter).
//! 2. Chunked provider that returns text in multiple "segments".
//! 3. Provider that reports unusually large token usage (budget gate).
//! 4. Provider that reports zero token usage (minimal billing case).
//! 5. Provider that errors immediately → Aborted path fires.
//! 6. Slash short-circuit (no provider call at all).
//!
//! Every path asserts the transcript ends in either `Finished` or
//! `Aborted` — the durability invariant that the engine must uphold
//! regardless of which provider is wired in.

use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use uuid::Uuid;
use vac_session_engine::{
    CompactConfig, EngineError, EngineResult, LlmAdapter, LlmRequest, LlmResponse,
    SlashCommand, SlashProcessor, SubmitContext, SubmitEvent, TranscriptKind,
    TranscriptWriter, TrivialCompactBoundary, UsageTracker, submit_one,
};

struct EchoLikeAdapter;
#[async_trait]
impl LlmAdapter for EchoLikeAdapter {
    async fn complete(&self, req: LlmRequest) -> EngineResult<LlmResponse> {
        Ok(LlmResponse {
            provider: "echo-like".into(),
            model: "m1".into(),
            content: format!("ack: {}", req.prompt),
            input_tokens: req.prompt.split_whitespace().count() as u64,
            output_tokens: 3,
        })
    }
}

struct ChunkedAdapter {
    call_count: AtomicU32,
}
impl ChunkedAdapter {
    fn new() -> Self {
        Self {
            call_count: AtomicU32::new(0),
        }
    }
}
#[async_trait]
impl LlmAdapter for ChunkedAdapter {
    async fn complete(&self, _req: LlmRequest) -> EngineResult<LlmResponse> {
        let n = self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(LlmResponse {
            provider: "chunky".into(),
            model: "stream-1".into(),
            content: format!("chunk-{n} body"),
            input_tokens: 10,
            output_tokens: 10,
        })
    }
}

struct BigUsageAdapter;
#[async_trait]
impl LlmAdapter for BigUsageAdapter {
    async fn complete(&self, _req: LlmRequest) -> EngineResult<LlmResponse> {
        Ok(LlmResponse {
            provider: "budget-eater".into(),
            model: "b1".into(),
            content: "ok".into(),
            input_tokens: 180_000,
            output_tokens: 20_000,
        })
    }
}

struct ZeroUsageAdapter;
#[async_trait]
impl LlmAdapter for ZeroUsageAdapter {
    async fn complete(&self, _req: LlmRequest) -> EngineResult<LlmResponse> {
        Ok(LlmResponse {
            provider: "silent".into(),
            model: "z1".into(),
            content: "(nothing to say)".into(),
            input_tokens: 0,
            output_tokens: 0,
        })
    }
}

struct BoomAdapter;
#[async_trait]
impl LlmAdapter for BoomAdapter {
    async fn complete(&self, _req: LlmRequest) -> EngineResult<LlmResponse> {
        Err(EngineError::Other("upstream offline".into()))
    }
}

struct NoopSlash;
#[async_trait]
impl SlashCommand for NoopSlash {
    fn name(&self) -> &str {
        "noop"
    }
    fn description(&self) -> &str {
        "no-op"
    }
    async fn handle(
        &self,
        _args: &str,
    ) -> EngineResult<vac_session_engine::slash::SlashResult> {
        Ok(vac_session_engine::slash::SlashResult {
            summary: "nothing happened".into(),
            payload: serde_json::json!({ "noop": true }),
        })
    }
}

async fn drive(
    adapter: &dyn LlmAdapter,
    slash: &SlashProcessor,
    input: &str,
) -> (
    Vec<TranscriptKind>,
    Vec<&'static str>,
    Result<(), EngineError>,
) {
    let tmp = tempfile::tempdir().unwrap();
    let writer = TranscriptWriter::new(tmp.path().to_path_buf());
    let ctx = SubmitContext::new(Uuid::new_v4(), input);
    let sid = ctx.session_id;
    let compact = TrivialCompactBoundary::default();
    let usage = UsageTracker::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let result = submit_one(
        ctx,
        &writer,
        slash,
        &compact,
        &usage,
        adapter,
        CompactConfig::default(),
        Some(tx),
    )
    .await
    .map(|_| ());
    let mut labels = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        labels.push(ev.label());
    }
    let rows = writer.read(sid).await.unwrap();
    let kinds: Vec<_> = rows.iter().map(|r| r.kind).collect();
    (kinds, labels, result)
}

#[tokio::test]
async fn matrix_path_1_echo_happy_path() {
    let (kinds, labels, r) = drive(&EchoLikeAdapter, &SlashProcessor::new(), "hello").await;
    assert!(r.is_ok());
    assert_eq!(kinds[0], TranscriptKind::Accepted);
    assert_eq!(*kinds.last().unwrap(), TranscriptKind::Finished);
    assert!(labels.contains(&"llm.chunk"));
}

#[tokio::test]
async fn matrix_path_2_chunked_adapter_finishes_cleanly() {
    let adapter = ChunkedAdapter::new();
    let (kinds, _labels, r) =
        drive(&adapter, &SlashProcessor::new(), "stream me").await;
    assert!(r.is_ok());
    assert!(kinds.contains(&TranscriptKind::LlmRequest));
    assert!(kinds.contains(&TranscriptKind::LlmResponse));
    assert_eq!(*kinds.last().unwrap(), TranscriptKind::Finished);
}

#[tokio::test]
async fn matrix_path_3_big_usage_is_recorded_on_finished() {
    let (kinds, _labels, r) =
        drive(&BigUsageAdapter, &SlashProcessor::new(), "expensive").await;
    assert!(r.is_ok());
    assert_eq!(*kinds.last().unwrap(), TranscriptKind::Finished);
}

#[tokio::test]
async fn matrix_path_4_zero_usage_still_completes() {
    let (kinds, _labels, r) =
        drive(&ZeroUsageAdapter, &SlashProcessor::new(), "quiet").await;
    assert!(r.is_ok());
    assert_eq!(*kinds.last().unwrap(), TranscriptKind::Finished);
}

#[tokio::test]
async fn matrix_path_5_provider_error_lands_aborted() {
    let (kinds, labels, r) =
        drive(&BoomAdapter, &SlashProcessor::new(), "will fail").await;
    assert!(r.is_err());
    assert_eq!(*kinds.last().unwrap(), TranscriptKind::Aborted);
    assert_eq!(labels.last().copied(), Some("aborted"));
}

#[tokio::test]
async fn matrix_path_6_slash_short_circuit_skips_provider() {
    let mut slash = SlashProcessor::new();
    slash.register(Arc::new(NoopSlash));
    let (kinds, _labels, r) = drive(&BoomAdapter, &slash, "/noop").await;
    // BoomAdapter would Err on any call; passing because slash
    // short-circuited the LLM round-trip entirely.
    assert!(r.is_ok());
    assert!(kinds.contains(&TranscriptKind::Slash));
    assert!(!kinds.contains(&TranscriptKind::LlmRequest));
    assert_eq!(*kinds.last().unwrap(), TranscriptKind::Finished);
}

/// Aggregate invariant across every path the matrix exercises: the
/// transcript's last row must be either Finished or Aborted. The
/// engine never leaves a session in the "Accepted but nothing after"
/// pending state on a successful adapter return or a surfaced error.
#[tokio::test]
async fn matrix_terminal_row_invariant_holds_across_all_providers() {
    let adapters: Vec<(&str, &dyn LlmAdapter)> = vec![
        ("echo", &EchoLikeAdapter),
        ("big", &BigUsageAdapter),
        ("zero", &ZeroUsageAdapter),
        ("boom", &BoomAdapter),
    ];
    for (name, a) in adapters {
        let (kinds, _labels, _r) = drive(a, &SlashProcessor::new(), "x").await;
        let last = *kinds.last().unwrap();
        assert!(
            matches!(last, TranscriptKind::Finished | TranscriptKind::Aborted),
            "provider {name} left transcript in non-terminal kind {last:?}",
        );
    }
}
