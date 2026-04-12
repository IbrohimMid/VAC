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
