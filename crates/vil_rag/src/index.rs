//! RAG Index — manages document embeddings and semantic similarity search.

use crate::embedding::EmbeddingModel;
use crate::error::{RagError, RagResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedDocument {
    pub id: String,
    pub path: String,
    pub content_preview: String,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub document_id: String,
    pub path: String,
    pub score: f64,
    pub content_preview: String,
}

pub struct RagIndex {
    documents: Arc<RwLock<HashMap<String, IndexedDocument>>>,
    model: Arc<RwLock<Option<EmbeddingModel>>>,
    doc_count: Arc<AtomicUsize>,
}

impl Default for RagIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl RagIndex {
    pub fn new() -> Self {
        Self {
            documents: Arc::new(RwLock::new(HashMap::new())),
            model: Arc::new(RwLock::new(None)),
            doc_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub async fn init_model(&self) -> RagResult<()> {
        let model = EmbeddingModel::new()?;
        let mut m = self.model.write().await;
        *m = Some(model);
        Ok(())
    }

    pub async fn index_file(&self, id: &str, path: &str, content: &str) -> RagResult<()> {
        let model_guard = self.model.read().await;
        let model = model_guard
            .as_ref()
            .ok_or_else(|| RagError::Indexing("Model not initialized".to_string()))?;

        let embedding = model.embed(content)?;

        let doc = IndexedDocument {
            id: id.to_string(),
            path: path.to_string(),
            content_preview: content.chars().take(200).collect(),
            embedding,
        };

        let mut docs = self.documents.write().await;
        let is_new = docs.insert(id.to_string(), doc).is_none();
        if is_new {
            self.doc_count.fetch_add(1, Ordering::Relaxed);
        }

        Ok(())
    }

    pub async fn search(&self, query: &str, top_k: usize) -> RagResult<Vec<SearchResult>> {
        let model_guard = self.model.read().await;
        let model = model_guard
            .as_ref()
            .ok_or_else(|| RagError::Search("Model not initialized".to_string()))?;

        let query_embedding = model.embed(query)?;

        let docs = self.documents.read().await;
        let mut results: Vec<SearchResult> = docs
            .values()
            .map(|doc| {
                let score = cosine_similarity(&query_embedding, &doc.embedding);
                SearchResult {
                    document_id: doc.id.clone(),
                    path: doc.path.clone(),
                    score,
                    content_preview: doc.content_preview.clone(),
                }
            })
            .filter(|r| r.score > 0.0)
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(top_k);

        Ok(results)
    }

    /// R5.b (blueprint) — alternative search path that routes the
    /// cosine scan through `LinearAnnIndex`. Result shape is the
    /// same as `search`; swapping backends (LinearAnnIndex →
    /// `HnswIndex` once the corpus crosses 10 k docs) is a
    /// one-line flip inside this function. Callers that want the
    /// ANN-backed path opt in explicitly so today's callers stay
    /// on the default HashMap scan until persistence lands.
    pub async fn search_via_linear(
        &self,
        query: &str,
        top_k: usize,
    ) -> RagResult<Vec<SearchResult>> {
        use crate::ann::Embedding;
        use crate::linear::LinearAnnIndex;

        let model_guard = self.model.read().await;
        let model = model_guard
            .as_ref()
            .ok_or_else(|| RagError::Search("Model not initialized".to_string()))?;
        let query_embedding = Embedding::new(model.embed(query)?);

        let docs = self.documents.read().await;
        let mut linear = LinearAnnIndex::new();
        let mut preview_by_id: std::collections::HashMap<String, (String, String)> =
            std::collections::HashMap::new();
        for doc in docs.values() {
            preview_by_id.insert(
                doc.id.clone(),
                (doc.path.clone(), doc.content_preview.clone()),
            );
            linear.add(doc.id.clone(), Embedding::new(doc.embedding.clone()))?;
        }
        let hits = linear.search(&query_embedding, top_k)?;
        Ok(hits
            .into_iter()
            .filter_map(|n| {
                preview_by_id.get(&n.document_id).map(|(path, preview)| {
                    SearchResult {
                        document_id: n.document_id.clone(),
                        path: path.clone(),
                        // Linear index returns cosine *distance*;
                        // convert to the similarity score the
                        // `SearchResult` contract expects.
                        score: (1.0 - n.score) as f64,
                        content_preview: preview.clone(),
                    }
                })
            })
            .collect())
    }

    pub fn document_count(&self) -> usize {
        self.doc_count.load(Ordering::Relaxed)
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    (dot_product / (norm_a * norm_b)) as f64
}
