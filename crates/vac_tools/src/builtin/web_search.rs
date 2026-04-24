//! NS.1 — `web_search` tool. Wraps
//! `vac_session_primitives::web::BraveBackend`. API key comes from
//! env var `VAC_BRAVE_API_KEY`. Missing key returns a clear error
//! rather than silently failing.

use async_trait::async_trait;
use serde::Deserialize;

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};
use vac_session_primitives::web::{BraveBackend, SearchBackend, WebSearchRequest};

#[derive(Debug, Deserialize)]
struct Input {
    query: String,
    #[serde(default = "default_count")]
    count: u8,
}

fn default_count() -> u8 {
    10
}

pub struct WebSearchTool;

impl WebSearchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for WebSearchTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web via Brave Search. Returns title/url/snippet rows. Requires VAC_BRAVE_API_KEY in env."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query." },
                "count": { "type": "integer", "minimum": 1, "maximum": 50, "description": "Max results (default 10)." }
            },
            "required": ["query"]
        })
    }

    fn trust_requirement(&self) -> &str {
        "ask_once"
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
        let api_key = std::env::var("VAC_BRAVE_API_KEY").map_err(|_| {
            ToolError::ExecutionFailed(
                "VAC_BRAVE_API_KEY is not set; web_search requires a Brave API key".into(),
            )
        })?;
        let backend = BraveBackend::new(api_key);
        let req = WebSearchRequest {
            query: input.query,
            backend: "brave".into(),
            count: input.count,
        };
        let hits = backend
            .search(&req)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("brave search: {e}")))?;
        Ok(serde_json::json!({ "hits": hits }))
    }
}
