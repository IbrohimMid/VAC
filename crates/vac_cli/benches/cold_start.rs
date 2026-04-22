//! Criterion micro-benchmark for `vac` CLI cold-start (M3).
//!
//! Measures the end-to-end cost of spawning the compiled `vac` binary and
//! letting clap print `--version`. This is the tightest proxy for cold-start
//! overhead that doesn't require instrumenting the CLI itself: process fork
//! + dynamic linker + Rust runtime init + clap construction + stdout flush.
//!
//! Cargo automatically exports `CARGO_BIN_EXE_vac` for bench targets so we
//! avoid brittle path guessing.
//!
//! SLA target: cold `vac --version` completes in under 50 ms on a warm FS
//! cache. Anything > 150 ms is a regression (usually a new heavy dependency
//! being linked or a module that does work in `main`).

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::process::Command;

fn bench_version_cold_start(c: &mut Criterion) {
    let bin = env!("CARGO_BIN_EXE_vac");

    let mut group = c.benchmark_group("vac::cold_start");
    // Cold start benches are inherently slow relative to in-process work;
    // drop the sample size so the group finishes in a reasonable CI budget.
    group.sample_size(20);
    group.bench_function("version", |b| {
        b.iter(|| {
            let out = Command::new(black_box(bin))
                .arg("--version")
                .output()
                .expect("vac binary must be available for bench");
            assert!(out.status.success(), "vac --version failed: {:?}", out);
            black_box(out);
        });
    });
    group.finish();
}

criterion_group!(benches, bench_version_cold_start);
criterion_main!(benches);
