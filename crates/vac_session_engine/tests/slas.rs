//! F10.2 — SLA assertion tests (not microbenchmarks).
//!
//! These are integration tests that bound wall-clock time on the
//! load-bearing paths of `submit_one` and the transcript writer so a
//! regression that doubles submit latency fails CI here instead of
//! in production. Real statistical benchmarks (warm-up samples,
//! outlier rejection) live under `benches/` via criterion.
//!
//! ## Tolerance
//!
//! Budgets are tuned for a warm local tempfs. Noisy shared-tenant CI
//! (GitHub Actions, container runners) can multiply wall-clock by
//! 2–3×. Export `VAC_SLA_TOLERANCE=<multiplier>` to scale every
//! budget uniformly; `VAC_SLA_TOLERANCE=3` triples the slack without
//! changing the asserted ratios.

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
        tool_calls: Vec::new(),
        })
    }
}

/// Read `VAC_SLA_TOLERANCE` once; default 1.0. Invalid / non-positive
/// values degrade to 1.0 rather than failing the test setup.
fn tolerance_multiplier() -> f32 {
    match std::env::var("VAC_SLA_TOLERANCE") {
        Ok(s) => s.parse::<f32>().ok().filter(|v| *v > 0.0).unwrap_or(1.0),
        Err(_) => 1.0,
    }
}

/// Assert `measured <= sla * tolerance`; on overrun print a
/// diagnostic that includes both the observed time and the slack, so
/// flakes triage fast.
fn assert_within_sla(label: &str, measured: Duration, sla: Duration) {
    let mult = tolerance_multiplier();
    let budget = sla.mul_f32(mult);
    if measured > budget {
        panic!(
            "SLA breach [{label}]: measured {:?} > budget {:?} \
             (base {:?} × tolerance {:.2} = {:?}; overshoot {:?})",
            measured,
            budget,
            sla,
            mult,
            budget,
            measured - budget,
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

    // Warm up: one append pays the first-write + directory-entry
    // fsync cost that otherwise skews the loop below against the
    // budget on a cold tempfs.
    let warmup = TranscriptEntry::new(
        sid,
        TranscriptKind::LlmResponse,
        serde_json::json!({ "warmup": true }),
    );
    writer.append(&handle, &warmup).await.unwrap();

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

    // Baseline: one warm single-submit measurement so we can assert
    // concurrency actually helps (10 parallel submits must be faster
    // than 10× serial).
    let base_ctx = SubmitContext::new(Uuid::new_v4(), "baseline");
    let base_start = Instant::now();
    submit_one(
        base_ctx,
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
    let single = base_start.elapsed();

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
    let parallel = started.elapsed();
    assert_within_sla("10 concurrent submits", parallel, Duration::from_secs(1));

    // Parallelism lower bound: a fully-serialized implementation
    // would take roughly 10 × single. Require the actual run to
    // come in under 70% of that — proves the writer's per-handle
    // mutex isn't accidentally serializing across sessions. Only
    // enforce when `single` is ≥ 2ms; below that the absolute
    // difference is dominated by scheduler jitter and the ratio is
    // not meaningful.
    if single >= Duration::from_millis(2) {
        let serial_upper = single.mul_f32(10.0 * 0.7);
        assert!(
            parallel < serial_upper,
            "parallelism degraded: parallel {:?} >= 70% of serialized {:?} \
             (single {:?})",
            parallel,
            serial_upper,
            single,
        );
    }
}
