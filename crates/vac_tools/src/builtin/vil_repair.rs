//! vil_repair — dry-run contract repair tool.
//!
//! Runs VIL validation, then maps each known issue pattern to a planned repair.
//! Currently returns suggestions only (dry_run: true). Tier C will add actual
//! edit execution.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

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

#[derive(Debug, Deserialize)]
struct RepairInput {
    /// File path relative to project root.
    file: String,
}

#[derive(Debug, Serialize)]
struct RepairOutput {
    file: String,
    dry_run: bool,
    overall_score: f64,
    planned_repairs: Vec<PlannedRepair>,
    note: String,
}

#[derive(Debug, Serialize)]
struct PlannedRepair {
    issue: String,
    severity: String,
    suggested_fix: String,
    rationale: String,
}

pub struct VilRepairTool;

impl VilRepairTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for VilRepairTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a validation issue message to a planned repair suggestion.
fn issue_to_repair(issue: &str) -> Option<PlannedRepair> {
    let issue_lower = issue.to_lowercase();

    if issue_lower.contains("copies data") || issue_lower.contains("shmslice") {
        // Zero-copy violation
        return Some(PlannedRepair {
            issue: issue.to_string(),
            severity: "high".into(),
            suggested_fix: "Change parameter type from String/Vec<u8> to ShmSlice<'a> or Bytes \
                for zero-copy body extraction on the network boundary."
                .into(),
            rationale: "Network boundary handlers must avoid copying request body data. \
                ShmSlice provides zero-copy access to shared memory regions."
                .into(),
        });
    }

    if issue_lower.contains("json<t>") && issue_lower.contains("forbidden") {
        return Some(PlannedRepair {
            issue: issue.to_string(),
            severity: "high".into(),
            suggested_fix: "Replace Json<T> parameter with ShmSlice and deserialize manually, \
                or use VilRequest<T> which handles zero-copy deserialization."
                .into(),
            rationale: "Json<T> copies the entire request body. VIL handlers must use \
                the zero-copy pipeline for body extraction."
                .into(),
        });
    }

    if issue_lower.contains("extension<t>") && issue_lower.contains("forbidden") {
        return Some(PlannedRepair {
            issue: issue.to_string(),
            severity: "medium".into(),
            suggested_fix: "Replace Extension<T> with ServiceCtx which provides typed \
                access to shared services via the VIL dependency injection system."
                .into(),
            rationale: "Extension<T> bypasses VIL's service lifecycle management. \
                ServiceCtx ensures proper initialization ordering and shutdown."
                .into(),
        });
    }

    if issue_lower.contains("returns json<t>") {
        return Some(PlannedRepair {
            issue: issue.to_string(),
            severity: "medium".into(),
            suggested_fix: "Replace Json<T> return type with VilResponse which handles \
                content negotiation and response serialization."
                .into(),
            rationale: "VilResponse integrates with VIL's response pipeline including \
                compression, content negotiation, and observability."
                .into(),
        });
    }

    if issue_lower.contains("no tracing") || issue_lower.contains("no #[tracing::instrument]") {
        return Some(PlannedRepair {
            issue: issue.to_string(),
            severity: "low".into(),
            suggested_fix: "Add #[tracing::instrument(skip_all)] above the handler function \
                and ensure `use tracing;` is imported."
                .into(),
            rationale: "VIL requires observability completeness on network paths for \
                distributed tracing and debugging."
                .into(),
        });
    }

    if issue_lower.contains("manually implements") {
        return Some(PlannedRepair {
            issue: issue.to_string(),
            severity: "medium".into(),
            suggested_fix: "Remove the manual impl block and add the corresponding VIL \
                macro (#[vil_message], #[vil_state]) to the struct definition instead."
                .into(),
            rationale: "Manual implementations of VIL traits bypass macro-generated \
                plumbing including serialization format guarantees and version compatibility."
                .into(),
        });
    }

    if issue_lower.contains("semantic macros") || issue_lower.contains("lacks explicit") {
        return Some(PlannedRepair {
            issue: issue.to_string(),
            severity: "low".into(),
            suggested_fix: "Add the appropriate VIL semantic macro: #[vil_state] for state, \
                #[vil_event] for events, #[vil_fault] for error types, \
                #[vil_decision] for decision records."
                .into(),
            rationale: "Explicit VIL macros enable the IR to track semantic roles, \
                generate correct plumbing, and enforce boundary rules."
                .into(),
        });
    }

    if issue_lower.contains("fast lane") && issue_lower.contains("blocking") {
        return Some(PlannedRepair {
            issue: issue.to_string(),
            severity: "high".into(),
            suggested_fix: "Move blocking calls (std::fs, std::thread::sleep, reqwest::blocking) \
                to the Compute Lane, or replace with async equivalents (tokio::fs, tokio::time::sleep)."
                .into(),
            rationale: "Fast Lane handlers must not block the event loop. Synchronous I/O \
                starves other handlers sharing the same runtime."
                .into(),
        });
    }

    None
}

#[async_trait]
impl VilTool for VilRepairTool {
    fn name(&self) -> &str {
        "vil_repair"
    }

    fn description(&self) -> &str {
        "Analyze VIL contract violations in a file and suggest concrete repairs. \
        Currently operates in dry-run mode: returns planned fixes without applying them. \
        Use this to understand what needs to change before editing."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file": {
                    "type": "string",
                    "description": "File path relative to project root"
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
        let input: RepairInput =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        validate_path_within_root(&context.working_dir, &input.file)?;

        let pipeline = vil_ir::IrPipeline::new(&context.working_dir)
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to build IR pipeline: {e}")))?;

        let files = vec![input.file.clone()];
        let report = vil_validate::validate_changes(&pipeline, &files)
            .map_err(|e| ToolError::ExecutionFailed(format!("Validation failed: {e}")))?;

        let planned_repairs: Vec<PlannedRepair> =
            report.issues.iter().filter_map(|i| issue_to_repair(i)).collect();

        let output = RepairOutput {
            file: input.file,
            dry_run: true,
            overall_score: report.score,
            planned_repairs,
            note: "Dry-run mode: repairs are suggestions only. Edit execution engine \
                is planned for a future release."
                .into(),
        };

        serde_json::to_value(output).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}
