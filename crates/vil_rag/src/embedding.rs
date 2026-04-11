//! Embedding generation — Phase 2: local embedding via fastembed-rs.

use crate::error::RagResult;

pub struct EmbeddingModel;

impl EmbeddingModel {
    pub fn new() -> RagResult<Self> {
        tracing::info!("Embedding model initialized (stub — Phase 2)");
        Ok(Self)
    }

    pub fn embed(&self, _text: &str) -> RagResult<Vec<f32>> {
        Ok(vec![0.0; 384])
    }

    pub fn embed_batch(&self, texts: &[&str]) -> RagResult<Vec<Vec<f32>>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }
}
