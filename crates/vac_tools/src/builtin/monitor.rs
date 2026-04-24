//! NS.1 — `monitor` tool. Spawns a subprocess, filters stdout by
//! a regex, and returns the matched lines once the child exits
//! (or `max_lines` is reached). One-shot: the LLM gets a bounded
//! list, not a live stream. Wraps
//! `vac_session_primitives::monitor::spawn_monitor`.

use async_trait::async_trait;
use futures::StreamExt;
use serde::Deserialize;

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};
use vac_session_primitives::monitor::{MonitorSpec, spawn_monitor};

#[derive(Debug, Deserialize)]
struct Input {
    argv: Vec<String>,
    match_regex: String,
    #[serde(default = "default_max_lines")]
    max_lines: u64,
    /// Hard wall-clock cap in seconds. Protects against a child
    /// that produces matching lines forever.
    #[serde(default = "default_timeout_secs")]
    timeout_secs: u64,
}

fn default_max_lines() -> u64 {
    200
}

fn default_timeout_secs() -> u64 {
    60
}

pub struct MonitorTool;

impl MonitorTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MonitorTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for MonitorTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &str {
        "monitor"
    }

    fn description(&self) -> &str {
        "Run a command and collect stdout lines matching a regex, up to max_lines or timeout_secs. Use for extracting signal from noisy tools (e.g. tail a log for errors, watch a build for warnings)."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "argv": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 1,
                    "description": "Program + args (no shell expansion)."
                },
                "match_regex": {
                    "type": "string",
                    "description": "Regex; a named group `severity` becomes the label."
                },
                "max_lines": {
                    "type": "integer",
                    "description": "Stop after this many matches (default 200)."
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Wall-clock cap (default 60)."
                }
            },
            "required": ["argv", "match_regex"]
        })
    }

    fn trust_requirement(&self) -> &str {
        // Spawns arbitrary argv — same blast radius as BashTool.
        // Pre-NS-arc classification was too lenient.
        "privileged"
    }

    fn risk_level(&self) -> &str {
        "destructive"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: Input = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid arguments: {e}")))?;
        if input.argv.is_empty() {
            return Err(ToolError::ExecutionFailed("argv must be non-empty".into()));
        }
        let spec = MonitorSpec {
            argv: input.argv,
            match_regex: input.match_regex,
            max_lines: input.max_lines,
        };
        let mut handle = spawn_monitor(spec)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("spawn_monitor: {e}")))?;

        let collect = async {
            let mut rows = Vec::<serde_json::Value>::new();
            while let Some(line) = handle.lines.next().await {
                rows.push(serde_json::json!({
                    "seq": line.seq,
                    "severity": line.severity,
                    "line": line.line,
                }));
            }
            rows
        };

        let timeout = std::time::Duration::from_secs(input.timeout_secs.max(1));
        let (rows, timed_out) = match tokio::time::timeout(timeout, collect).await {
            Ok(rows) => (rows, false),
            Err(_) => {
                let _ = handle.child.start_kill();
                (Vec::new(), true)
            }
        };
        let _ = handle.child.wait().await;

        Ok(serde_json::json!({
            "lines": rows,
            "timed_out": timed_out,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::test_util::make_ctx;

    #[cfg(unix)]
    #[tokio::test]
    async fn monitor_collects_matching_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        let out = MonitorTool::new()
            .execute(
                serde_json::json!({
                    "argv": ["sh", "-c", "printf 'foo\\nERR boom\\nbar\\nERR oops\\n'"],
                    "match_regex": "^ERR",
                    "max_lines": 10,
                    "timeout_secs": 5
                }),
                &ctx,
            )
            .await
            .unwrap();
        let lines = out["lines"].as_array().unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(out["timed_out"], false);
    }

    #[tokio::test]
    async fn monitor_rejects_empty_argv() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        let err = MonitorTool::new()
            .execute(
                serde_json::json!({
                    "argv": [],
                    "match_regex": "x",
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("argv"));
    }
}
