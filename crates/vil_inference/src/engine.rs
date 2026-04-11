//! Local inference engine — runs GGUF/ONNX models.

use crate::error::{InferenceError, InferenceResult};
use std::path::Path;

pub struct InferenceEngine;

#[allow(clippy::new_without_default)]
impl InferenceEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn load_gguf(&self, path: &Path) -> InferenceResult<()> {
        if !path.exists() {
            return Err(InferenceError::ModelNotFound(path.display().to_string()));
        }
        tracing::info!(path = %path.display(), "GGUF model loading (stub)");
        Ok(())
    }

    pub fn load_onnx(&self, path: &Path) -> InferenceResult<()> {
        if !path.exists() {
            return Err(InferenceError::ModelNotFound(path.display().to_string()));
        }
        tracing::info!(path = %path.display(), "ONNX model loading (stub)");
        Ok(())
    }

    pub async fn infer(&self, _prompt: &str, _max_tokens: u32) -> InferenceResult<String> {
        tracing::warn!("Local inference not yet implemented (Phase 2)");
        Err(InferenceError::InferenceFailed(
            "Not yet implemented".into(),
        ))
    }
}
