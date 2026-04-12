//! Spawn Subtask — delegate work to specialist sub-agents.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use vac_tools::registry::{ToolContext, VilTool};
use vac_tools::error::ToolError;
use vil_swarm::{AgentRole, SwarmOrchestrator};

#[derive(Debug, Deserialize)]
struct SpawnSubtaskInput {
    target_role: String,
    task_description: String,
    #[serde(default = "default_wait")]
    wait_for_completion: bool,
}

fn default_wait() -> bool {
    true
}

#[derive(Debug, Serialize)]
struct SpawnSubtaskOutput {
    role: String,
    summary: String,
    modified_files: Vec<String>,
    created_files: Vec<String>,
    tokens_used: u64,
    success: bool,
    error: Option<String>,
}

pub struct SpawnSubtaskTool {
    swarm: Arc<RwLock<SwarmOrchestrator>>,
}

impl SpawnSubtaskTool {
    pub fn new(swarm: Arc<RwLock<SwarmOrchestrator>>) -> Self {
        Self { swarm }
    }

    fn parse_role(role_str: &str) -> Result<AgentRole, ToolError> {
        match role_str.to_lowercase().as_str() {
            "architect" => Ok(AgentRole::Architect),
            "coder" => Ok(AgentRole::Coder),
            "tester" => Ok(AgentRole::Tester),
            "security" => Ok(AgentRole::Security),
            "deploy" => Ok(AgentRole::Deploy),
            "monitor" => Ok(AgentRole::Monitor),
            "optimizer" => Ok(AgentRole::Optimizer),
            "documenter" => Ok(AgentRole::Documenter),
            _ => Err(ToolError::InvalidArguments(format!("Unknown role: {}", role_str))),
        }
    }
}

#[async_trait]
impl VilTool for SpawnSubtaskTool {
    fn name(&self) -> &str {
        "spawn_subtask"
    }

    fn description(&self) -> &str {
        "Spawn a specialist sub-agent to handle a subtask. Valid roles: architect, coder, tester, security, deploy, monitor, optimizer, documenter."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "target_role": {
                    "type": "string",
                    "description": "Role to spawn: architect, coder, tester, security, deploy, monitor, optimizer, or documenter"
                },
                "task_description": {
                    "type": "string",
                    "description": "Description of the task to delegate"
                },
                "wait_for_completion": {
                    "type": "boolean",
                    "default": true,
                    "description": "Wait for subtask to complete"
                }
            },
            "required": ["target_role", "task_description"]
        })
    }

    fn trust_requirement(&self) -> &str {
        "trusted"
    }

    fn risk_level(&self) -> &str {
        "medium"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: SpawnSubtaskInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let role = Self::parse_role(&input.target_role)?;
        info!(role = ?role, "Spawning subtask");

        let swarm = self.swarm.read().await;

        let result = swarm
            .spawn_subtask(role, &input.task_description, input.wait_for_completion)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let output = SpawnSubtaskOutput {
            role: format!("{:?}", result.role),
            summary: result.summary,
            modified_files: result.modified_files,
            created_files: result.created_files,
            tokens_used: result.tokens_used,
            success: result.success,
            error: result.error,
        };

        serde_json::to_value(output).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}
