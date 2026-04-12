//! High-level VilLspService — manages vil-lsp lifecycle and diagnostic snapshots.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tracing::{info, warn};

use super::client::VilLspClient;
use super::types::{LspDiagnostic, LspPromptContext, LspSeverity, LspWorkspaceSnapshot};
use crate::config::VilLspConfig;

pub struct VilLspService {
    client: Arc<VilLspClient>,
    snapshot: Arc<RwLock<LspWorkspaceSnapshot>>,
    project_root: PathBuf,
    cache_path: PathBuf,
}

impl VilLspService {
    pub async fn new(config: &VilLspConfig, project_root: PathBuf) -> anyhow::Result<Self> {
        let cache_path = project_root.join(".vac/cache/vil_lsp_diagnostics.json");

        let (diag_tx, mut diag_rx) = mpsc::unbounded_channel::<(String, Vec<LspDiagnostic>)>();
        let snapshot = Arc::new(RwLock::new(LspWorkspaceSnapshot::default()));

        // Spawn background task to merge incoming diagnostics into snapshot
        let snap = snapshot.clone();
        let cache = cache_path.clone();
        tokio::spawn(async move {
            while let Some((uri, diags)) = diag_rx.recv().await {
                let mut s = snap.write().await;
                // Replace diagnostics for this file
                let path = super::protocol::uri_to_path(&uri);
                s.diagnostics.retain(|d| d.file_path != path);
                s.diagnostics.extend(diags);
                s.rebuild_counts();
                // Persist to cache
                if let Ok(json) = serde_json::to_string_pretty(&*s) {
                    if let Some(parent) = cache.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::write(&cache, json);
                }
            }
        });

        let client = VilLspClient::start(
            &config.binary_path,
            &project_root,
            &config.arguments,
            diag_tx,
        ).await?;

        Ok(Self {
            client: Arc::new(client),
            snapshot,
            project_root,
            cache_path,
        })
    }

    /// Open all .rs files in the workspace to trigger diagnostics.
    pub async fn analyze_workspace(&self) -> anyhow::Result<()> {
        let files = collect_rs_files(&self.project_root);
        info!(count = files.len(), "vil-lsp: analyzing workspace");
        self.open_files_batch(&files).await?;
        // Give LSP time to process
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        Ok(())
    }

    /// Open specific files to refresh their diagnostics.
    pub async fn analyze_files(&self, files: &[String]) -> anyhow::Result<()> {
        let paths: Vec<PathBuf> = files.iter().map(|f| {
            let p = PathBuf::from(f);
            if p.is_absolute() { p } else { self.project_root.join(f) }
        }).collect();
        self.open_files_batch(&paths).await?;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        Ok(())
    }

    pub async fn snapshot(&self) -> LspWorkspaceSnapshot {
        self.snapshot.read().await.clone()
    }

    pub async fn prompt_context(&self, max_items: usize) -> LspPromptContext {
        let snap = self.snapshot.read().await;
        let top_findings: Vec<String> = snap.diagnostics.iter()
            .filter(|d| matches!(d.severity, LspSeverity::Error | LspSeverity::Warning))
            .take(max_items)
            .map(|d| {
                let file = d.file_path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?");
                format!("{}: {}", file, d.message)
            })
            .collect();

        LspPromptContext {
            total_errors: snap.total_errors,
            total_warnings: snap.total_warnings,
            top_findings,
        }
    }

    pub async fn diagnostics_for_files(&self, files: &[String]) -> Vec<LspDiagnostic> {
        let snap = self.snapshot.read().await;
        snap.diagnostics.iter()
            .filter(|d| {
                files.iter().any(|f| {
                    let p = PathBuf::from(f);
                    d.file_path == p || d.file_path.ends_with(f)
                })
            })
            .cloned()
            .collect()
    }

    pub async fn shutdown(&self) {
        if let Err(e) = self.client.shutdown().await {
            warn!(error = %e, "vil-lsp shutdown error");
        }
    }

    async fn open_files_batch(&self, paths: &[PathBuf]) -> anyhow::Result<()> {
        for path in paths {
            if let Ok(text) = std::fs::read_to_string(path) {
                let _ = self.client.open_file(path, text).await;
            }
        }
        Ok(())
    }
}

fn collect_rs_files(root: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path();
            p.extension().is_some_and(|x| x == "rs")
                && !p.to_string_lossy().contains("/target/")
                && !p.to_string_lossy().contains("/.vac/")
        })
        .map(|e| e.into_path())
        .collect()
}
