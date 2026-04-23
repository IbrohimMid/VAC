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
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    async fn prepare_permission_matcher(
        &self,
        args: &serde_json::Value,
    ) -> Box<dyn Fn(&str) -> bool + Send + Sync> {
        let path = args.get("path").and_then(|v| v.as_str()).map(|s| s.to_string());
        Box::new(move |target| {
            if let Some(p) = &path {
                target.contains(p)
            } else {
                true
            }
        })
    }

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

    /// W2.4 — File write is destructive only when it overwrites an
    /// existing file. Append-mode with a brand-new path is mutating
    /// but recoverable. Creating a new file is neither destructive
    /// nor read-only.
    ///
    /// **Sync I/O note.** The trait method is synchronous (called
    /// from permission gates that are themselves not async), so the
    /// `exists()` check blocks on a single-syscall `stat`. This is
    /// acceptable for permission dispatch — a few microseconds per
    /// check — but callers that batch thousands of classifications
    /// should wrap the invocation in `spawn_blocking`.
    fn is_input_destructive(&self, input: &serde_json::Value) -> bool {
        let Some(path) = input.get("path").and_then(|p| p.as_str()) else {
            return true; // Missing path → assume worst case.
        };
        let append = input
            .get("append")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let pb = std::path::PathBuf::from(path);
        // Overwriting an existing file is destructive. Append extends
        // so the prior content survives — not destructive even when
        // the file exists.
        pb.exists() && !append
    }

    fn is_input_concurrency_safe(&self, input: &serde_json::Value) -> bool {
        // Two writes to different paths are fine; two writes to the
        // same path are not. Without full input-matrix awareness we
        // conservatively report non-safe so the scheduler serialises.
        let _ = input;
        false
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

        // Snapshot before overwrite (not for append). For new files
        // we still snapshot an empty record — restoring an empty
        // snapshot reverses the create.
        if !input.append.unwrap_or(false) {
            // Session-journal snapshot (path-keyed, session-scoped).
            if !created {
                crate::journal::snapshot_before_write(
                    &context.working_dir,
                    context.session_id,
                    &input.path,
                );
            }
            // R2.a — content-addressable backup for `vac restore`.
            // Best-effort: on failure we log + continue so the tool
            // call isn't blocked by a snapshot problem.
            if let Err(e) = crate::backup::snapshot_file(&context.working_dir, &path, context.submit_id).await {
                tracing::warn!(
                    target: "vac_tools::backup",
                    path = %path.display(),
                    error = %e,
                    "backup snapshot failed; write proceeds without reversal record",
                );
            }
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
