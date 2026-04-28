//! Criterion microbench for `submit_one`.
//!
//! Complements the integration-level SLA tests in `tests/slas.rs`
//! with statistical samples (warm-up + outlier rejection) so long-
//! term drift is visible even when the wall-clock asserts pass.
//! Run via:
//!
//! ```sh
//! cargo bench -p vac_session_engine
//! ```

use criterion::{Criterion, criterion_group, criterion_main};
use tokio::runtime::Runtime;
use uuid::Uuid;
use vac_session_engine::{
    CompactConfig, EchoAdapter, SlashProcessor, SubmitContext, TranscriptWriter,
    TrivialCompactBoundary, UsageTracker, submit_one,
};

fn bench_submit_one(c: &mut Criterion) {
    let rt = Runtime::new().expect("tokio runtime");
    let tmp = tempfile::tempdir().expect("tempdir");
    let writer = TranscriptWriter::new(tmp.path().to_path_buf());

    // Warm-up: one cycle before criterion starts timing, so the
    // first-create + directory-entry fsync cost doesn't skew the
    // first sample.
    rt.block_on(async {
        let ctx = SubmitContext::new(Uuid::new_v4(), "warmup");
        submit_one(
            ctx,
            &writer,
            &SlashProcessor::new(),
            &TrivialCompactBoundary::default(),
            &UsageTracker::new(),
            &EchoAdapter,
            CompactConfig::default(),
            None,
        )
        .await
        .unwrap();
    });

    c.bench_function("submit_one warm tempfs", |b| {
        b.iter(|| {
            rt.block_on(async {
                let ctx = SubmitContext::new(Uuid::new_v4(), "bench");
                submit_one(
                    ctx,
                    &writer,
                    &SlashProcessor::new(),
                    &TrivialCompactBoundary::default(),
                    &UsageTracker::new(),
                    &EchoAdapter,
                    CompactConfig::default(),
                    None,
                )
                .await
                .unwrap()
            })
        });
    });
}

/// A.6 — measures latency until the first `SubmitChunk` arrives.
/// This is the Claude-Code-style first-token metric: the legacy
/// `submit_one` path blocks on the full round-trip before any UI
/// update is possible, so the streaming API is required for <100ms
/// first-paint. Benchmark exists so the number is visible in CI
/// drift-tracking from the moment the API lands.
fn bench_submit_stream_first_chunk(c: &mut Criterion) {
    use futures::StreamExt;
    use std::sync::Arc;
    use vac_session_engine::{CompactBoundary, submit_stream};

    let rt = Runtime::new().expect("tokio runtime");
    let tmp = tempfile::tempdir().expect("tempdir");

    c.bench_function("submit_stream first chunk", |b| {
        b.iter(|| {
            rt.block_on(async {
                let writer = Arc::new(TranscriptWriter::new(tmp.path().to_path_buf()));
                let slash = Arc::new(SlashProcessor::new());
                let compact: Arc<dyn CompactBoundary> = Arc::new(TrivialCompactBoundary::default());
                let usage = Arc::new(UsageTracker::new());
                let llm: Arc<dyn vac_session_engine::LlmAdapter> = Arc::new(EchoAdapter);
                let ctx = SubmitContext::new(Uuid::new_v4(), "bench");
                let mut stream = submit_stream(
                    ctx,
                    writer,
                    slash,
                    compact,
                    usage,
                    llm,
                    CompactConfig::default(),
                );
                // Read chunks until the first non-Accepted arrives —
                // Accepted is deterministic and near-instant; the
                // interesting metric is time-to-first-text or
                // time-to-first-tool-request.
                let mut saw_non_accepted = false;
                while let Some(chunk) = stream.next().await {
                    if !matches!(chunk, vac_session_engine::SubmitChunk::Accepted { .. }) {
                        saw_non_accepted = true;
                        break;
                    }
                }
                assert!(saw_non_accepted);
            })
        });
    });
}

criterion_group!(benches, bench_submit_one, bench_submit_stream_first_chunk,);
criterion_main!(benches);
