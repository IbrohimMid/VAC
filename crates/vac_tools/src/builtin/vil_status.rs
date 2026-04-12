//! Vil Status — report VAC subsystem status.

use async_trait::async_trait;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool, ToolRegistry};

#[derive(Debug, Serialize)]
pub struct VilStatusOutput {
    pub project_root: String,
    pub session_active: bool,
    pub trace_enabled: bool,
    pub builtin_tools: usize,
    pub mcp_servers: Vec<String>,
    pub skills_available: usize,
    pub knowledge_patterns: usize,
    pub knowledge_best_practices: usize,
    pub knowledge_blueprints: usize,
}

pub struct VilStatusTool {
    registry: Arc<ToolRegistry>,
}

impl VilStatusTool {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }

    fn get_knowledge_stats() -> (usize, usize, usize) {
        let kb = vil_knowledge::KnowledgeBase::bootstrap();
        (kb.patterns.len(), kb.best_practices.len(), kb.blueprints.len())
    }
}

#[async_trait]
impl VilTool for VilStatusTool {
    fn name(&self) -> &str {
        "vil_status"
    }

    fn description(&self) -> &str {
        "Report status of VAC subsystems including project, tools, knowledge base, and configuration."
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
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let tools = self.registry.list().await;
        let builtin_count = tools.len();
        
        let skills_dir = PathBuf::from(".vac/skills");
        let skills_count = if skills_dir.exists() {
            std::fs::read_dir(&skills_dir)
                .map(|d| d.flatten().filter(|e| e.path().extension().map(|ext| ext == "toml").unwrap_or(false)).count())
                .unwrap_or(0)
        } else {
            0
        };

        let (patterns, best_practices, blueprints) = Self::get_knowledge_stats();

        let output = VilStatusOutput {
            project_root: context.working_dir.to_string_lossy().to_string(),
            session_active: true,
            trace_enabled: true,
            builtin_tools: builtin_count,
            mcp_servers: vec![],
            skills_available: skills_count,
            knowledge_patterns: patterns,
            knowledge_best_practices: best_practices,
            knowledge_blueprints: blueprints,
        };

        serde_json::to_value(output).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}
