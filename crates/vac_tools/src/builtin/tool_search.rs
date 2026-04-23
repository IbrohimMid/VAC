//! F1.6 — `tool_search` — agent searches its own tool registry.
//!
//! Claude Code lesson: once the tool count passes ~20, the agent
//! benefits from being able to search its own inventory rather than
//! carrying a 40+ tool manifest in every request. `tool_search`
//! returns matching tools with their descriptions; the agent then
//! requests only the relevant ones.
//!
//! Ranking: substring match on name first, then description. Cheap,
//! deterministic, good enough for discovery.

use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;

use crate::error::ToolError;
use crate::registry::{ToolContext, ToolRegistry, VilTool};

#[derive(Debug, Deserialize)]
struct Input {
    /// Free-form search query.
    query: String,
    /// Max results to return (default 10).
    #[serde(default)]
    limit: Option<usize>,
}

pub struct ToolSearchTool {
    registry: Arc<ToolRegistry>,
}

impl ToolSearchTool {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }
}

/// Score how well a tool matches the query. Higher is better.
/// - exact name match → 100
/// - name contains query → 50 + (match_length / name_length * 30)
/// - description contains query → 20 + (match_length / description_length * 10)
/// - no match → 0
fn score(name: &str, description: &str, query: &str) -> u32 {
    let q = query.to_lowercase();
    let n = name.to_lowercase();
    let d = description.to_lowercase();

    if n == q {
        return 100;
    }
    if n.contains(&q) && !n.is_empty() {
        let ratio = (q.len() as f32 / n.len() as f32).min(1.0);
        return 50 + (ratio * 30.0) as u32;
    }
    if d.contains(&q) && !d.is_empty() {
        let ratio = (q.len() as f32 / d.len().max(1) as f32).min(1.0);
        return 20 + (ratio * 10.0) as u32;
    }
    0
}

#[async_trait]
impl VilTool for ToolSearchTool {
    fn name(&self) -> &str {
        "tool_search"
    }

    fn description(&self) -> &str {
        "Search the agent's own tool registry by query. Returns matching tools ranked by relevance. Use this before committing to a tool call when the right tool is non-obvious."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search term — tool name or capability." },
                "limit": { "type": "integer", "description": "Max results (default 10, max 50).", "minimum": 1, "maximum": 50 }
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
        _context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: Input = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid arguments: {e}")))?;
        let limit = input.limit.unwrap_or(10).min(50);

        let defs = self.registry.list().await;
        let mut scored: Vec<_> = defs
            .into_iter()
            .map(|d| {
                let s = score(&d.name, &d.description, &input.query);
                (s, d)
            })
            .filter(|(s, _)| *s > 0)
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.truncate(limit);

        let results: Vec<_> = scored
            .into_iter()
            .map(|(score, d)| {
                serde_json::json!({
                    "name": d.name,
                    "description": d.description,
                    "category": d.category,
                    "trust_requirement": d.trust_requirement,
                    "risk_level": d.risk_level,
                    "score": score,
                })
            })
            .collect();

        Ok(serde_json::json!({
            "query": input.query,
            "results": results,
            "total": results.len(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_exact_name_match_is_100() {
        assert_eq!(score("signal_tail", "tail a stream", "signal_tail"), 100);
    }

    #[test]
    fn score_name_substring_beats_description() {
        let by_name = score("plan_mode", "enter reviewing state", "plan");
        let by_desc = score("xyz", "call this to plan things", "plan");
        assert!(by_name > by_desc);
    }

    #[test]
    fn score_no_match_is_zero() {
        assert_eq!(score("foo", "bar baz", "unrelated"), 0);
    }

    #[test]
    fn score_case_insensitive() {
        assert!(score("SignalTail", "ignored", "signaltail") >= 50);
    }

    #[tokio::test]
    async fn tool_search_ranks_signal_tools_above_noise() {
        use crate::builtin::signal_list::SignalListTool;
        use crate::builtin::signal_tail::SignalTailTool;
        use crate::builtin::test_util::make_ctx;

        let registry = Arc::new(ToolRegistry::new());
        registry.register(SignalTailTool::new()).await.unwrap();
        registry.register(SignalListTool::new()).await.unwrap();
        // Add a distractor — plan_mode doesn't match "signal".
        registry
            .register(crate::builtin::plan_mode::EnterPlanModeTool::new())
            .await
            .unwrap();

        let tool = ToolSearchTool::new(registry.clone());
        let ctx = make_ctx(std::env::temp_dir(), uuid::Uuid::new_v4());
        let out = tool
            .execute(serde_json::json!({"query": "signal"}), &ctx)
            .await
            .unwrap();

        let results = out["results"].as_array().unwrap();
        assert!(results.len() >= 2);
        // Top result must have "signal" in the name.
        let top = &results[0]["name"].as_str().unwrap();
        assert!(top.contains("signal"), "expected signal tool on top, got {top}");
    }
}
