//! W5.2 — aggregated LSP diagnostic registry.
//!
//! LSP servers emit `textDocument/publishDiagnostics` notifications
//! asynchronously as they analyse the workspace. `LspDiagnosticRegistry`
//! collects the latest snapshot per file so the TUI passive-feedback
//! service can push toasts without re-querying the server.
//!
//! Keyed on the file path; each write replaces the full diagnostic
//! list for that file (matches LSP semantics — a server pushes the
//! *current* set, not deltas).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

impl DiagnosticSeverity {
    pub fn from_lsp_int(v: u64) -> Self {
        match v {
            1 => Self::Error,
            2 => Self::Warning,
            3 => Self::Information,
            _ => Self::Hint,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub file: PathBuf,
    pub line: u32,
    pub column: u32,
    pub severity: DiagnosticSeverity,
    pub message: String,
    /// Optional LSP source (e.g. `"rust-analyzer"`, `"pyright"`).
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Default)]
pub struct LspDiagnosticRegistry {
    by_file: RwLock<HashMap<PathBuf, Vec<Diagnostic>>>,
    /// Monotonic counter — incremented on every publish. TUI uses
    /// this to know when to re-render the feedback panel.
    revision: std::sync::atomic::AtomicU64,
}

impl LspDiagnosticRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the full diagnostic list for `file`. Matches LSP's
    /// pub-sub contract — the server always sends the current set.
    pub async fn publish(&self, file: PathBuf, diagnostics: Vec<Diagnostic>) {
        let mut guard = self.by_file.write().await;
        if diagnostics.is_empty() {
            guard.remove(&file);
        } else {
            guard.insert(file, diagnostics);
        }
        self.revision
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    pub async fn for_file(&self, file: &Path) -> Vec<Diagnostic> {
        self.by_file
            .read()
            .await
            .get(file)
            .cloned()
            .unwrap_or_default()
    }

    /// Flattened snapshot across every tracked file, sorted by
    /// severity (Error first) then path. The TUI renders in this
    /// order so errors bubble to the top.
    pub async fn snapshot(&self) -> Vec<Diagnostic> {
        let guard = self.by_file.read().await;
        let mut all: Vec<Diagnostic> = guard.values().flat_map(|v| v.iter().cloned()).collect();
        all.sort_by(|a, b| {
            severity_rank(&a.severity)
                .cmp(&severity_rank(&b.severity))
                .then(a.file.cmp(&b.file))
                .then(a.line.cmp(&b.line))
        });
        all
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub async fn clear(&self) {
        self.by_file.write().await.clear();
        self.revision
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

fn severity_rank(s: &DiagnosticSeverity) -> u8 {
    match s {
        DiagnosticSeverity::Error => 0,
        DiagnosticSeverity::Warning => 1,
        DiagnosticSeverity::Information => 2,
        DiagnosticSeverity::Hint => 3,
    }
}

/// Shared handle to a registry. The TUI's passive-feedback service
/// holds one of these; the LSP transport loop clones it and
/// `publish`es on each notification.
pub type DiagnosticRegistry = Arc<LspDiagnosticRegistry>;

#[cfg(test)]
mod tests {
    use super::*;

    fn diag(f: &str, line: u32, sev: DiagnosticSeverity, msg: &str) -> Diagnostic {
        Diagnostic {
            file: PathBuf::from(f),
            line,
            column: 0,
            severity: sev,
            message: msg.into(),
            source: None,
        }
    }

    #[tokio::test]
    async fn publish_and_retrieve_file_scoped() {
        let reg = LspDiagnosticRegistry::new();
        reg.publish(
            PathBuf::from("a.rs"),
            vec![diag("a.rs", 3, DiagnosticSeverity::Error, "boom")],
        )
        .await;
        let out = reg.for_file(Path::new("a.rs")).await;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].line, 3);
    }

    #[tokio::test]
    async fn empty_publish_clears_file() {
        let reg = LspDiagnosticRegistry::new();
        reg.publish(
            PathBuf::from("a.rs"),
            vec![diag("a.rs", 3, DiagnosticSeverity::Error, "boom")],
        )
        .await;
        reg.publish(PathBuf::from("a.rs"), Vec::new()).await;
        assert!(reg.for_file(Path::new("a.rs")).await.is_empty());
    }

    #[tokio::test]
    async fn snapshot_sorted_error_first() {
        let reg = LspDiagnosticRegistry::new();
        reg.publish(
            PathBuf::from("a.rs"),
            vec![diag("a.rs", 1, DiagnosticSeverity::Warning, "w")],
        )
        .await;
        reg.publish(
            PathBuf::from("b.rs"),
            vec![diag("b.rs", 1, DiagnosticSeverity::Error, "e")],
        )
        .await;
        let snap = reg.snapshot().await;
        assert_eq!(snap[0].severity, DiagnosticSeverity::Error);
        assert_eq!(snap[1].severity, DiagnosticSeverity::Warning);
    }

    #[tokio::test]
    async fn revision_increments_on_publish() {
        let reg = LspDiagnosticRegistry::new();
        let r0 = reg.revision();
        reg.publish(
            PathBuf::from("x"),
            vec![diag("x", 0, DiagnosticSeverity::Hint, "h")],
        )
        .await;
        assert!(reg.revision() > r0);
        let r1 = reg.revision();
        reg.clear().await;
        assert!(reg.revision() > r1);
    }

    #[tokio::test]
    async fn for_file_missing_returns_empty() {
        let reg = LspDiagnosticRegistry::new();
        assert!(reg.for_file(Path::new("nope")).await.is_empty());
    }

    #[test]
    fn severity_from_lsp_int_maps_standard_values() {
        assert_eq!(
            DiagnosticSeverity::from_lsp_int(1),
            DiagnosticSeverity::Error
        );
        assert_eq!(
            DiagnosticSeverity::from_lsp_int(2),
            DiagnosticSeverity::Warning
        );
        assert_eq!(
            DiagnosticSeverity::from_lsp_int(3),
            DiagnosticSeverity::Information
        );
        assert_eq!(
            DiagnosticSeverity::from_lsp_int(4),
            DiagnosticSeverity::Hint
        );
        assert_eq!(
            DiagnosticSeverity::from_lsp_int(99),
            DiagnosticSeverity::Hint
        );
    }

    #[tokio::test]
    async fn same_file_publish_replaces_not_appends() {
        let reg = LspDiagnosticRegistry::new();
        reg.publish(
            PathBuf::from("a.rs"),
            vec![
                diag("a.rs", 1, DiagnosticSeverity::Error, "e1"),
                diag("a.rs", 2, DiagnosticSeverity::Error, "e2"),
            ],
        )
        .await;
        reg.publish(
            PathBuf::from("a.rs"),
            vec![diag("a.rs", 3, DiagnosticSeverity::Warning, "w1")],
        )
        .await;
        let out = reg.for_file(Path::new("a.rs")).await;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].severity, DiagnosticSeverity::Warning);
    }
}
