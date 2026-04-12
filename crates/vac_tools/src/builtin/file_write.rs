use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

#[derive(Debug, Deserialize)]
pub struct FileWriteInput {
    pub path: String,
    pub content: String,
    pub create_dirs: Option<bool>,
    pub append: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct FileWriteOutput {
    pub path: String,
    pub bytes_written: usize,
    pub created: bool,
}

pub struct FileWriteTool;

#[allow(clippy::new_without_default)]
impl FileWriteTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl VilTool for FileWriteTool {
    fn name(&self) -> &str {
        "file_write"
    }

    fn description(&self) -> &str {
        "Write content to a file, creating it if necessary"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to file to write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to file"
                },
                "create_dirs": {
                    "type": "boolean",
                    "description": "Create parent directories if they don't exist",
                    "default": true
                },
                "append": {
                    "type": "boolean",
                    "description": "Append to existing file instead of overwriting",
                    "default": false
                }
            },
            "required": ["path", "content"]
        })
    }

    fn trust_requirement(&self) -> &str {
        "Untrusted"
    }

    fn risk_level(&self) -> &str {
        "needs_approval"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: FileWriteInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let path = if PathBuf::from(&input.path).is_absolute() {
            PathBuf::from(&input.path)
        } else {
            context.working_dir.join(&input.path)
        };

        debug!("Writing file: {:?}", path);

        if let Some(true) = input.create_dirs {
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(ToolError::IoError)?;
                    info!("Created directory: {:?}", parent);
                }
            }
        }

        let created = !path.exists();

        // Snapshot before overwrite (not for append or new files)
        if !created && !input.append.unwrap_or(false) {
            crate::journal::snapshot_before_write(
                &context.working_dir,
                context.session_id,
                &input.path,
            );
        }

        let bytes_written = if input.append.unwrap_or(false) {
            use tokio::io::AsyncWriteExt;
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await
                .map_err(ToolError::IoError)?;
            file.write_all(input.content.as_bytes())
                .await
                .map_err(ToolError::IoError)?;
            file.sync_all().await.map_err(ToolError::IoError)?;
            input.content.len()
        } else {
            tokio::fs::write(&path, &input.content)
                .await
                .map_err(ToolError::IoError)?;
            input.content.len()
        };

        info!("Wrote {} bytes to {:?}", bytes_written, path);

        Ok(serde_json::to_value(FileWriteOutput {
            path: path.to_string_lossy().to_string(),
            bytes_written,
            created,
        })?)
    }
}
