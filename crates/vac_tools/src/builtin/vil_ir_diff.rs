//! vil_ir_diff — agent-callable tool that runs IR-level diff on a file.
//!
//! Wraps `vil_ir::diff::diff_modules` so the agent can invoke semantic IR
//! comparison as a real tool call instead of a canned prompt.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

#[derive(Debug, Deserialize)]
struct DiffInput {
    /// File path relative to working_dir.
    file: String,
    /// Optional git revision to compare from (default: HEAD).
    #[serde(default = "default_from_rev")]
    from_rev: String,
}

fn default_from_rev() -> String {
    "HEAD".to_string()
}

#[derive(Debug, Serialize)]
struct DiffOutput {
    file: String,
    overall_kind: String,
    modifies_generated_region: bool,
    changes: Vec<ChangeEntry>,
    total_semantic: usize,
    total_cosmetic: usize,
}

#[derive(Debug, Serialize)]
struct ChangeEntry {
    entity_name: String,
    entity_type: String,
    kind: String,
    description: String,
}

/// RAII guard that removes a temp file on drop.
struct TempFile(std::path::PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Validate that a file path is within the project root (no traversal).
fn validate_path_within_root(
    working_dir: &std::path::Path,
    file: &str,
) -> Result<std::path::PathBuf, ToolError> {
    let abs_path = working_dir.join(file);
    let canonical = abs_path.canonicalize().map_err(|_| {
        ToolError::ExecutionFailed(format!("File not found: {}", file))
    })?;
    if !canonical.starts_with(working_dir) {
        return Err(ToolError::ExecutionFailed(
            "Path traversal denied: file must be within project root".into(),
        ));
    }
    Ok(canonical)
}

/// Validate that a git revision string contains only safe characters.
fn validate_rev(rev: &str) -> Result<(), ToolError> {
    if rev.is_empty() || rev.len() > 128 {
        return Err(ToolError::InvalidArguments(
            "Invalid revision: empty or too long".into(),
        ));
    }
    if !rev
        .chars()
        .all(|c| c.is_alphanumeric() || "~^./-@{}:_".contains(c))
    {
        return Err(ToolError::InvalidArguments(
            "Invalid revision format: contains disallowed characters".into(),
        ));
    }
    Ok(())
}

pub struct VilIrDiffTool;

impl VilIrDiffTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for VilIrDiffTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for VilIrDiffTool {
    fn name(&self) -> &str {
        "vil_ir_diff"
    }

    fn description(&self) -> &str {
        "Compare the IR (functions, structs, VIL attributes) of a file between a git \
        revision and the current worktree. Reports semantic vs cosmetic changes and \
        flags modifications to VIL-generated regions."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file": {
                    "type": "string",
                    "description": "File path relative to project root"
                },
                "from_rev": {
                    "type": "string",
                    "description": "Git revision to compare from (default: HEAD)"
                }
            },
            "required": ["file"]
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
        let input: DiffInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        validate_rev(&input.from_rev)?;
        let abs_path = validate_path_within_root(&context.working_dir, &input.file)?;

        // Compute the safe relative path from canonical working_dir
        let safe_rel = abs_path
            .strip_prefix(&context.working_dir)
            .unwrap_or(&abs_path)
            .display()
            .to_string();

        // Parse current worktree version
        let new_module = vil_ir::parser::parse_file(&abs_path)
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to parse current file: {e}")))?;

        // Get old version from git
        let old_module = {
            let output = std::process::Command::new("git")
                .args(["show", &format!("{}:{}", input.from_rev, safe_rel)])
                .current_dir(&context.working_dir)
                .output()
                .map_err(|e| ToolError::ExecutionFailed(format!("git show failed: {e}")))?;

            if output.status.success() {
                let content = String::from_utf8_lossy(&output.stdout);
                let tmp = TempFile(
                    std::env::temp_dir()
                        .join(format!("vac_diff_{}.rs", uuid::Uuid::new_v4())),
                );
                std::fs::write(&tmp.0, content.as_bytes())
                    .map_err(|e| ToolError::ExecutionFailed(format!("tmp write: {e}")))?;
                vil_ir::parser::parse_file(&tmp.0).ok()
                // tmp auto-cleaned on drop
            } else {
                None // File is new (not in git yet)
            }
        };

        let diff = vil_ir::diff::diff_modules(old_module.as_ref(), Some(&new_module));

        match diff {
            Some(d) => {
                let total_semantic = d
                    .changes
                    .iter()
                    .filter(|c| c.kind == vil_ir::diff::ChangeKind::Semantic)
                    .count();
                let total_cosmetic = d
                    .changes
                    .iter()
                    .filter(|c| c.kind == vil_ir::diff::ChangeKind::Cosmetic)
                    .count();

                let changes = d
                    .changes
                    .iter()
                    .map(|c| ChangeEntry {
                        entity_name: c.entity_name.clone(),
                        entity_type: c.entity_type.clone(),
                        kind: match c.kind {
                            vil_ir::diff::ChangeKind::Semantic => "Semantic".to_string(),
                            vil_ir::diff::ChangeKind::Cosmetic => "Cosmetic".to_string(),
                        },
                        description: c.description.clone(),
                    })
                    .collect();

                let output = DiffOutput {
                    file: input.file,
                    overall_kind: match d.overall_kind {
                        vil_ir::diff::ChangeKind::Semantic => "Semantic".to_string(),
                        vil_ir::diff::ChangeKind::Cosmetic => "Cosmetic".to_string(),
                    },
                    modifies_generated_region: d.modifies_generated_region,
                    changes,
                    total_semantic,
                    total_cosmetic,
                };

                serde_json::to_value(output)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
            }
            None => Ok(serde_json::json!({
                "file": input.file,
                "overall_kind": "None",
                "modifies_generated_region": false,
                "changes": [],
                "total_semantic": 0,
                "total_cosmetic": 0,
                "note": "No IR differences detected"
            })),
        }
    }
}
