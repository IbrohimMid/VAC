use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEditInput {
    pub file_path: String,
    pub old_string: String,
    pub new_string: String,
    #[serde(default)]
    pub replace_all: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEditOutput {
    pub file_path: String,
    pub occurrences_replaced: usize,
    pub content_updated: String,
}

pub struct FileEditTool;

#[async_trait]
impl crate::registry::VilTool for FileEditTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &'static str {
        "file_edit"
    }

    fn description(&self) -> &'static str {
        "Edit a file by replacing exact string matches"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {"type": "string"},
                "old_string": {"type": "string"},
                "new_string": {"type": "string"},
                "replace_all": {"type": "boolean"}
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    fn trust_requirement(&self) -> &'static str {
        "file_system"
    }

    fn risk_level(&self) -> &'static str {
        "needs_approval"
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        context: &crate::registry::ToolContext,
    ) -> Result<serde_json::Value, crate::error::ToolError> {
        let input: FileEditInput = serde_json::from_value(input)?;
        if input.old_string.is_empty() {
            return Err(crate::error::ToolError::InvalidArguments(
                "old_string must not be empty".to_string(),
            ));
        }

        let path = if PathBuf::from(&input.file_path).is_absolute() {
            PathBuf::from(&input.file_path)
        } else {
            context.working_dir.join(&input.file_path)
        };

        // Snapshot before edit — both paths: session-journal
        // (path-keyed, session-scoped) and content-addressable
        // backup (R2.a) that survives the session and drives
        // `vac restore <file>`.
        crate::journal::snapshot_before_write(
            &context.working_dir,
            context.session_id,
            &input.file_path,
        );
        if let Err(e) = crate::backup::snapshot_file(&context.working_dir, &path, context.submit_id).await {
            tracing::warn!(
                target: "vac_tools::backup",
                path = %path.display(),
                error = %e,
                "backup snapshot failed; edit proceeds without reversal record",
            );
        }

        let content = tokio::fs::read_to_string(&path).await?;

        let occurrences_replaced = if input.replace_all {
            content.matches(&input.old_string).count()
        } else {
            usize::from(content.contains(&input.old_string))
        };
        let content_updated = if input.replace_all {
            content.replace(&input.old_string, &input.new_string)
        } else {
            content.replacen(&input.old_string, &input.new_string, 1)
        };
        if occurrences_replaced > 0 {
            tokio::fs::write(&path, &content_updated).await?;
        }

        debug!(
            file = %input.file_path,
            occurrences = occurrences_replaced,
            "File edit completed"
        );

        let output = FileEditOutput {
            file_path: input.file_path,
            occurrences_replaced,
            content_updated,
        };

        Ok(serde_json::to_value(output)?)
    }
}
