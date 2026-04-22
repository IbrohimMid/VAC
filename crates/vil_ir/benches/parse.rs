//! Criterion micro-benchmarks for the VIL IR parser (M3).
//!
//! Tracks the cost of `parse_source` over a small but representative Rust
//! source fixture. Used by `.github/workflows/bench.yml` and
//! `scripts/check_bench_slas.py` to guard against silent parser regressions
//! in PRs that touch `vil_ir::parser` or the underlying `syn` version.
//!
//! SLA target: parse a ~200-line Rust file in under 2 ms on a modern x86_64
//! laptop. Treat anything > 5 ms as a hard regression.

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use vil_ir::parser::parse_source;

/// A small but realistic Rust source fixture. Intentionally compact so the
/// bench stays under one second wall-time; use `parse_large` for scaling
/// signal.
const SMALL_SOURCE: &str = r#"
use std::collections::HashMap;

pub struct Config {
    name: String,
    values: HashMap<String, i64>,
    enabled: bool,
}

impl Config {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            values: HashMap::new(),
            enabled: true,
        }
    }

    pub fn set<K: Into<String>>(&mut self, key: K, value: i64) -> &mut Self {
        self.values.insert(key.into(), value);
        self
    }

    pub fn get(&self, key: &str) -> Option<i64> {
        self.values.get(key).copied()
    }
}

pub trait Service {
    type Request;
    type Response;
    type Error: std::error::Error;

    fn handle(&mut self, req: Self::Request) -> Result<Self::Response, Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Created(String),
    Updated { id: u64, fields: Vec<String> },
    Deleted(u64),
}

fn sum(xs: &[i64]) -> i64 {
    xs.iter().copied().sum()
}
"#;

fn bench_parse_small(c: &mut Criterion) {
    let mut group = c.benchmark_group("vil_ir::parse_source");
    group.throughput(Throughput::Bytes(SMALL_SOURCE.len() as u64));
    group.bench_function("small_fixture", |b| {
        b.iter(|| {
            let module = parse_source(black_box(SMALL_SOURCE), black_box("bench_small.rs"))
                .expect("fixture must parse");
            black_box(module);
        });
    });
    group.finish();
}

criterion_group!(benches, bench_parse_small);
criterion_main!(benches);
