//! vil_repair — dry-run contract repair tool.
//!
//! Runs VIL validation, then maps each known issue pattern to a planned repair.
//! Currently returns suggestions only (dry_run: true). Tier C will add actual
//! edit execution.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};
use crate::security::validate_path_within_root;

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
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

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

        let abs_path = validate_path_within_root(&context.working_dir, &input.file)?;

        // Parse the module for real repair analysis
        let module = vil_ir::parser::parse_file_async(&abs_path)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to parse file: {e}")))?;
        let source = tokio::fs::read_to_string(&abs_path)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read file: {e}")))?;

        // Run real repair engine (3 deterministic patterns)
        let plan = vil_ir::refactor::generate_repair_plan(&module, &source, &input.file);

        // Also run validation for the score
        let pipeline = vil_ir::IrPipeline::new_async(&context.working_dir)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to build IR pipeline: {e}")))?;
        let files = vec![input.file.clone()];
        let report = vil_validate::validate_changes(&pipeline, &files)
            .map_err(|e| ToolError::ExecutionFailed(format!("Validation failed: {e}")))?;

        // Merge: real repair actions from engine + heuristic fallbacks for unmatched issues
        let mut planned_repairs: Vec<PlannedRepair> = plan
            .repairs
            .iter()
            .map(|r| PlannedRepair {
                issue: r.description.clone(),
                severity: r.severity.clone(),
                suggested_fix: format!(
                    "Line {}: replace cols {}..{} with `{}`",
                    r.edit.line,
                    r.edit.col_start,
                    r.edit.col_end,
                    r.edit.new_text.trim()
                ),
                rationale: format!("Repair pattern: {}", r.pattern),
            })
            .collect();

        // Add heuristic fallbacks for issues not covered by the 3 patterns
        for issue in &report.issues {
            if let Some(fallback) = issue_to_repair(issue) {
                // Avoid duplicating issues already covered by the engine
                let dominated = planned_repairs.iter().any(|r| {
                    r.issue
                        .contains(&fallback.issue[..fallback.issue.len().min(40)])
                });
                if !dominated {
                    planned_repairs.push(fallback);
                }
            }
        }

        let engine_repair_count = plan.repairs.len();
        let output = RepairOutput {
            file: input.file,
            dry_run: true,
            overall_score: report.score,
            planned_repairs,
            note: format!(
                "Engine produced {} concrete repair(s) (zero_copy, observability, semantic_macro). \
                Remaining suggestions are heuristic. Use file_edit to apply.",
                engine_repair_count
            ),
        };

        serde_json::to_value(output).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}
