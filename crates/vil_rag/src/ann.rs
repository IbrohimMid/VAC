//! Approximate nearest-neighbour index (Paket C scaffold for B2).
//!
//! This module is **scaffolding**, not the final B2 implementation. It wires
//! up `instant-distance` behind the optional `hnsw` feature so Paket C's
//! next session can focus on:
//!
//!   1. Replacing the O(n) cosine scan inside [`crate::index::RagIndex`]
//!      with [`HnswIndex::search`] (this crate).
//!   2. Persistence layout — serialising the graph + vectors to disk under
//!      `.vac/rag/` (see [`crate::store`]).
//!   3. Incremental re-embedding (content-hash → embedding cache).
//!
//! # Why instant-distance?
//!
//! Pure Rust, no FFI, no C++ toolchain, serde-derived persistence, ~300
//! LOC of dependency. That matches the plan's explicit B2 preference for
//! "risiko tech terbatas, tidak butuh FFI" and avoids dragging in
//! llama-cpp-style build complexity before Paket E has made that decision.
//!
//! # Contract
//!
//! The public API here is deliberately narrow and parallels the shape that
//! [`crate::index::RagIndex::search`] will eventually delegate to, so a
//! later session can swap the backend in place without rippling through
//! call sites.

use crate::error::{RagError, RagResult};

/// A unit-normalised embedding used by the HNSW index.
///
/// We wrap [`Vec<f32>`] instead of using it directly because
/// `instant_distance::Point` must be implemented on a local type, and we
/// want room to add invariants (dimension checks, normalization) later
/// without breaking callers.
#[derive(Debug, Clone)]
pub struct Embedding(pub Vec<f32>);

impl Embedding {
    pub fn new(values: Vec<f32>) -> Self {
        Self(values)
    }

    pub fn dim(&self) -> usize {
        self.0.len()
    }

    pub fn as_slice(&self) -> &[f32] {
        &self.0
    }

    /// Cosine distance. Assumes both vectors are non-empty and equal length;
    /// returns `1.0` (maximum distance) when they disagree so an ill-formed
    /// point is pushed to the back of the candidate list instead of
    /// panicking.
    pub fn cosine_distance(&self, other: &Self) -> f32 {
        if self.0.is_empty() || other.0.is_empty() || self.0.len() != other.0.len() {
            return 1.0;
        }
        let dot: f32 = self.0.iter().zip(other.0.iter()).map(|(a, b)| a * b).sum();
        let na: f32 = self.0.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = other.0.iter().map(|x| x * x).sum::<f32>().sqrt();
        if na == 0.0 || nb == 0.0 {
            return 1.0;
        }
        // Clamp into [0, 2] then treat "closer" as "smaller distance".
        (1.0 - (dot / (na * nb))).clamp(0.0, 2.0)
    }
}

#[cfg(feature = "hnsw")]
impl instant_distance::Point for Embedding {
    fn distance(&self, other: &Self) -> f32 {
        self.cosine_distance(other)
    }
}

/// A single neighbour returned by [`HnswIndex::search`].
#[derive(Debug, Clone)]
pub struct Neighbour {
    pub document_id: String,
    /// Cosine similarity in \[0.0, 1.0\] — higher is more similar.
    pub score: f32,
}

/// Builder for an [`HnswIndex`].
///
/// Today only the `hnsw` feature path is populated; the `#[cfg(not)]` variant
/// returns an error so callers can gracefully fall back to the HashMap scan
/// inside [`crate::index::RagIndex`].
#[derive(Debug, Default)]
pub struct HnswBuilder;

impl HnswBuilder {
    pub fn new() -> Self {
        Self
    }

    #[cfg(feature = "hnsw")]
    pub fn build(self, ids: Vec<String>, embeddings: Vec<Embedding>) -> RagResult<HnswIndex> {
        if ids.len() != embeddings.len() {
            return Err(RagError::Indexing(format!(
                "ann: ids ({}) and embeddings ({}) length mismatch",
                ids.len(),
                embeddings.len(),
            )));
        }
        // `Builder::build` returns an `HnswMap<Point, Value>` that owns both
        // the ANN graph and a parallel values vector, so we attach the raw
        // id strings as values and skip a side-table lookup at query time.
        let map = instant_distance::Builder::default().build(embeddings, ids);
        Ok(HnswIndex { map })
    }

    #[cfg(not(feature = "hnsw"))]
    pub fn build(self, _ids: Vec<String>, _embeddings: Vec<Embedding>) -> RagResult<HnswIndex> {
        Err(RagError::Indexing(
            "ann: `hnsw` feature disabled; rebuild with --features hnsw".to_string(),
        ))
    }
}

/// An HNSW-backed ANN index over [`Embedding`]s.
///
/// When the `hnsw` feature is off this is a placeholder struct so downstream
/// modules can name the type; any call to [`HnswIndex::search`] will return
/// an error explaining the required feature flag.
#[cfg(feature = "hnsw")]
pub struct HnswIndex {
    map: instant_distance::HnswMap<Embedding, String>,
}

#[cfg(not(feature = "hnsw"))]
pub struct HnswIndex {
    _unbuildable: (),
}

impl HnswIndex {
    #[cfg(feature = "hnsw")]
    pub fn search(&self, query: &Embedding, top_k: usize) -> RagResult<Vec<Neighbour>> {
        let mut search = instant_distance::Search::default();
        // `HnswMap::search` yields `MapItem { distance, value, .. }`. The
        // `value` is the id we stored at build time; `distance` is the raw
        // cosine distance so we flip it back into a \[0, 1\] similarity.
        let results: Vec<Neighbour> = self
            .map
            .search(query, &mut search)
            .take(top_k)
            .map(|item| Neighbour {
                document_id: item.value.clone(),
                score: (1.0 - item.distance).clamp(0.0, 1.0),
            })
            .collect();
        Ok(results)
    }

    #[cfg(not(feature = "hnsw"))]
    pub fn search(&self, _query: &Embedding, _top_k: usize) -> RagResult<Vec<Neighbour>> {
        Err(RagError::Search(
            "ann: `hnsw` feature disabled; rebuild with --features hnsw".to_string(),
        ))
    }
}

#[cfg(all(test, feature = "hnsw"))]
mod tests {
    use super::*;

    fn synth(seed: u64) -> Embedding {
        // Tiny 4-dim synthetic vector, unit-normalised.
        let base = [
            (seed as f32).sin(),
            ((seed + 1) as f32).cos(),
            ((seed + 2) as f32).sin(),
            ((seed + 3) as f32).cos(),
        ];
        let n: f32 = base.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-8);
        Embedding(base.iter().map(|x| x / n).collect())
    }

    #[test]
    fn build_and_search_smoke() {
        let ids: Vec<String> = (0..20).map(|i| format!("d{i}")).collect();
        let embs: Vec<Embedding> = (0..20).map(synth).collect();
        let index = HnswBuilder::new()
            .build(ids.clone(), embs.clone())
            .expect("build");
        let hits = index.search(&embs[0], 3).expect("search");
        assert!(!hits.is_empty());
        // The nearest neighbour of a point against itself must be itself.
        assert_eq!(hits[0].document_id, ids[0]);
    }
}
