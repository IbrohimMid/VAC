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
pub fn build_sandbox_context(overlay_dir: std::path::PathBuf) -> ToolContext {
    ToolContext::new(overlay_dir).with_zone(AgentZone::SandboxedSubagent)
}

/// Build a parent-level tool context.
pub fn build_parent_context(root: std::path::PathBuf) -> ToolContext {
    ToolContext::new(root)
}
