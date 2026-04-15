//! VIL Knowledge tool — searches VIL pattern library and best practices.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

#[derive(Debug, Deserialize)]
struct KnowledgeInput {
    /// Search query
    query: String,
    /// Optional: filter by category (server, pipeline, plugin, config, tools, recipe)
    category: Option<String>,
    /// Optional: max results (default 5)
    max_results: Option<usize>,
}

#[derive(Debug, Serialize)]
struct KnowledgeOutput {
    patterns: Vec<PatternResult>,
    best_practices: Vec<BestPracticeResult>,
    total_found: usize,
}

#[derive(Debug, Serialize)]
struct PatternResult {
    name: String,
    category: String,
    description: String,
    code_template: String,
    when_to_use: String,
}

#[derive(Debug, Serialize)]
struct BestPracticeResult {
    rule: String,
    rationale: String,
}

pub struct KnowledgeTool {}

impl KnowledgeTool {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl VilTool for KnowledgeTool {
    fn name(&self) -> &str {
        "vil_knowledge"
    }

    fn description(&self) -> &str {
        "Search VIL pattern library, code templates, and best practices. Use this to find VIL-specific patterns for VilServer handlers, streaming pipelines, plugins, configuration, and more."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query for VIL patterns (e.g., 'create VilServer', 'pipeline streaming', 'rag plugin', 'sidecar python')"
                },
                "category": {
                    "type": "string",
                    "enum": ["server", "pipeline", "plugin", "config", "tools", "recipe"],
                    "description": "Optional: filter by pattern category"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 5)"
                }
            },
            "required": ["query"]
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
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: KnowledgeInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let max_results = input.max_results.unwrap_or(5);

        debug!(query = %input.query, category = ?input.category, "Searching VIL knowledge base");

        let knowledge = vil_knowledge::KnowledgeBase::load(&context.working_dir);

        // Search patterns
        let mut patterns: Vec<&vil_knowledge::Pattern> = if let Some(ref cat) = input.category {
            knowledge.patterns_by_category(cat)
        } else {
            knowledge.search_patterns(&input.query)
        };

        // If category filter + query, further filter by query relevance
        if input.category.is_some() {
            let query_lower = input.query.to_lowercase();
            patterns.retain(|p| {
                let searchable =
                    format!("{} {} {}", p.name, p.description, p.when_to_use).to_lowercase();
                query_lower
                    .split_whitespace()
                    .any(|kw| searchable.contains(kw))
            });
        }

        patterns.truncate(max_results);

        // Search best practices
        let best_practices: Vec<&vil_knowledge::BestPractice> =
            knowledge.search_best_practices(&input.query);

        let total_found = patterns.len() + best_practices.len();

        let output = KnowledgeOutput {
            patterns: patterns
                .iter()
                .map(|p| PatternResult {
                    name: p.name.clone(),
                    category: p.category.clone(),
                    description: p.description.clone(),
                    code_template: p.code_template.clone(),
                    when_to_use: p.when_to_use.clone(),
                })
                .collect(),
            best_practices: best_practices
                .iter()
                .take(3)
                .map(|bp| BestPracticeResult {
                    rule: bp.rule.clone(),
                    rationale: bp.rationale.clone(),
                })
                .collect(),
            total_found,
        };

        serde_json::to_value(output).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}
