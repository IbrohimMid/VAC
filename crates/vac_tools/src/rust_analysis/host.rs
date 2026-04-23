//! Analysis host trait and no-op backend.
//!
//! The actual rust-analyzer integration (M1 follow-up) will back this
//! trait with `ra_ap_ide::Analysis` running on a dedicated worker task.
//! Until then, this file ships the trait plus a stub that mirrors the
//! "not yet implemented" semantics used throughout the VIL inference
//! stub.

use async_trait::async_trait;
use std::path::PathBuf;
use thiserror::Error;

/// A single analysis request. Intentionally tiny; extend with new variants
/// rather than adding optional fields so the trait stays exhaustive.
#[derive(Debug, Clone)]
pub enum AnalysisRequest {
    /// Return the list of symbols (name + span) defined in a file.
    FileSymbols { path: PathBuf },
    /// Resolve a symbol by name, returning every definition the analyzer
    /// can see across the workspace.
    ResolveSymbol { name: String },
    /// Explain a lifetime / borrow-check diagnostic the compiler emitted.
    ExplainLifetime { diagnostic_code: String },
    /// Fetch workspace diagnostics.
    WorkspaceDiagnostics,
    /// Hover over a symbol.
    Hover { file: PathBuf, line: u32, column: u32 },
    /// Go to definition.
    GotoDefinition { file: PathBuf, line: u32, column: u32 },
}

/// A single analysis response. Variants parallel [`AnalysisRequest`].
#[derive(Debug, Clone)]
pub enum AnalysisResponse {
    FileSymbols(Vec<Symbol>),
    ResolveSymbol(Vec<Symbol>),
    ExplainLifetime { summary: String, hints: Vec<String> },
    WorkspaceDiagnostics(Vec<serde_json::Value>),
    Hover(String),
    GotoDefinition(Vec<Symbol>),
}

/// A symbol entry. Kept deliberately format-free so the real backend can
/// populate it from `ra_ap_ide::StructureNode` / `SymbolKind` without a
/// lossy round-trip.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
    pub file: PathBuf,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error("rust-analyzer backend not available (M1 scaffold)")]
    BackendUnavailable,
    #[error("analysis failed: {0}")]
    Failed(String),
    #[error("unsupported request: {0}")]
    Unsupported(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type AnalysisResult<T> = Result<T, AnalysisError>;

/// Contract for any rust-analyzer-like analysis backend.
#[async_trait]
pub trait AnalysisHost: Send + Sync {
    async fn analyze(&self, request: AnalysisRequest) -> AnalysisResult<AnalysisResponse>;

    /// Fast opt-out so callers can skip the async hop when the only host
    /// available is the stub.
    fn is_real(&self) -> bool {
        false
    }
}

/// Always-available no-op backend. Every request returns
/// [`AnalysisError::BackendUnavailable`], matching the existing
/// vil_inference stub's "not yet implemented" shape.
#[derive(Debug, Default, Clone, Copy)]
pub struct StubAnalysisHost;

#[async_trait]
impl AnalysisHost for StubAnalysisHost {
    async fn analyze(&self, _request: AnalysisRequest) -> AnalysisResult<AnalysisResponse> {
        tracing::warn!("rust-analyzer host not implemented (M1 Paket E follow-up)");
        Err(AnalysisError::BackendUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_rejects_every_request() {
        let host = StubAnalysisHost;
        assert!(!host.is_real());
        let err = host
            .analyze(AnalysisRequest::ResolveSymbol { name: "foo".into() })
            .await
            .unwrap_err();
        assert!(matches!(err, AnalysisError::BackendUnavailable));
    }

    #[tokio::test]
    async fn symbol_roundtrip_is_trivial() {
        let s = Symbol {
            name: "foo".into(),
            kind: "fn".into(),
            file: PathBuf::from("src/lib.rs"),
            line: 1,
            column: 4,
        };
        assert_eq!(s.name, "foo");
    }
}
