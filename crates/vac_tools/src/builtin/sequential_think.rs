//! Sequential Think — structured reasoning tool for complex problem decomposition.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

#[derive(Debug, Deserialize)]
struct ThinkInput {
    thought_summary: String,
    details: Option<String>,
    next_action_prediction: Option<String>,
}

#[derive(Debug, Serialize)]
struct ThinkOutput {
    result: String,
    thought_id: u64,
}

pub struct SequentialThinkTool {
    thought_counter: std::sync::atomic::AtomicU64,
}

impl SequentialThinkTool {
    pub fn new() -> Self {
        Self {
            thought_counter: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl VilTool for SequentialThinkTool {
    fn name(&self) -> &str { "sequential_think" }

    fn description(&self) -> &str {
        "Record a structured reasoning step. Use this before complex actions to decompose problems, plan approaches, and document your thinking. The thought is logged for audit but does not consume significant tokens."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "thought_summary": {
                    "type": "string",
                    "description": "Concise summary of current reasoning (1-2 sentences)"
                },
                "details": {
                    "type": "string",
                    "description": "Optional: detailed reasoning, analysis, or considerations"
                },
                "next_action_prediction": {
                    "type": "string",
                    "description": "Optional: what you plan to do next based on this reasoning"
                }
            },
            "required": ["thought_summary"]
        })
    }

    fn trust_requirement(&self) -> &str { "safe" }
    fn risk_level(&self) -> &str { "safe" }

    async fn execute(
        &self,
        args: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: ThinkInput = serde_json::from_value(args)
            .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let thought_id = self.thought_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;

        debug!(
            thought_id = thought_id,
            summary = %input.thought_summary,
            "Agent thinking step"
        );

        let output = ThinkOutput {
            result: "Thought recorded.".to_string(),
            thought_id,
        };

        serde_json::to_value(output).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}