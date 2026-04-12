//! Vil Status — report VAC subsystem status from actual runtime state.

use async_trait::async_trait;
use serde::Serialize;
use std::sync::Arc;

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool, ToolRegistry};

#[derive(Debug, Serialize)]
pub struct VilStatusOutput {
    pub project_root: String,
    pub session_id: String,
    /// Derived from whether session_id is non-nil (always true if tool is executing)
    pub session_active: bool,
    /// Derived from .vac/config.toml trace.enable field
    pub trace_enabled: Option<bool>,
    /// Derived from .vac/config.toml trace.enable_signing field
    pub trace_signing_enabled: Option<bool>,
    pub builtin_tools: usize,
    /// Derived from .vac/config.toml mcp_servers list
    pub mcp_servers_configured: Vec<String>,
    /// Builtin skills (hardcoded in SkillLoader) + custom .vac/skills/*.toml
    pub skills_available: SkillStats,
    pub knowledge: KnowledgeStats,
    /// Whether knowledge is loaded from authoritative corpus or fallback bootstrap
    pub knowledge_authoritative: bool,
    pub corpus_root: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SkillStats {
    pub builtin: usize,
    pub custom: usize,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct KnowledgeStats {
    pub patterns: usize,
    pub best_practices: usize,
    pub blueprints: usize,
}

pub struct VilStatusTool {
    registry: Arc<ToolRegistry>,
}

impl VilStatusTool {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl VilTool for VilStatusTool {
    fn name(&self) -> &str {
        "vil_status"
    }

    fn description(&self) -> &str {
        "Report runtime status of VAC subsystems: session, tools, knowledge base, MCP servers, skills, and config."
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

        // Derive trace config from .vac/config.toml
        let (trace_enabled, trace_signing, mcp_servers_configured) =
            read_config_state(&context.working_dir);

        // Skills: builtin (hardcoded count) + custom TOML files
        let custom_skills = count_custom_skills(&context.working_dir);
        // SkillRunnerTool itself is 1 tool, but it loads N builtin recipes internally.
        // We report the known builtin recipe count (4: new_vilapp, new_pipeline, new_plugin, new_handler)
        let builtin_skill_recipes = 4;

        // Knowledge: load from corpus if available (same logic as KnowledgeTool)
        let kb = vil_knowledge::KnowledgeBase::load(&context.working_dir);

        let output = VilStatusOutput {
            project_root: context.working_dir.to_string_lossy().to_string(),
            session_id: context.session_id.to_string(),
            session_active: true, // tool is executing, so session is active by definition
            trace_enabled,
            trace_signing_enabled: trace_signing,
            builtin_tools: builtin_count,
            mcp_servers_configured,
            skills_available: SkillStats {
                builtin: builtin_skill_recipes,
                custom: custom_skills,
                total: builtin_skill_recipes + custom_skills,
            },
            knowledge: KnowledgeStats {
                patterns: kb.patterns.len(),
                best_practices: kb.best_practices.len(),
                blueprints: kb.blueprints.len(),
            },
            knowledge_authoritative: kb.is_authoritative,
            corpus_root: kb.corpus_root.map(|p| p.to_string_lossy().to_string()),
        };

        serde_json::to_value(output).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}

/// Read trace.enable, trace.enable_signing, and mcp_servers from .vac/config.toml.
/// Returns (trace_enabled, trace_signing, mcp_server_names).
fn read_config_state(project_root: &std::path::Path) -> (Option<bool>, Option<bool>, Vec<String>) {
    let config_path = project_root.join(".vac/config.toml");
    let Ok(content) = std::fs::read_to_string(&config_path) else {
        return (None, None, vec![]);
    };
    let Ok(table) = content.parse::<toml::Table>() else {
        return (None, None, vec![]);
    };

    let trace_enabled = table
        .get("trace")
        .and_then(|t| t.get("enable"))
        .and_then(|v| v.as_bool());

    let trace_signing = table
        .get("trace")
        .and_then(|t| t.get("enable_signing"))
        .and_then(|v| v.as_bool());

    let mcp_servers = table
        .get("mcp_servers")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    (trace_enabled, trace_signing, mcp_servers)
}

fn count_custom_skills(project_root: &std::path::Path) -> usize {
    let skills_dir = project_root.join(".vac/skills");
    if !skills_dir.exists() {
        return 0;
    }
    std::fs::read_dir(&skills_dir)
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().extension().map(|ext| ext == "toml").unwrap_or(false))
                .count()
        })
        .unwrap_or(0)
}
