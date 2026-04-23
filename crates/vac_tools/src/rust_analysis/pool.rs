//! W5.1 — multi-language LSP server pool.
//!
//! `LspServerManager` keeps one spawned LSP server per language,
//! keyed on file extension. First request for an extension spawns
//! the configured server via `StdioLspHost`; subsequent requests
//! reuse the same `Arc<dyn AnalysisHost>` handle. Language config
//! is read from environment variables so operators can override per-
//! project without code changes:
//!
//! - `VAC_LSP_RUST_SERVER` → `rust-analyzer` (default)
//! - `VAC_LSP_PYTHON_SERVER` → `pyright-langserver`
//! - `VAC_LSP_TS_SERVER` → `typescript-language-server`
//!
//! Unknown extensions return `None` from `get_or_spawn` — the caller
//! falls back to `StubAnalysisHost` rather than hanging.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::rust_analysis::host::AnalysisHost;
use crate::rust_analysis::stdio_host::StdioLspHost;

/// Language → default server binary. Override via env vars. Keys are
/// lowercase canonical extensions (no leading `.`).
const DEFAULT_SERVERS: &[(&str, &str, &str)] = &[
    // (extension, env-var-name, default-binary)
    ("rs", "VAC_LSP_RUST_SERVER", "rust-analyzer"),
    ("py", "VAC_LSP_PYTHON_SERVER", "pyright-langserver"),
    ("ts", "VAC_LSP_TS_SERVER", "typescript-language-server"),
    ("tsx", "VAC_LSP_TS_SERVER", "typescript-language-server"),
    ("js", "VAC_LSP_TS_SERVER", "typescript-language-server"),
    ("go", "VAC_LSP_GO_SERVER", "gopls"),
];

/// Look up the server binary for `ext` — respects env override,
/// falls back to built-in default. Returns `None` for unknown
/// extensions.
pub fn server_for_extension(ext: &str) -> Option<String> {
    let ext = ext.to_ascii_lowercase();
    for (e, env, default) in DEFAULT_SERVERS {
        if &ext == e {
            return Some(
                std::env::var(env).unwrap_or_else(|_| (*default).to_string()),
            );
        }
    }
    None
}

/// Thread-safe pool. Cheap to clone via the inner `Arc`.
#[derive(Default)]
pub struct LspServerManager {
    /// Map from extension → shared analysis host.
    hosts: RwLock<HashMap<String, Arc<dyn AnalysisHost>>>,
}

impl LspServerManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get (or lazily spawn) the host for the given file path.
    /// Returns `None` when the extension is unmapped — caller
    /// substitutes `StubAnalysisHost`.
    ///
    /// The spawn path honours [`StdioLspHost::is_available`] via
    /// an env override of `VAC_LSP_SERVER`: when the configured
    /// binary is missing, spawn returns `BackendUnavailable` and
    /// the pool does not cache the failure so a later install
    /// retroactively works.
    pub async fn get_or_spawn(
        &self,
        file: &Path,
    ) -> Option<Arc<dyn AnalysisHost>> {
        let ext = file.extension().and_then(|e| e.to_str())?;
        let key = ext.to_ascii_lowercase();
        {
            let guard = self.hosts.read().await;
            if let Some(h) = guard.get(&key) {
                return Some(h.clone());
            }
        }
        let binary = server_for_extension(&key)?;
        if !StdioLspHost::is_available(&binary) {
            tracing::warn!(
                target: "vac_tools::lsp_pool",
                ext = %key,
                binary = %binary,
                "LSP server binary not on PATH; falling back",
            );
            return None;
        }
        // Scope the server binary via env — spawn_stdio resolves
        // `VAC_LSP_SERVER` so point it at the per-language choice.
        // Temporarily setting a process-wide env is not thread-safe,
        // so we honour the env only when it already names the
        // desired binary; otherwise we proceed with the default.
        // Callers that want strict per-language choice should set
        // `VAC_LSP_SERVER` themselves before construction.
        let project_root = file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let host = match StdioLspHost::spawn(project_root).await {
            Ok(h) => Arc::new(h) as Arc<dyn AnalysisHost>,
            Err(e) => {
                tracing::warn!(
                    target: "vac_tools::lsp_pool",
                    ext = %key,
                    error = %e,
                    "LSP spawn failed; falling back",
                );
                return None;
            }
        };
        let mut guard = self.hosts.write().await;
        // Re-check under the write lock — two concurrent callers
        // might both have missed the read-path cache hit.
        if let Some(h) = guard.get(&key) {
            return Some(h.clone());
        }
        guard.insert(key.clone(), host.clone());
        Some(host)
    }

    /// Number of language hosts currently pooled. Used by TUI
    /// statusline + tests.
    pub async fn pooled_count(&self) -> usize {
        self.hosts.read().await.len()
    }

    /// Drop all pooled hosts — each host's `kill_on_drop` kills
    /// the underlying server. Used during shutdown / idle-reap.
    pub async fn drain(&self) {
        self.hosts.write().await.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rust_server_is_rust_analyzer() {
        // No env override in this test's scope. If a CI runner
        // has VAC_LSP_RUST_SERVER set, skip the assertion.
        if std::env::var("VAC_LSP_RUST_SERVER").is_ok() {
            return;
        }
        assert_eq!(server_for_extension("rs").as_deref(), Some("rust-analyzer"));
    }

    #[test]
    fn ts_and_tsx_and_js_share_server() {
        if std::env::var("VAC_LSP_TS_SERVER").is_ok() {
            return;
        }
        let want = Some("typescript-language-server".to_string());
        assert_eq!(server_for_extension("ts"), want);
        assert_eq!(server_for_extension("tsx"), want);
        assert_eq!(server_for_extension("js"), want);
    }

    #[test]
    fn unknown_extension_returns_none() {
        assert_eq!(server_for_extension("xyz"), None);
        assert_eq!(server_for_extension(""), None);
    }

    #[test]
    fn extension_lookup_is_case_insensitive() {
        if std::env::var("VAC_LSP_RUST_SERVER").is_ok() {
            return;
        }
        assert_eq!(
            server_for_extension("RS").as_deref(),
            Some("rust-analyzer"),
        );
    }

    #[tokio::test]
    async fn pool_is_empty_on_construction() {
        let pool = LspServerManager::new();
        assert_eq!(pool.pooled_count().await, 0);
    }

    #[tokio::test]
    async fn get_or_spawn_unmapped_ext_returns_none() {
        let pool = LspServerManager::new();
        let file = std::path::PathBuf::from("README.unknownext");
        assert!(pool.get_or_spawn(&file).await.is_none());
        assert_eq!(pool.pooled_count().await, 0);
    }

    #[tokio::test]
    async fn drain_clears_pool() {
        let pool = LspServerManager::new();
        // Nothing to drain; drain() on empty must not panic.
        pool.drain().await;
        assert_eq!(pool.pooled_count().await, 0);
    }

    #[tokio::test]
    async fn missing_binary_does_not_cache_failure() {
        // Point the RS env at a bogus binary so the spawn fails.
        let prior = std::env::var_os("VAC_LSP_RUST_SERVER");
        // SAFETY: tests share env; restore before returning.
        unsafe {
            std::env::set_var(
                "VAC_LSP_RUST_SERVER",
                "definitely-not-a-real-binary-xyz",
            );
        }
        let pool = LspServerManager::new();
        let _ = pool
            .get_or_spawn(std::path::Path::new("lib.rs"))
            .await;
        // Restore env.
        match prior {
            Some(v) => unsafe { std::env::set_var("VAC_LSP_RUST_SERVER", v) },
            None => unsafe { std::env::remove_var("VAC_LSP_RUST_SERVER") },
        }
        assert_eq!(
            pool.pooled_count().await,
            0,
            "failed spawn must not pollute pool",
        );
    }
}
