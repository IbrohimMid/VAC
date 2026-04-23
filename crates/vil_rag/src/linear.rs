//! F9.2 — Brute-force linear ANN index.
//!
//! Feature-free alternative to the optional `hnsw` backend for small
//! corpora (~10⁴ embeddings) where an O(n) scan is still sub-ms. Ships
//! with the crate so downstream consumers never block on the HNSW
//! feature flag landing; swap in `HnswIndex` when n grows.

use crate::ann::{Embedding, Neighbour};
use crate::error::{RagError, RagResult};

/// In-memory vector store + cosine scan.
#[derive(Debug, Default)]
pub struct LinearAnnIndex {
    points: Vec<Embedding>,
    /// Declared dimension; set on first insert so subsequent mismatches
    /// surface as `RagError` instead of silently producing garbage.
    dim: Option<usize>,
}

impl LinearAnnIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_points(points: Vec<Embedding>) -> RagResult<Self> {
        let mut idx = Self::new();
        for p in points {
            idx.add(p)?;
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

    /// Append a single embedding. First insert pins the dimension;
    /// every subsequent insert must match.
    pub fn add(&mut self, emb: Embedding) -> RagResult<()> {
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
        self.points.push(emb);
        Ok(())
    }

    /// Brute-force top-k cosine search. Returns at most `top_k`
    /// neighbours, ranked by ascending cosine distance (best first).
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
            .enumerate()
            .map(|(id, p)| Neighbour {
                document_id: id.to_string(),
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

    #[test]
    fn empty_index_returns_empty_search() {
        let idx = LinearAnnIndex::new();
        let out = idx.search(&e(&[1.0, 0.0]), 5).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn top_k_zero_returns_empty() {
        let idx = LinearAnnIndex::with_points(vec![e(&[1.0, 0.0])]).unwrap();
        assert!(idx.search(&e(&[1.0, 0.0]), 0).unwrap().is_empty());
    }

    #[test]
    fn identical_vector_has_distance_zero() {
        let idx = LinearAnnIndex::with_points(vec![e(&[1.0, 0.0, 0.0])]).unwrap();
        let hits = idx.search(&e(&[1.0, 0.0, 0.0]), 1).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].score.abs() < 1e-6);
    }

    #[test]
    fn search_ranks_closest_first() {
        let idx = LinearAnnIndex::with_points(vec![
            e(&[1.0, 0.0]),
            e(&[0.0, 1.0]),
            e(&[0.707, 0.707]),
        ])
        .unwrap();
        let hits = idx.search(&e(&[1.0, 0.0]), 3).unwrap();
        assert_eq!(hits[0].document_id, "0", "self hit first");
        // 0.707/0.707 is 45° from [1,0] → closer than [0,1] (90°).
        assert_eq!(hits[1].document_id, "2");
        assert_eq!(hits[2].document_id, "1");
    }

    #[test]
    fn top_k_caps_results_at_requested_count() {
        let idx = LinearAnnIndex::with_points(vec![
            e(&[1.0, 0.0]),
            e(&[0.0, 1.0]),
            e(&[0.5, 0.5]),
        ])
        .unwrap();
        assert_eq!(idx.search(&e(&[1.0, 0.0]), 2).unwrap().len(), 2);
    }

    #[test]
    fn dimension_mismatch_rejects_insert() {
        let mut idx = LinearAnnIndex::new();
        idx.add(e(&[1.0, 0.0])).unwrap();
        let err = idx.add(e(&[1.0, 0.0, 0.0])).unwrap_err();
        assert!(matches!(err, RagError::Embedding(_)));
    }

    #[test]
    fn dimension_mismatch_rejects_query() {
        let idx = LinearAnnIndex::with_points(vec![e(&[1.0, 0.0])]).unwrap();
        let err = idx.search(&e(&[1.0, 0.0, 0.0]), 1).unwrap_err();
        assert!(matches!(err, RagError::Embedding(_)));
    }

    #[test]
    fn zero_dim_insert_is_rejected() {
        let mut idx = LinearAnnIndex::new();
        let err = idx.add(e(&[])).unwrap_err();
        assert!(matches!(err, RagError::Embedding(_)));
    }
}
