//! Shared builders for `#[cfg(test)]` modules in builtin tools.

#![cfg(test)]

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

use crate::PrivacyVault;
use crate::registry::{AgentZone, ToolContext};

/// Build a minimal ToolContext for tool tests.
pub fn make_ctx(working_dir: PathBuf, session_id: Uuid) -> ToolContext {
    ToolContext {
        working_dir,
        env_vars: std::collections::HashMap::new(),
        session_id,
        submit_id: None,
        shm: None,
        agent_zone: AgentZone::ParentAgent,
        environment_mode: "host".to_string(),
        privacy: Arc::new(RwLock::new(PrivacyVault::new())),
    }
}
