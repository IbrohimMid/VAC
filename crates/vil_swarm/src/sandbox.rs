//! Sandboxed subagent runtime — production-grade lifecycle management.
//!
//! Ephemeral: isolated overlay dir, auto-cleaned on complete/fail.
//! Persistent: overlay survives across tasks in .vac/sandboxes/<id>/.
//!
//! Subagents write to overlay, not directly to repo.
//! Parent agent reviews SandboxPatchResult before merging.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxMode {
    Ephemeral,
    Persistent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxSpec {
    pub mode: SandboxMode,
    pub working_dir: PathBuf,
    pub mount_project_readonly: bool,
    pub allow_shell: bool,
    pub max_runtime_secs: u64,
    /// Tools explicitly allowed (empty = use default policy)
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// Tools explicitly denied
    #[serde(default)]
    pub denied_tools: Vec<String>,
}

impl Default for SandboxSpec {
    fn default() -> Self {
        Self {
            mode: SandboxMode::Ephemeral,
            working_dir: PathBuf::from("."),
            mount_project_readonly: true,
            allow_shell: false,
            max_runtime_secs: 300,
            allowed_tools: vec![],
            denied_tools: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxPatchResult {
    pub created_files: Vec<String>,
    pub modified_files: Vec<String>,
    /// Unified diff of all changes
    pub patch_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxStatus {
    Active,
    Completed,
    Failed(String),
    Cleaned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxHandle {
    pub id: Uuid,
    pub mode: SandboxMode,
    pub status: SandboxStatus,
    pub working_dir: PathBuf,
    pub overlay_dir: PathBuf,
    pub created_at: DateTime<Utc>,
    pub task_description: String,
    pub patch_result: Option<SandboxPatchResult>,
}

pub struct SandboxRegistry {
    sandboxes: RwLock<HashMap<Uuid, SandboxHandle>>,
    base_dir: PathBuf,
}

impl SandboxRegistry {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { sandboxes: RwLock::new(HashMap::new()), base_dir }
    }

    pub fn with_project_root(project_root: &Path) -> Self {
        Self::new(project_root.join(".vac/sandboxes"))
    }

    /// Spawn a sandbox with the given spec. Returns the handle.
    pub async fn spawn(&self, spec: &SandboxSpec, task_description: &str) -> SandboxHandle {
        let id = Uuid::new_v4();

        let overlay_dir = match spec.mode {
            SandboxMode::Ephemeral => {
                // Use system temp dir for ephemeral
                std::env::temp_dir().join(format!("vac-sandbox-{id}"))
            }
            SandboxMode::Persistent => {
                self.base_dir.join(id.to_string())
            }
        };
        let _ = std::fs::create_dir_all(&overlay_dir);

        let handle = SandboxHandle {
            id,
            mode: spec.mode.clone(),
            status: SandboxStatus::Active,
            working_dir: spec.working_dir.clone(),
            overlay_dir,
            created_at: Utc::now(),
            task_description: task_description.to_string(),
            patch_result: None,
        };

        self.sandboxes.write().await.insert(id, handle.clone());
        tracing::info!(%id, mode = ?spec.mode, "Sandbox spawned");
        handle
    }

    pub async fn complete(&self, id: Uuid, patch: Option<SandboxPatchResult>) {
        let mut sandboxes = self.sandboxes.write().await;
        if let Some(h) = sandboxes.get_mut(&id) {
            h.status = SandboxStatus::Completed;
            h.patch_result = patch;
            if h.mode == SandboxMode::Ephemeral {
                let _ = std::fs::remove_dir_all(&h.overlay_dir);
                h.status = SandboxStatus::Cleaned;
            }
        }
        tracing::debug!(%id, "Sandbox completed");
    }

    /// Build a patch result by diffing overlay files against the working_dir originals.
    pub async fn build_patch(&self, id: Uuid) -> Option<SandboxPatchResult> {
        let sandboxes = self.sandboxes.read().await;
        let h = sandboxes.get(&id)?;

        let mut created = Vec::new();
        let mut modified = Vec::new();
        let mut diff_lines = Vec::new();

        let Ok(entries) = std::fs::read_dir(&h.overlay_dir) else { return None };

        for entry in entries.flatten() {
            let overlay_path = entry.path();
            let file_name = overlay_path.file_name()?.to_string_lossy().to_string();
            // Reverse the __ → / encoding used in journal
            let rel_path = file_name.replace("__", "/");
            let original_path = h.working_dir.join(&rel_path);

            let overlay_content = std::fs::read_to_string(&overlay_path).unwrap_or_default();

            if original_path.exists() {
                let original_content = std::fs::read_to_string(&original_path).unwrap_or_default();
                if original_content != overlay_content {
                    modified.push(rel_path.clone());
                    diff_lines.push(format!("--- a/{rel_path}"));
                    diff_lines.push(format!("+++ b/{rel_path}"));
                    // Simple line-level diff
                    for (i, (orig, new)) in original_content.lines()
                        .zip(overlay_content.lines())
                        .enumerate()
                    {
                        if orig != new {
                            diff_lines.push(format!("@@ line {} @@", i + 1));
                            diff_lines.push(format!("-{orig}"));
                            diff_lines.push(format!("+{new}"));
                        }
                    }
                }
            } else {
                created.push(rel_path.clone());
                diff_lines.push(format!("--- /dev/null"));
                diff_lines.push(format!("+++ b/{rel_path}"));
                for line in overlay_content.lines() {
                    diff_lines.push(format!("+{line}"));
                }
            }
        }

        Some(SandboxPatchResult {
            created_files: created,
            modified_files: modified,
            patch_summary: diff_lines.join("\n"),
        })
    }

    /// Apply a sandbox patch to the real working directory.
    pub async fn merge_patch(&self, id: Uuid) -> Result<(), String> {
        let patch = self.build_patch(id).await
            .ok_or_else(|| format!("No patch available for sandbox {id}"))?;

        let sandboxes = self.sandboxes.read().await;
        let h = sandboxes.get(&id)
            .ok_or_else(|| format!("Sandbox {id} not found"))?;

        // Apply: copy overlay files to working_dir
        let Ok(entries) = std::fs::read_dir(&h.overlay_dir) else {
            return Err(format!("Cannot read overlay dir: {}", h.overlay_dir.display()));
        };

        for entry in entries.flatten() {
            let overlay_path = entry.path();
            let file_name = overlay_path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let rel_path = file_name.replace("__", "/");
            let dest = h.working_dir.join(&rel_path);

            if let Some(parent) = dest.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::copy(&overlay_path, &dest)
                .map_err(|e| format!("Failed to apply patch for {rel_path}: {e}"))?;
        }

        tracing::info!(%id, created = patch.created_files.len(), modified = patch.modified_files.len(), "Sandbox patch merged");
        Ok(())
    }

    pub async fn fail(&self, id: Uuid, reason: String) {
        let mut sandboxes = self.sandboxes.write().await;
        if let Some(h) = sandboxes.get_mut(&id) {
            h.status = SandboxStatus::Failed(reason.clone());
            if h.mode == SandboxMode::Ephemeral {
                let _ = std::fs::remove_dir_all(&h.overlay_dir);
                h.status = SandboxStatus::Cleaned;
            }
        }
        tracing::warn!(%id, %reason, "Sandbox failed");
    }

    pub async fn get(&self, id: Uuid) -> Option<SandboxHandle> {
        self.sandboxes.read().await.get(&id).cloned()
    }

    pub async fn list_active(&self) -> Vec<SandboxHandle> {
        self.sandboxes.read().await.values()
            .filter(|h| h.status == SandboxStatus::Active)
            .cloned()
            .collect()
    }

    pub async fn teardown_all(&self) {
        let mut sandboxes = self.sandboxes.write().await;
        for h in sandboxes.values_mut() {
            if h.status == SandboxStatus::Active {
                let _ = std::fs::remove_dir_all(&h.overlay_dir);
                h.status = SandboxStatus::Cleaned;
            }
        }
        tracing::info!("All sandboxes torn down");
    }
}
