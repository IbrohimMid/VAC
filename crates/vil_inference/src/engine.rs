//! Local inference engine — pluggable backend trait plus a stub default.
//!
//! **Paket E scaffold for B1.** The real B1 deliverable is a full local
//! inference stack (GGUF via llama-cpp-2 / ONNX via ort / Candle pure-Rust).
//! That work is 1–2 weeks on its own and requires a decision about which
//! C++/FFI toolchain is acceptable. This file stages the surface so the
//! follow-up session can drop in a real backend behind the existing API.
//!
//! # Design
//!
//! * [`InferenceBackend`] is the async trait every concrete backend
//!   implements. Callers code against `Arc<dyn InferenceBackend>` and are
//!   oblivious to the format.
//! * [`BackendKind`] is a plain enum used by factories, CLI flags, and
//!   config files to name a backend without pulling its implementation
//!   into the type system.
//! * [`StubBackend`] is the no-op backend that preserves the current
//!   “Not yet implemented” behaviour while the real backends land. It
//!   is always available so the crate compiles with the default feature
//!   set and so tests can exercise the trait in isolation.
//! * [`InferenceEngine`] remains the public facade. It wraps a backend
//!   handle and keeps the previous `load_gguf` / `load_onnx` / `infer`
//!   method names so downstream callers compile unchanged.
//!
//! Follow-up scaffolds (one module per backend, added in dedicated
//! sub-PRs so each can be reviewed in isolation):
//!
//! * `gguf.rs`  — llama-cpp-2 FFI or candle’s GGUF loader.
//! * `onnx.rs`  — `ort` crate.
//! * `candle.rs` — pure-Rust inference for small models.

use crate::error::{InferenceError, InferenceResult};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Names of the supported backends. Used by factories / CLI / config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BackendKind {
    /// No-op backend used before B1 lands. Always available.
    Stub,
    /// Deterministic mock for tests + `--mock` runs (F9.1). Distinct
    /// from `Stub` so routing can tell a production fallback apart
    /// from a testing adapter.
    Mock,
    /// GGUF via llama-cpp-2 (or candle-gguf). Not yet implemented.
    Gguf,
    /// ONNX via `ort`. Not yet implemented.
    Onnx,
    /// Candle (pure-Rust). Not yet implemented.
    Candle,
}

impl BackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BackendKind::Stub => "stub",
            BackendKind::Mock => "mock",
            BackendKind::Gguf => "gguf",
            BackendKind::Onnx => "onnx",
            BackendKind::Candle => "candle",
        }
    }
}

/// What the caller wants a backend to produce.
#[derive(Debug, Clone)]
pub struct InferenceRequest {
    pub prompt: String,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
}

impl InferenceRequest {
    pub fn new(prompt: impl Into<String>, max_tokens: u32) -> Self {
        Self {
            prompt: prompt.into(),
            max_tokens,
            temperature: None,
        }
    }
}

/// Metadata a backend returns once a model is loaded. Helps the router
/// pick between backends (e.g. max ctx length, quantisation).
#[derive(Debug, Clone, Default)]
pub struct LoadedModel {
    pub path: PathBuf,
    pub kind: Option<BackendKind>,
    pub max_context_tokens: Option<u32>,
}

/// Contract every concrete backend implements. Async so backends can run
/// inference on a dedicated worker without blocking the caller thread.
#[async_trait]
pub trait InferenceBackend: Send + Sync {
    fn kind(&self) -> BackendKind;

    async fn load(&self, path: &Path) -> InferenceResult<LoadedModel>;

    async fn infer(&self, request: &InferenceRequest) -> InferenceResult<String>;
}

/// The default no-op backend. Preserves today’s behaviour: `load` checks
/// that the file exists and records it, `infer` returns
/// `InferenceError::InferenceFailed("Not yet implemented")`.
#[derive(Debug, Default)]
pub struct StubBackend;

#[async_trait]
impl InferenceBackend for StubBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Stub
    }

    async fn load(&self, path: &Path) -> InferenceResult<LoadedModel> {
        if !path.exists() {
            return Err(InferenceError::ModelNotFound(path.display().to_string()));
        }
        tracing::info!(path = %path.display(), backend = "stub", "model load (stub)");
        Ok(LoadedModel {
            path: path.to_path_buf(),
            kind: Some(BackendKind::Stub),
            max_context_tokens: None,
        })
    }

    async fn infer(&self, _request: &InferenceRequest) -> InferenceResult<String> {
        tracing::warn!("local inference not yet implemented (B1 Paket E follow-up)");
        Err(InferenceError::InferenceFailed(
            "Not yet implemented (stub backend)".into(),
        ))
    }
}

/// Facade kept for backwards compatibility with callers that predate the
/// backend trait. New code should prefer `Arc<dyn InferenceBackend>`.
pub struct InferenceEngine {
    backend: Arc<dyn InferenceBackend>,
}

#[allow(clippy::new_without_default)]
impl InferenceEngine {
    /// Construct an engine with the best available default backend:
    /// Candle when the `candle` feature is enabled, Stub otherwise.
    /// Previous callers got Stub unconditionally; post-M11 the feature
    /// flag is the selector so enabling `--features candle` flips the
    /// default without touching call sites.
    pub fn new() -> Self {
        #[cfg(feature = "candle")]
        {
            return Self::with_candle();
        }
        #[cfg(not(feature = "candle"))]
        {
            Self {
                backend: Arc::new(StubBackend),
            }
        }
    }

    /// Force the stub backend regardless of features. Handy for tests
    /// that want the no-op contract even when `candle` is enabled.
    pub fn stub() -> Self {
        Self {
            backend: Arc::new(StubBackend),
        }
    }

    /// Wire in a concrete backend. Factories (including CLI / config) should
    /// go through this constructor so they can swap implementations
    /// without touching callers.
    pub fn with_backend(backend: Arc<dyn InferenceBackend>) -> Self {
        Self { backend }
    }

    /// Select a backend by name, honouring env override
    /// `VAC_INFERENCE_BACKEND` when `name` is `None`. Unknown or
    /// unavailable names fall back to the default. Intended for CLI
    /// `--backend <kind>` and config wiring.
    pub fn from_name(name: Option<&str>) -> Self {
        let picked = name
            .map(|s| s.to_string())
            .or_else(|| std::env::var("VAC_INFERENCE_BACKEND").ok())
            .map(|s| s.to_ascii_lowercase());
        match picked.as_deref() {
            Some("stub") => Self::stub(),
            #[cfg(feature = "candle")]
            Some("candle") => Self::with_candle(),
            _ => Self::new(),
        }
    }

    /// Convenience constructor for the Candle backend (B1 primary). Only
    /// available when the `candle` feature is enabled.
    #[cfg(feature = "candle")]
    pub fn with_candle() -> Self {
        Self {
            backend: Arc::new(crate::backends::CandleBackend::new_cpu()),
        }
    }

    pub fn backend_kind(&self) -> BackendKind {
        self.backend.kind()
    }

    /// Preserved name for callers that only care about “some model is
    /// loaded.” Delegates to the backend’s `load` and discards the
    /// metadata.
    pub async fn load_gguf(&self, path: &Path) -> InferenceResult<()> {
        self.backend.load(path).await.map(|_| ())
    }

    pub async fn load_onnx(&self, path: &Path) -> InferenceResult<()> {
        self.backend.load(path).await.map(|_| ())
    }

    pub async fn infer(&self, prompt: &str, max_tokens: u32) -> InferenceResult<String> {
        self.backend
            .infer(&InferenceRequest::new(prompt, max_tokens))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_backend_reports_its_kind() {
        let engine = InferenceEngine::stub();
        assert_eq!(engine.backend_kind(), BackendKind::Stub);
    }

    #[tokio::test]
    async fn stub_infer_is_not_yet_implemented() {
        let engine = InferenceEngine::stub();
        let err = engine.infer("hello", 16).await.unwrap_err();
        assert!(matches!(err, InferenceError::InferenceFailed(_)));
    }

    #[tokio::test]
    async fn load_missing_file_returns_not_found() {
        let engine = InferenceEngine::stub();
        let err = engine
            .load_gguf(Path::new("/does/not/exist.gguf"))
            .await
            .unwrap_err();
        assert!(matches!(err, InferenceError::ModelNotFound(_)));
    }

    #[tokio::test]
    async fn from_name_stub_forces_stub() {
        let engine = InferenceEngine::from_name(Some("stub"));
        assert_eq!(engine.backend_kind(), BackendKind::Stub);
    }

    #[cfg(feature = "candle")]
    #[tokio::test]
    async fn default_new_is_candle_when_feature_on() {
        let engine = InferenceEngine::new();
        assert_eq!(engine.backend_kind(), BackendKind::Candle);
    }

    #[cfg(not(feature = "candle"))]
    #[tokio::test]
    async fn default_new_is_stub_when_feature_off() {
        let engine = InferenceEngine::new();
        assert_eq!(engine.backend_kind(), BackendKind::Stub);
    }
}
