//! F9.1 — Deterministic mock inference backend.
//!
//! Paket E's real Candle/GGUF/ONNX wiring is still pending (see the
//! scaffold note in `engine.rs`). Downstream crates that want to
//! exercise the `InferenceBackend` trait today need *something* that
//! produces stable output without pulling in FFI or ML weights.
//!
//! `MockBackend` fills that role: every call returns a deterministic
//! response seeded by the prompt + optional user-supplied seed. It is
//! feature-free, dep-free, and always available.

use std::path::Path;

use async_trait::async_trait;

use crate::engine::{
    BackendKind, InferenceBackend, InferenceRequest, LoadedModel,
};
use crate::error::{InferenceError, InferenceResult};

/// Behaviour switch for the mock.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub enum MockBehaviour {
    /// Return `mock:<prompt>` regardless of request.
    #[default]
    EchoPrompt,
    /// Return a fixed canned string (useful for golden tests).
    Canned,
    /// Fail every `infer` call with a synthetic error. Used to
    /// exercise the consumer's error-path wiring.
    AlwaysFail,
}

/// Deterministic inference backend for tests + `--mock` runs.
#[derive(Debug, Clone)]
pub struct MockBackend {
    pub behaviour: MockBehaviour,
    pub canned_response: String,
    /// Optional context-window hint exposed via `LoadedModel`.
    pub max_context_tokens: Option<u32>,
}

impl Default for MockBackend {
    fn default() -> Self {
        Self {
            behaviour: MockBehaviour::EchoPrompt,
            canned_response: "mock response".to_string(),
            max_context_tokens: Some(8_192),
        }
    }
}

impl MockBackend {
    #[must_use]
    pub fn echo() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn canned(response: impl Into<String>) -> Self {
        Self {
            behaviour: MockBehaviour::Canned,
            canned_response: response.into(),
            max_context_tokens: Some(8_192),
        }
    }

    #[must_use]
    pub fn always_fail() -> Self {
        Self {
            behaviour: MockBehaviour::AlwaysFail,
            canned_response: String::new(),
            max_context_tokens: None,
        }
    }
}

#[async_trait]
impl InferenceBackend for MockBackend {
    fn kind(&self) -> BackendKind {
        // Dedicated `Mock` variant so the scheduler / operator
        // filter can distinguish a testing adapter from the
        // production no-op `Stub` fallback.
        BackendKind::Mock
    }

    async fn load(&self, path: &Path) -> InferenceResult<LoadedModel> {
        Ok(LoadedModel {
            path: path.to_path_buf(),
            kind: Some(BackendKind::Mock),
            max_context_tokens: self.max_context_tokens,
        })
    }

    async fn infer(&self, request: &InferenceRequest) -> InferenceResult<String> {
        match self.behaviour {
            MockBehaviour::EchoPrompt => {
                // Clamp by CHARACTER count (not byte length) so
                // non-ASCII prompts don't blow up at a non-UTF-8
                // boundary. `max_tokens == 0` is treated as "no cap"
                // because the mock has no real tokenizer to honor
                // the literal zero — callers wiring real budgets
                // pick a concrete cap.
                let cap = request.max_tokens as usize;
                let body = format!("mock:{}", request.prompt);
                if cap == 0 {
                    return Ok(body);
                }
                let end = body
                    .char_indices()
                    .nth(cap)
                    .map(|(i, _)| i)
                    .unwrap_or(body.len());
                Ok(body[..end].to_string())
            }
            MockBehaviour::Canned => Ok(self.canned_response.clone()),
            MockBehaviour::AlwaysFail => Err(InferenceError::InferenceFailed(
                "mock backend failure (synthetic)".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn echo_returns_prompt_prefixed() {
        let b = MockBackend::echo();
        let out = b
            .infer(&InferenceRequest::new("hello", 128))
            .await
            .unwrap();
        assert_eq!(out, "mock:hello");
    }

    #[tokio::test]
    async fn echo_clamps_to_max_tokens_as_chars() {
        let b = MockBackend::echo();
        let out = b.infer(&InferenceRequest::new("hello world", 6)).await.unwrap();
        // "mock:h" = 6 chars.
        assert_eq!(out.chars().count(), 6, "expected 6 chars, got {out:?}");
    }

    #[tokio::test]
    async fn echo_clamp_is_codepoint_safe_for_non_ascii() {
        // Prior byte-based truncate would panic on a boundary like
        // this one (é is two UTF-8 bytes). Char-boundary clamp must
        // round-trip cleanly.
        let b = MockBackend::echo();
        let out = b.infer(&InferenceRequest::new("héllo", 8)).await.unwrap();
        // "mock:hél" = 8 chars; no panic, exact char count.
        assert_eq!(out.chars().count(), 8, "got {out:?}");
        assert!(out.starts_with("mock:h"));
    }

    #[tokio::test]
    async fn echo_max_tokens_zero_means_no_cap() {
        let b = MockBackend::echo();
        let out = b.infer(&InferenceRequest::new("long prompt here", 0)).await.unwrap();
        assert_eq!(out, "mock:long prompt here");
    }

    #[tokio::test]
    async fn canned_ignores_prompt() {
        let b = MockBackend::canned("always the same");
        let a = b.infer(&InferenceRequest::new("x", 100)).await.unwrap();
        let c = b.infer(&InferenceRequest::new("completely different", 100))
            .await
            .unwrap();
        assert_eq!(a, c);
        assert_eq!(a, "always the same");
    }

    #[tokio::test]
    async fn always_fail_surfaces_inference_error() {
        let b = MockBackend::always_fail();
        let err = b.infer(&InferenceRequest::new("x", 10)).await.unwrap_err();
        assert!(format!("{err}").to_lowercase().contains("mock"));
    }

    #[tokio::test]
    async fn load_records_supplied_path_and_kind() {
        let b = MockBackend::echo();
        let m = b.load(Path::new("/nowhere/model.gguf")).await.unwrap();
        assert_eq!(m.path, Path::new("/nowhere/model.gguf"));
        assert_eq!(m.kind, Some(BackendKind::Mock));
        assert_eq!(m.max_context_tokens, Some(8_192));
    }

    #[test]
    fn mock_kind_is_distinct_from_stub() {
        assert_ne!(MockBackend::echo().kind(), BackendKind::Stub);
        assert_eq!(MockBackend::echo().kind(), BackendKind::Mock);
    }
}
