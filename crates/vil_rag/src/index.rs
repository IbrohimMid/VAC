//! RAG Index — manages document embeddings and similarity search.

use crate::error::RagResult;
use serde::{Deserialize, Serialize};

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
    documents: Vec<IndexedDocument>,
}

#[allow(clippy::new_without_default)]
impl RagIndex {
    pub fn new() -> Self {
        Self {
            documents: Vec::new(),
        }
    }

    pub fn add_document(&mut self, id: &str, path: &str, content: &str) -> RagResult<()> {
        self.documents.push(IndexedDocument {
            id: id.to_string(),
            path: path.to_string(),
            content_preview: content.chars().take(200).collect(),
            embedding: vec![],
        });
        Ok(())
    }

    pub fn search(&self, query: &str, top_k: usize) -> RagResult<Vec<SearchResult>> {
        let query_lower = query.to_lowercase();
        let mut results: Vec<SearchResult> = self
            .documents
            .iter()
            .filter(|d| {
                d.path.to_lowercase().contains(&query_lower)
                    || d.content_preview.to_lowercase().contains(&query_lower)
            })
            .map(|d| SearchResult {
                document_id: d.id.clone(),
                path: d.path.clone(),
                score: 1.0,
                content_preview: d.content_preview.clone(),
            })
            .collect();

        results.truncate(top_k);
        Ok(results)
    }

    pub fn document_count(&self) -> usize {
        self.documents.len()
    }
}
