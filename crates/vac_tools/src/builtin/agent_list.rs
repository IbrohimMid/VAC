//! NS.1 — `agent_list` tool. Read-only: returns the 5 built-in
//! subagent kinds the LLM can choose from. Live dispatch
//! (`dispatch_agent_tool`) requires a `SubagentDispatchContext`
//! plumbed through `ToolContext`; that wiring lands in a later
//! part alongside the TUI event-loop stream migration.

use async_trait::async_trait;
use serde::Deserialize;

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

#[derive(Debug, Deserialize, Default)]
struct Input {}

pub struct AgentListTool;

impl AgentListTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AgentListTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for AgentListTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &str {
        "agent_list"
    }

    fn description(&self) -> &str {
        "List built-in subagent kinds (explore, plan, verify, general-purpose, statusline-setup). Use before invoking Agent to pick the right subagent_type."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn trust_requirement(&self) -> &str {
        "safe"
    }

    fn risk_level(&self) -> &str {
        "safe"
    }

    async fn execute(
        &self,
        _args: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        // Mirror of vac_session_engine::agent_tool::BUILT_IN_SUBAGENTS.
        // Kept in sync manually — the kinds are a stable operator
        // vocabulary and change rarely.
        let agents = serde_json::json!([
            {"kind": "explore", "title": "Explore", "description": "Search codebase, gather context, answer questions about files."},
            {"kind": "plan", "title": "Plan", "description": "Design an implementation plan before writing code."},
            {"kind": "verify", "title": "Verify", "description": "Independently review changes for correctness and regressions."},
            {"kind": "general-purpose", "title": "General Purpose", "description": "Multi-step research or implementation task with broad tool access."},
            {"kind": "statusline-setup", "title": "Statusline Setup", "description": "Configure the TUI statusline."}
        ]);
        Ok(serde_json::json!({ "agents": agents }))
    }
}
