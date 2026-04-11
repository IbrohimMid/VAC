use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

#[derive(Debug, Deserialize)]
pub struct TaskDoneInput {
    pub message: Option<String>,
    pub success: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct TaskDoneOutput {
    pub completed: bool,
    pub message: String,
}

pub struct TaskDoneTool;

#[allow(clippy::new_without_default)]
impl TaskDoneTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl VilTool for TaskDoneTool {
    fn name(&self) -> &str {
        "task_done"
    }

    fn description(&self) -> &str {
        "Signal task completion with optional message"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Completion message"
                },
                "success": {
                    "type": "boolean",
                    "description": "Whether the task was successful",
                    "default": true
                }
            }
        })
    }

    fn trust_requirement(&self) -> &str {
        "Trusted"
    }

    fn risk_level(&self) -> &str {
        "Safe"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: TaskDoneInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let success = input.success.unwrap_or(true);
        let message = input.message.unwrap_or_else(|| {
            if success {
                "Task completed successfully".to_string()
            } else {
                "Task completed with errors".to_string()
            }
        });

        info!("Task done: {} (success={})", message, success);

        Ok(serde_json::to_value(TaskDoneOutput {
            completed: true,
            message,
        })?)
    }
}
