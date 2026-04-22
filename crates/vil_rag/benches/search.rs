//! Criterion micro-benchmarks for the RAG similarity kernel (M3).
//!
//! The current `RagIndex::search` implementation is an O(n) cosine scan over
//! an in-memory `HashMap<String, IndexedDocument>`. We benchmark the kernel
//! directly — synthetic 384-dim vectors, varying corpus sizes — so that
//! Paket C (B2 HNSW swap) has a baseline to compare recall/latency against.
//!
//! SLA target (today, HashMap scan):
//!   N=100  top-k=5   : < 200 µs
//!   N=1000 top-k=5   : <   2 ms
//!   N=10k  top-k=5   : <  25 ms
//! Once HNSW lands, rewrite these targets to sub-linear scaling expectations.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};

/// Deterministic pseudo-random vector generator. We avoid pulling `rand` as
/// a dev-dep just for benches; a linear-congruential generator is more than
/// enough to keep the vectors decorrelated for the purposes of timing the
/// dot-product kernel.
fn synth_vec(seed: u64, dim: usize) -> Vec<f32> {
    let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    let mut v = Vec::with_capacity(dim);
    for _ in 0..dim {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        // Map the high bits into [-1.0, 1.0].
        let bits = (state >> 33) as u32;
        let f = (bits as f32 / u32::MAX as f32) * 2.0 - 1.0;
        v.push(f);
    }
    // Normalize to unit length so cosine ~= dot-product.
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-8);
    v.iter_mut().for_each(|x| *x /= norm);
    v
}

/// Mirrors the kernel inside `RagIndex::search`: linear scan of cosine
/// similarity, keeping the top-k highest-scoring document ids.
fn top_k_cosine(query: &[f32], corpus: &[(String, Vec<f32>)], k: usize) -> Vec<(String, f32)> {
    let mut scored: Vec<(String, f32)> = corpus
        .iter()
        .map(|(id, emb)| {
            let dot: f32 = query
                .iter()
                .zip(emb.iter())
                .map(|(a, b)| a * b)
                .sum();
            (id.clone(), dot)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(k);
    scored
}

fn bench_cosine_scan(c: &mut Criterion) {
    const DIM: usize = 384; // fastembed default
    const TOP_K: usize = 5;

    let mut group = c.benchmark_group("vil_rag::cosine_scan");
    for &n in &[100usize, 1_000, 10_000] {
        let corpus: Vec<(String, Vec<f32>)> = (0..n)
            .map(|i| (format!("doc_{i}"), synth_vec(i as u64, DIM)))
            .collect();
        let query = synth_vec(u64::MAX, DIM);

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                let out = top_k_cosine(black_box(&query), black_box(&corpus), TOP_K);
                black_box(out);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_cosine_scan);
criterion_main!(benches);
