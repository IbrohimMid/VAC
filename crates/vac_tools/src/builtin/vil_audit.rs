//! vil_audit — agent-callable tool that runs VIL validation passes on files.
//!
//! Wraps `vil_validate::validate_changes` so the agent can invoke the 7-pass
//! validator as a real tool call.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

/// Validate that a file path is within the project root (no traversal).
fn validate_path_within_root(
    working_dir: &std::path::Path,
    file: &str,
) -> Result<(), ToolError> {
    let abs_path = working_dir.join(file);
    let canonical = abs_path.canonicalize().map_err(|_| {
        ToolError::ExecutionFailed(format!("File not found: {}", file))
    })?;
    if !canonical.starts_with(working_dir) {
        return Err(ToolError::ExecutionFailed(
            "Path traversal denied: file must be within project root".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct AuditInput {
    /// List of file paths relative to project root.
    /// If empty/missing, audits all modules found by the IR pipeline.
    #[serde(default)]
    files: Vec<String>,
    /// Optional: only report issues from a specific pass.
    /// Values: "zero_copy", "observability", "vil_way", "tri_lane",
    ///         "plumbing", "semantic", "macro_coverage", "all"
    #[serde(default = "default_pass_filter")]
    pass_filter: String,
}

fn default_pass_filter() -> String {
    "all".to_string()
}

#[derive(Debug, Serialize)]
struct AuditOutput {
    overall_score: f64,
    files_audited: usize,
    issues: Vec<IssueEntry>,
    total_issues: usize,
}

#[derive(Debug, Serialize)]
struct IssueEntry {
    message: String,
}

pub struct VilAuditTool;

impl VilAuditTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for VilAuditTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for VilAuditTool {
    fn name(&self) -> &str {
        "vil_audit"
    }

    fn description(&self) -> &str {
        "Run VIL semantic validation passes (zero-copy legality, observability, \
        VIL Way compliance, tri-lane consistency, plumbing, macro coverage) on \
        specified files. Returns a validation score and list of issues."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "File paths relative to project root. If empty, audits all modules."
                },
                "pass_filter": {
                    "type": "string",
                    "enum": ["zero_copy", "observability", "vil_way", "tri_lane", "plumbing", "semantic", "macro_coverage", "all"],
                    "description": "Only report issues from a specific validation pass (default: all)"
                }
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
        let input: AuditInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        // Validate user-provided file paths against path traversal
        for file in &input.files {
            validate_path_within_root(&context.working_dir, file)?;
        }

        let pipeline = vil_ir::IrPipeline::new(&context.working_dir)
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to build IR pipeline: {e}")))?;

        let files: Vec<String> = if input.files.is_empty() {
            pipeline.modules().keys().cloned().collect()
        } else {
            input.files
        };

        let report = vil_validate::validate_changes(&pipeline, &files)
            .map_err(|e| ToolError::ExecutionFailed(format!("Validation failed: {e}")))?;

        let pass_filter = input.pass_filter.as_str();
        let issues: Vec<IssueEntry> = report
            .issues
            .iter()
            .filter(|issue| {
                if pass_filter == "all" {
                    return true;
                }
                let issue_lower = issue.to_lowercase();
                match pass_filter {
                    "zero_copy" => {
                        issue_lower.contains("zero-copy")
                            || issue_lower.contains("shmslice")
                            || issue_lower.contains("copies data")
                    }
                    "observability" => {
                        issue_lower.contains("observability")
                            || issue_lower.contains("tracing")
                            || issue_lower.contains("requestid")
                    }
                    "vil_way" => {
                        issue_lower.contains("forbidden")
                            || issue_lower.contains("json<t>")
                            || issue_lower.contains("extension<t>")
                    }
                    "tri_lane" => {
                        issue_lower.contains("lane")
                            || issue_lower.contains("blocking")
                    }
                    "plumbing" => {
                        issue_lower.contains("manually implements")
                            || issue_lower.contains("plumbing")
                    }
                    "semantic" => {
                        issue_lower.contains("boundary")
                            || issue_lower.contains("network")
                    }
                    "macro_coverage" => {
                        issue_lower.contains("semantic macros")
                            || issue_lower.contains("vil_state")
                            || issue_lower.contains("vil_event")
                    }
                    _ => true,
                }
            })
            .map(|msg| IssueEntry {
                message: msg.clone(),
            })
            .collect();

        let output = AuditOutput {
            overall_score: report.score,
            files_audited: files.len(),
            total_issues: issues.len(),
            issues,
        };

        serde_json::to_value(output).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}
