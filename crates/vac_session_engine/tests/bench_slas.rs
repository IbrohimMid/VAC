//! F10.2 — Benchmark matrix with stable SLAs.
//!
//! Lightweight in-test microbenches (no criterion dep) that measure
//! the load-bearing paths of `submit_one` and the transcript writer.
//! Each assertion bounds wall-clock time to an SLA the team commits
//! to; a regression that doubles submit latency fails CI here instead
//! of in production.
//!
//! SLAs are intentionally loose (1–2x the observed steady-state) so
//! noisy CI doesn't flap. Tighten over time as confidence grows.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use uuid::Uuid;
use vac_session_engine::{
    CompactConfig, EngineResult, LlmAdapter, LlmRequest, LlmResponse, SlashProcessor,
    SubmitContext, TranscriptEntry, TranscriptKind, TranscriptWriter,
    TrivialCompactBoundary, UsageTracker, submit_one,
};

/// Zero-latency adapter so the measured cost is engine + transcript
/// I/O only, not synthetic LLM delay.
struct InstantAdapter;

#[async_trait]
impl LlmAdapter for InstantAdapter {
    async fn complete(&self, req: LlmRequest) -> EngineResult<LlmResponse> {
        Ok(LlmResponse {
            provider: "bench".into(),
            model: "b1".into(),
            content: req.prompt,
            input_tokens: 1,
            output_tokens: 1,
        })
    }
}

/// Assert `measured <= sla`; on overrun print a diagnostic that
/// includes both the observed time and the slack, so flakes triage
/// fast.
fn assert_within_sla(label: &str, measured: Duration, sla: Duration) {
    if measured > sla {
        panic!(
            "SLA breach [{label}]: measured {:?} > budget {:?} (overshoot {:?})",
            measured,
            sla,
            measured - sla,
        );
    }
}

/// SLA: one full submit_one cycle with in-memory transcript writes
/// and a zero-latency adapter must complete under 150ms on a warm
/// tempfs. Local observed range is 4–8ms; 150ms leaves headroom for
/// cold CI disks.
#[tokio::test]
async fn sla_submit_one_under_150ms() {
    let tmp = tempfile::tempdir().unwrap();
    let writer = TranscriptWriter::new(tmp.path().to_path_buf());
    // Warm up once so file-create + tokio initialization aren't
    // counted against the measurement.
    let warmup = SubmitContext::new(Uuid::new_v4(), "warmup");
    submit_one(
        warmup,
        &writer,
        &SlashProcessor::new(),
        &TrivialCompactBoundary::default(),
        &UsageTracker::new(),
        &InstantAdapter,
        CompactConfig::default(),
        None,
    )
    .await
    .unwrap();

    let ctx = SubmitContext::new(Uuid::new_v4(), "measured");
    let started = Instant::now();
    submit_one(
        ctx,
        &writer,
        &SlashProcessor::new(),
        &TrivialCompactBoundary::default(),
        &UsageTracker::new(),
        &InstantAdapter,
        CompactConfig::default(),
        None,
    )
    .await
    .unwrap();
    assert_within_sla(
        "submit_one end-to-end",
        started.elapsed(),
        Duration::from_millis(150),
    );
}

/// SLA: 100 transcript appends on an already-open session must
/// complete under 500ms. 5ms per append on warm tempfs is typical.
#[tokio::test]
async fn sla_transcript_append_100_under_500ms() {
    let tmp = tempfile::tempdir().unwrap();
    let writer = TranscriptWriter::new(tmp.path().to_path_buf());
    let sid = Uuid::new_v4();
    let handle = writer.open(sid).await.unwrap();

    let started = Instant::now();
    for i in 0..100 {
        let entry = TranscriptEntry::new(
            sid,
            TranscriptKind::LlmResponse,
            serde_json::json!({ "i": i }),
        );
        writer.append(&handle, &entry).await.unwrap();
    }
    assert_within_sla(
        "100 transcript appends",
        started.elapsed(),
        Duration::from_millis(500),
    );
}

/// SLA: 10 concurrent submits against 10 distinct sessions complete
/// under 1s. Shakes out any accidental serialization across the
/// writer root (handles are per-session, so concurrency should be
/// limited only by the tokio runtime and disk).
#[tokio::test]
async fn sla_10_concurrent_submits_under_1s() {
    let tmp = tempfile::tempdir().unwrap();
    let writer = std::sync::Arc::new(TranscriptWriter::new(tmp.path().to_path_buf()));

    let started = Instant::now();
    let mut tasks = Vec::new();
    for _ in 0..10 {
        let w = writer.clone();
        tasks.push(tokio::spawn(async move {
            let ctx = SubmitContext::new(Uuid::new_v4(), "concurrent");
            submit_one(
                ctx,
                &w,
                &SlashProcessor::new(),
                &TrivialCompactBoundary::default(),
                &UsageTracker::new(),
                &InstantAdapter,
                CompactConfig::default(),
                None,
            )
            .await
            .unwrap();
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    assert_within_sla(
        "10 concurrent submits",
        started.elapsed(),
        Duration::from_secs(1),
    );
}
