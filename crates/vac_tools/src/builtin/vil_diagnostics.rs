//! vil_diagnostics — read-only tool for latest vil-lsp findings.
//!
//! Reads from .vac/cache/vil_lsp_diagnostics.json (written by VilLspService).
//! Use before editing VIL code to understand active semantic violations.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

#[derive(Debug, Deserialize)]
struct DiagInput {
    /// Optional file path filter
    path: Option<String>,
    /// Optional severity filter: "error", "warning", "all"
    #[serde(default = "default_severity")]
    severity: String,
    #[serde(default = "default_max")]
    max_results: usize,
}

fn default_severity() -> String {
    "all".to_string()
}
fn default_max() -> usize {
    20
}

#[derive(Debug, Serialize)]
struct DiagOutput {
    total_errors: usize,
    total_warnings: usize,
    diagnostics: Vec<DiagItem>,
    source: String,
}

#[derive(Debug, Serialize)]
struct DiagItem {
    file: String,
    severity: String,
    message: String,
    line: u32,
    code: Option<String>,
}

pub struct VilDiagnosticsTool;

impl VilDiagnosticsTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for VilDiagnosticsTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for VilDiagnosticsTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &str {
        "vil_diagnostics"
    }

    fn description(&self) -> &str {
        "Read latest vil-lsp diagnostics collected by VAC. Use this before editing VIL code \
        to understand active semantic macro misuse, lane violations, and type errors."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Filter by file path (optional)" },
                "severity": {
                    "type": "string",
                    "enum": ["error", "warning", "all"],
                    "description": "Filter by severity (default: all)"
                },
                "max_results": { "type": "integer", "description": "Max results (default: 20)" }
            }
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
        let input: DiagInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let cache_path = context
            .working_dir
            .join(".vac/cache/vil_lsp_diagnostics.json");

        if !cache_path.exists() {
            return Ok(serde_json::to_value(DiagOutput {
                total_errors: 0,
                total_warnings: 0,
                diagnostics: vec![],
                source: "no diagnostics cache found — vil-lsp may not be running".to_string(),
            })?);
        }

        let content = std::fs::read_to_string(&cache_path)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let snapshot: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| ToolError::ExecutionFailed(format!("Cache parse error: {e}")))?;

        let total_errors = snapshot["total_errors"].as_u64().unwrap_or(0) as usize;
        let total_warnings = snapshot["total_warnings"].as_u64().unwrap_or(0) as usize;

        let mut items: Vec<DiagItem> = snapshot["diagnostics"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|d| {
                let file = d["file_path"].as_str()?.to_string();
                let severity = format!("{:?}", d["severity"])
                    .to_lowercase()
                    .replace('"', "");
                let message = d["message"].as_str()?.to_string();
                let line = d["range"]["start_line"].as_u64().unwrap_or(0) as u32;
                let code = d["code"].as_str().map(String::from);

                // Path filter
                if let Some(ref path_filter) = input.path {
                    if !file.contains(path_filter.as_str()) {
                        return None;
                    }
                }

                // Severity filter
                match input.severity.as_str() {
                    "error" if severity != "error" => return None,
                    "warning" if severity != "warning" => return None,
                    _ => {}
                }

                Some(DiagItem {
                    file,
                    severity,
                    message,
                    line,
                    code,
                })
            })
            .collect();

        items.truncate(input.max_results);

        Ok(serde_json::to_value(DiagOutput {
            total_errors,
            total_warnings,
            diagnostics: items,
            source: cache_path.display().to_string(),
        })?)
    }
}
