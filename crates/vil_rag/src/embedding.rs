//! Embedding generation via fastembed.

use crate::error::{RagError, RagResult};

pub struct EmbeddingModel {
    model: fastembed::TextEmbedding,
}

impl EmbeddingModel {
    pub fn new() -> RagResult<Self> {
        let model = fastembed::TextEmbedding::try_new(fastembed::InitOptions::new(
            fastembed::EmbeddingModel::AllMiniLML6V2,
        ))
        .map_err(|e| RagError::Embedding(e.to_string()))?;

        tracing::info!("Embedding model initialized: AllMiniLML6V2 (384 dimensions)");
        Ok(Self { model })
    }

    pub fn embed(&self, text: &str) -> RagResult<Vec<f32>> {
        let embeddings = self.model.embed(vec![text], None)?;
        Ok(embeddings.into_iter().next().unwrap_or_default())
    }

    pub fn embed_batch(&self, texts: &[&str]) -> RagResult<Vec<Vec<f32>>> {
        let texts: Vec<&str> = texts.iter().map(|s| *s).collect();
        Ok(self.model.embed(texts, None)?)
    }
}
