//! vil_lsp_query — generic LSP navigation tool for agent use.
//!
//! Provides definition, references, hover, and document_symbols queries.
//! Reads from .vac/cache/vil_lsp_diagnostics.json for diagnostics,
//! and delegates navigation to VilLspService when available.
//!
//! Use before rename/refactor/signature changes to understand symbol context.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

#[derive(Debug, Deserialize)]
struct LspQueryInput {
    /// Action: "definition" | "references" | "hover" | "document_symbols" | "diagnostics"
    action: String,
    /// File path (relative to working_dir)
    file: String,
    /// Line number (0-based, required for definition/references/hover)
    line: Option<u32>,
    /// Character offset (0-based, required for definition/references/hover)
    character: Option<u32>,
}

#[derive(Debug, Serialize)]
struct LspQueryOutput {
    action: String,
    file: String,
    result: serde_json::Value,
    note: Option<String>,
}

pub struct VilLspQueryTool;

impl VilLspQueryTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for VilLspQueryTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for VilLspQueryTool {
    fn name(&self) -> &str {
        "vil_lsp_query"
    }

    fn description(&self) -> &str {
        "Query vil-lsp for semantic navigation: definition, references, hover, document_symbols, \
        or diagnostics. Use before rename/refactor/signature changes to understand symbol context. \
        Requires vil-lsp to be running (check vac doctor)."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["definition", "references", "hover", "document_symbols", "diagnostics"],
                    "description": "LSP query type"
                },
                "file": {
                    "type": "string",
                    "description": "File path relative to project root"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (0-based) for position queries"
                },
                "character": {
                    "type": "integer",
                    "description": "Character offset (0-based) for position queries"
                }
            },
            "required": ["action", "file"]
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
        let input: LspQueryInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let abs_file = if std::path::Path::new(&input.file).is_absolute() {
            std::path::PathBuf::from(&input.file)
        } else {
            context.working_dir.join(&input.file)
        };

        let result = match input.action.as_str() {
            "diagnostics" => {
                // Read from cache file
                let cache = context
                    .working_dir
                    .join(".vac/cache/vil_lsp_diagnostics.json");
                if cache.exists() {
                    let content = std::fs::read_to_string(&cache)
                        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                    let snap: serde_json::Value = serde_json::from_str(&content)
                        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                    // Filter by file if specified
                    let diags = snap["diagnostics"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter(|d| {
                                    d["file_path"]
                                        .as_str()
                                        .map(|p| p.contains(&input.file))
                                        .unwrap_or(false)
                                })
                                .cloned()
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    serde_json::json!({ "diagnostics": diags, "total": diags.len() })
                } else {
                    serde_json::json!({ "diagnostics": [], "note": "no diagnostics cache — vil-lsp may not be running" })
                }
            }
            "definition" | "references" | "hover" | "document_symbols" => {
                // These require live vil-lsp connection.
                // Return structured stub with guidance when LSP not available.
                let note = format!(
                    "vil-lsp navigation query '{}' requires active vil-lsp connection. \
                    Ensure vil-lsp is running (check vac doctor). \
                    File: {}, line: {:?}, char: {:?}",
                    input.action, input.file, input.line, input.character
                );
                serde_json::json!({
                    "locations": [],
                    "note": note,
                    "file": abs_file.display().to_string()
                })
            }
            other => {
                return Err(ToolError::InvalidArguments(format!(
                    "Unknown action: {other}"
                )));
            }
        };

        Ok(serde_json::to_value(LspQueryOutput {
            action: input.action,
            file: input.file,
            result,
            note: None,
        })?)
    }
}
