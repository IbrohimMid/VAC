//! Subagent/delegation boundary helpers.
//! Extracted from orchestrator.rs — generic mechanics, not VIL semantic policy.

use vil_llm::provider::{Message, ToolDefinition};
use vac_tools::registry::{AgentZone, ToolContext};
use crate::agent::AgentRole;

/// Build the initial message list for a subagent task.
pub fn build_subagent_messages(role: &AgentRole, task_description: &str) -> Vec<Message> {
    vec![
        Message::system(role.system_prompt().to_string()),
        Message::user(task_description.to_string()),
    ]
}

/// Build tool definitions from a registry listing.
/// Filtering by allowed_tools is enforced at execution time via SandboxSpec in the router.
pub async fn build_tool_defs(
    registry: &vac_tools::registry::ToolRegistry,
) -> Vec<ToolDefinition> {
    registry
        .list()
        .await
        .into_iter()
        .map(|t| ToolDefinition {
            name: t.name,
            description: t.description,
            input_schema: t.input_schema,
        })
        .collect()
}

/// Build a sandboxed tool context for a subagent.
///
/// Context leak prevention:
/// - Uses overlay_dir as working_dir (isolated from parent project root)
/// - Strips all env_vars (parent env may contain secrets/credentials)
/// - Assigns a fresh session_id (no parent session state leaks)
/// - Sets zone to SandboxedSubagent (router enforces NeedsApproval → Deny)
pub fn build_sandbox_context(overlay_dir: std::path::PathBuf, privacy_vault: std::sync::Arc<tokio::sync::RwLock<vac_tools::PrivacyVault>>) -> ToolContext {
    let mut ctx = ToolContext::new(overlay_dir).with_zone(AgentZone::SandboxedSubagent);
    // Explicitly clear env_vars to prevent parent environment leaking into subagent
    ctx.env_vars.clear();
    ctx.privacy = privacy_vault;
    ctx
}

/// Build a parent-level tool context.
pub fn build_parent_context(root: std::path::PathBuf, privacy_vault: std::sync::Arc<tokio::sync::RwLock<vac_tools::PrivacyVault>>) -> ToolContext {
    let mut ctx = ToolContext::new(root);
    ctx.privacy = privacy_vault;
    ctx
}
