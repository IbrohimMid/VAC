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

criterion_group!(benches, bench_submit_one);
criterion_main!(benches);
