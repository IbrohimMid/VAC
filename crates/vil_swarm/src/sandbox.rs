//! Sandboxed subagent lifecycle — ephemeral and persistent execution modes.
//!
//! Ephemeral: isolated context, cleaned up after task completes.
//! Persistent: shared context, survives across tasks (e.g. long-running analysis).
//!
//! All subagents remain subject to VIL semantic planner and authoritative knowledge.
//! Sandbox = execution isolation, NOT semantic independence.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxMode {
    /// Isolated context, cleaned up after task. Use for: analysis, experiments, compile checks.
    Ephemeral,
    /// Shared context, survives across tasks. Use for: long-running multi-step work.
    Persistent,
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
    pub working_dir: std::path::PathBuf,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub task_description: String,
}

/// Registry of active sandboxes.
pub struct SandboxRegistry {
    sandboxes: RwLock<HashMap<Uuid, SandboxHandle>>,
}

impl Default for SandboxRegistry {
    fn default() -> Self {
        Self { sandboxes: RwLock::new(HashMap::new()) }
    }
}

impl SandboxRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn a new sandbox. Returns the handle ID.
    pub async fn spawn(
        &self,
        mode: SandboxMode,
        working_dir: std::path::PathBuf,
        task_description: &str,
    ) -> Uuid {
        let id = Uuid::new_v4();
        let handle = SandboxHandle {
            id,
            mode,
            status: SandboxStatus::Active,
            working_dir,
            created_at: chrono::Utc::now(),
            task_description: task_description.to_string(),
        };
        self.sandboxes.write().await.insert(id, handle);
        tracing::info!(%id, "Sandbox spawned");
        id
    }

    pub async fn complete(&self, id: Uuid) {
        if let Some(h) = self.sandboxes.write().await.get_mut(&id) {
            h.status = SandboxStatus::Completed;
        }
        // Ephemeral sandboxes are cleaned immediately on completion
        self.cleanup_ephemeral(id).await;
    }

    pub async fn fail(&self, id: Uuid, reason: String) {
        if let Some(h) = self.sandboxes.write().await.get_mut(&id) {
            h.status = SandboxStatus::Failed(reason);
        }
        self.cleanup_ephemeral(id).await;
    }

    async fn cleanup_ephemeral(&self, id: Uuid) {
        let is_ephemeral = self.sandboxes.read().await
            .get(&id)
            .map(|h| h.mode == SandboxMode::Ephemeral)
            .unwrap_or(false);

        if is_ephemeral {
            if let Some(h) = self.sandboxes.write().await.get_mut(&id) {
                h.status = SandboxStatus::Cleaned;
            }
            tracing::debug!(%id, "Ephemeral sandbox cleaned");
        }
    }

    pub async fn get(&self, id: Uuid) -> Option<SandboxHandle> {
        self.sandboxes.read().await.get(&id).cloned()
    }

    pub async fn list_active(&self) -> Vec<SandboxHandle> {
        self.sandboxes.read().await
            .values()
            .filter(|h| h.status == SandboxStatus::Active)
            .cloned()
            .collect()
    }

    /// Teardown all persistent sandboxes (e.g. on engine shutdown).
    pub async fn teardown_all(&self) {
        let ids: Vec<Uuid> = self.sandboxes.read().await.keys().cloned().collect();
        for id in ids {
            if let Some(h) = self.sandboxes.write().await.get_mut(&id) {
                if h.status == SandboxStatus::Active {
                    h.status = SandboxStatus::Cleaned;
                }
            }
        }
        tracing::info!("All sandboxes torn down");
    }
}
