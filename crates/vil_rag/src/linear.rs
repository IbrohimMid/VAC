//! F9.2 — Brute-force linear ANN index.
//!
//! Feature-free alternative to the optional `hnsw` backend for small
//! corpora (~10⁴ embeddings) where an O(n) scan is still sub-ms. Ships
//! with the crate so downstream consumers never block on the HNSW
//! feature flag landing; swap in `HnswIndex` when n grows.

use crate::ann::{Embedding, Neighbour};
use crate::error::{RagError, RagResult};

/// In-memory vector store + cosine scan.
///
/// Each insert pairs an `Embedding` with a caller-supplied
/// `document_id` (string). The id round-trips verbatim through
/// `Neighbour.document_id` on search — same contract shape as
/// `HnswIndex`, so callers can swap backends without changing how
/// they look up the surrounding document. Positional-index ids are
/// NOT used: two inserts of the same id are preserved in call order
/// and both are eligible to return from `search`.
#[derive(Debug, Default)]
pub struct LinearAnnIndex {
    points: Vec<(String, Embedding)>,
    /// Declared dimension; set on first insert so subsequent mismatches
    /// surface as `RagError` instead of silently producing garbage.
    dim: Option<usize>,
}

impl LinearAnnIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bulk-insert helper. `points` is a list of `(document_id,
    /// embedding)` pairs.
    pub fn with_points(points: Vec<(String, Embedding)>) -> RagResult<Self> {
        let mut idx = Self::new();
        for (id, e) in points {
            idx.add(id, e)?;
        }
        Ok(idx)
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn dim(&self) -> Option<usize> {
        self.dim
    }

    /// Append a single `(document_id, embedding)` pair. First insert
    /// pins the dimension; every subsequent insert must match.
    pub fn add(&mut self, document_id: impl Into<String>, emb: Embedding) -> RagResult<()> {
        let d = emb.dim();
        if d == 0 {
            return Err(RagError::Embedding(
                "cannot index zero-dimensional embedding".into(),
            ));
        }
        match self.dim {
            None => self.dim = Some(d),
            Some(existing) if existing != d => {
                return Err(RagError::Embedding(format!(
                    "dimension mismatch: index={existing}, insert={d}",
                )));
            }
            _ => {}
        }
        self.points.push((document_id.into(), emb));
        Ok(())
    }

    /// Brute-force top-k cosine search.
    ///
    /// Returns at most `top_k` [`Neighbour`]s, ranked by ascending
    /// `Neighbour.score` (smaller = closer; the value is cosine
    /// *distance* = 1 − similarity). Neighbours carry the caller-
    /// supplied `document_id` verbatim, not positional indices.
    pub fn search(&self, query: &Embedding, top_k: usize) -> RagResult<Vec<Neighbour>> {
        if top_k == 0 || self.points.is_empty() {
            return Ok(Vec::new());
        }
        let q_dim = query.dim();
        if let Some(d) = self.dim {
            if d != q_dim {
                return Err(RagError::Embedding(format!(
                    "dimension mismatch: index={d}, query={q_dim}",
                )));
            }
        }
        let mut scored: Vec<Neighbour> = self
            .points
            .iter()
            .map(|(id, p)| Neighbour {
                document_id: id.clone(),
                score: query.cosine_distance(p),
            })
            .collect();
        scored.sort_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_k);
        Ok(scored)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(v: &[f32]) -> Embedding {
        Embedding::new(v.to_vec())
    }

    fn pt(id: &str, v: &[f32]) -> (String, Embedding) {
        (id.to_string(), e(v))
    }

    #[test]
    fn empty_index_returns_empty_search() {
        let idx = LinearAnnIndex::new();
        let out = idx.search(&e(&[1.0, 0.0]), 5).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn top_k_zero_returns_empty() {
        let idx = LinearAnnIndex::with_points(vec![pt("a", &[1.0, 0.0])]).unwrap();
        assert!(idx.search(&e(&[1.0, 0.0]), 0).unwrap().is_empty());
    }

    #[test]
    fn identical_vector_has_distance_zero() {
        let idx = LinearAnnIndex::with_points(vec![pt("a", &[1.0, 0.0, 0.0])]).unwrap();
        let hits = idx.search(&e(&[1.0, 0.0, 0.0]), 1).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].document_id, "a");
        assert!(hits[0].score.abs() < 1e-6);
    }

    #[test]
    fn search_ranks_closest_first_by_document_id() {
        let idx = LinearAnnIndex::with_points(vec![
            pt("doc-east", &[1.0, 0.0]),
            pt("doc-north", &[0.0, 1.0]),
            pt("doc-ne", &[0.707, 0.707]),
        ])
        .unwrap();
        let hits = idx.search(&e(&[1.0, 0.0]), 3).unwrap();
        assert_eq!(hits[0].document_id, "doc-east", "self hit first");
        // 45° from [1,0] → closer than 90°.
        assert_eq!(hits[1].document_id, "doc-ne");
        assert_eq!(hits[2].document_id, "doc-north");
    }

    #[test]
    fn top_k_caps_results_at_requested_count() {
        let idx = LinearAnnIndex::with_points(vec![
            pt("a", &[1.0, 0.0]),
            pt("b", &[0.0, 1.0]),
            pt("c", &[0.5, 0.5]),
        ])
        .unwrap();
        assert_eq!(idx.search(&e(&[1.0, 0.0]), 2).unwrap().len(), 2);
    }

    #[test]
    fn dimension_mismatch_rejects_insert() {
        let mut idx = LinearAnnIndex::new();
        idx.add("a", e(&[1.0, 0.0])).unwrap();
        let err = idx.add("b", e(&[1.0, 0.0, 0.0])).unwrap_err();
        assert!(matches!(err, RagError::Embedding(_)));
    }

    #[test]
    fn dimension_mismatch_rejects_query() {
        let idx = LinearAnnIndex::with_points(vec![pt("a", &[1.0, 0.0])]).unwrap();
        let err = idx.search(&e(&[1.0, 0.0, 0.0]), 1).unwrap_err();
        assert!(matches!(err, RagError::Embedding(_)));
    }

    #[test]
    fn zero_dim_insert_is_rejected() {
        let mut idx = LinearAnnIndex::new();
        let err = idx.add("a", e(&[])).unwrap_err();
        assert!(matches!(err, RagError::Embedding(_)));
    }

    #[test]
    fn empty_constructor_pins_dim_on_first_add() {
        let mut idx = LinearAnnIndex::with_points(vec![]).unwrap();
        assert_eq!(idx.dim(), None);
        idx.add("first", e(&[1.0, 0.0, 0.0])).unwrap();
        assert_eq!(idx.dim(), Some(3));
        // Subsequent mismatch still fails.
        let err = idx.add("second", e(&[1.0, 0.0])).unwrap_err();
        assert!(matches!(err, RagError::Embedding(_)));
    }
}
