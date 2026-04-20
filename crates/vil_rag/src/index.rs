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
