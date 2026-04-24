//! NS.1 — `web_fetch` tool. Wraps
//! `vac_session_primitives::web::fetch`: issues an HTTP request
//! through the header-allowlisted transport and honours the 2 MB
//! response cap, spilling oversize bodies to
//! `.vac/tool-results/web-fetch-*.bin`.

use async_trait::async_trait;
use serde::Deserialize;

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};
use vac_session_primitives::web::{
    DEFAULT_RESPONSE_CAP, WebFetchRequest, fetch,
};

#[derive(Debug, Deserialize)]
struct Input {
    url: String,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    body: Option<serde_json::Value>,
    #[serde(default)]
    headers: std::collections::HashMap<String, String>,
}

pub struct WebFetchTool;

impl WebFetchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for WebFetchTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Issue an HTTP request (GET/POST/etc.) and return the response body. Headers outside a small allowlist are dropped. Bodies above 2 MB spill to disk and the result carries a pointer."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "Target URL." },
                "method": { "type": "string", "description": "HTTP method (default GET)." },
                "body": { "description": "Optional JSON body for POST/PUT." },
                "headers": { "type": "object", "description": "Additional headers (allowlist: Accept, Accept-Language, Authorization, Content-Type, User-Agent)." }
            },
            "required": ["url"]
        })
    }

    fn trust_requirement(&self) -> &str {
        "ask_once"
    }

    fn risk_level(&self) -> &str {
        // Arbitrary URL egress with forwarded Authorization headers
        // is network-exfil-shaped. Classify as destructive so gates
        // reading is_input_destructive treat it accordingly.
        "destructive"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: Input = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid arguments: {e}")))?;
        let spill_dir = context.working_dir.join(".vac/tool-results");
        let req = WebFetchRequest {
            url: input.url,
            method: input.method,
            body: input.body,
            headers: input.headers,
        };
        let result = fetch(&req, &spill_dir, DEFAULT_RESPONSE_CAP)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("web fetch: {e}")))?;
        serde_json::to_value(result)
            .map_err(|e| ToolError::ExecutionFailed(format!("serialize: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_parses_required_url() {
        let r: Result<Input, _> = serde_json::from_value(serde_json::json!({"url": "https://example.com"}));
        assert!(r.is_ok());
    }

    #[test]
    fn input_rejects_missing_url() {
        let r: Result<Input, _> = serde_json::from_value(serde_json::json!({}));
        assert!(r.is_err());
    }
}
