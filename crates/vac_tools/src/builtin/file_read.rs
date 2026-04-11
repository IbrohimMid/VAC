use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

#[derive(Debug, Deserialize)]
pub struct FileReadInput {
    pub path: String,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct FileReadOutput {
    pub content: String,
    pub path: String,
    pub line_count: usize,
    pub truncated: bool,
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
        "Safe"
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

        let mut content = tokio::fs::read_to_string(&path)
            .await
            .map_err(ToolError::IoError)?;

        let total_lines = content.lines().count();
        let mut truncated = false;

        if let (Some(start), Some(end)) = (input.start_line, input.end_line) {
            let lines: Vec<&str> = content.lines().collect();
            let start_idx = start.saturating_sub(1).min(lines.len());
            let end_idx = end.min(lines.len());
            content = lines[start_idx..end_idx].join("\n");
            truncated = start > 1 || end < total_lines;
        }

        if let Some(max_bytes) = input.max_bytes {
            if content.len() > max_bytes {
                content.truncate(max_bytes);
                truncated = true;
            }
        }

        let line_count = content.lines().count();
        info!("Read {} lines from {:?}", line_count, path);

        Ok(serde_json::to_value(FileReadOutput {
            content,
            path: path.to_string_lossy().to_string(),
            line_count,
            truncated,
        })?)
    }
}
