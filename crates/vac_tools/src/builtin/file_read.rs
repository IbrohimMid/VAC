use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

#[derive(Debug, Clone, Deserialize)]
pub struct FileReadInput {
    pub path: String,
    #[serde(default)]
    pub start_line: Option<usize>,
    #[serde(default)]
    pub end_line: Option<usize>,
    #[serde(default)]
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileReadOutput {
    pub path: String,
    pub content: String,
    pub num_lines: usize,
    pub start_line: Option<usize>,
    pub total_lines: usize,
    pub is_binary: bool,
}

pub struct FileReadTool;

#[allow(clippy::new_without_default)]
impl FileReadTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl VilTool for FileReadTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        "Read file contents with optional line range and size limits"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to file to read"
                },
                "start_line": {
                    "type": "integer",
                    "description": "Start line number (1-indexed)"
                },
                "end_line": {
                    "type": "integer",
                    "description": "End line number (1-indexed)"
                },
                "max_bytes": {
                    "type": "integer",
                    "description": "Maximum bytes to read"
                }
            },
            "required": ["path"]
        })
    }

    fn trust_requirement(&self) -> &str {
        "Trusted"
    }

    fn risk_level(&self) -> &str {
        "safe"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: FileReadInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let path = if PathBuf::from(&input.path).is_absolute() {
            PathBuf::from(&input.path)
        } else {
            context.working_dir.join(&input.path)
        };

        debug!("Reading file: {:?}", path);

        if !path.exists() {
            return Err(ToolError::NotFound(format!("File not found: {:?}", path)));
        }

        let metadata = std::fs::metadata(&path).map_err(ToolError::IoError)?;

        if metadata.is_dir() {
            return Err(ToolError::InvalidArguments(
                "Path is a directory".to_string(),
            ));
        }

        // Detect binary files
        let mut bytes = tokio::fs::read(&path).await.map_err(ToolError::IoError)?;
        if let Some(max_bytes) = input.max_bytes {
            bytes.truncate(max_bytes);
        }
        let is_binary = bytes.contains(&0);

        let content = if is_binary {
            String::new()
        } else {
            String::from_utf8_lossy(&bytes).to_string()
        };

        let total_lines = content.lines().count();
        let (selected_content, start_line) = if !is_binary {
            let lines: Vec<&str> = content.lines().collect();
            let start = input
                .start_line
                .unwrap_or(1)
                .saturating_sub(1)
                .min(lines.len());
            let end = input
                .end_line
                .unwrap_or(lines.len())
                .max(start)
                .min(lines.len());
            let selected = lines[start..end].join("\n");
            let actual_start_line = if selected.is_empty() && lines.is_empty() {
                None
            } else {
                Some(start + 1)
            };
            (selected, actual_start_line)
        } else {
            (String::new(), None)
        };

        let num_lines = selected_content.lines().count();
        info!("Read {} lines from {:?}", num_lines, path);

        Ok(serde_json::to_value(FileReadOutput {
            path: path.to_string_lossy().to_string(),
            content: selected_content,
            num_lines,
            start_line,
            total_lines,
            is_binary,
        })?)
    }
}
