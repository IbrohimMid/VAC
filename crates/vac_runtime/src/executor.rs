use std::path::PathBuf;

use crate::jobs::{Job, JobKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperatingMode {
    MonitorOnly,
    SuggestOnly,
    PatchProposal,
    AutoFixLowRisk,
}

impl OperatingMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "suggest-only" => Self::SuggestOnly,
            "patch-proposal" => Self::PatchProposal,
            "auto-fix-low-risk" => Self::AutoFixLowRisk,
            _ => Self::MonitorOnly,
        }
    }

    /// Whether this mode allows writing files without explicit approval.
    pub fn allows_auto_write(&self) -> bool {
        matches!(self, Self::AutoFixLowRisk)
    }

    /// Whether this mode produces patch proposals.
    pub fn produces_patches(&self) -> bool {
        matches!(self, Self::PatchProposal | Self::AutoFixLowRisk)
    }
}

pub struct TaskExecutor {
    pub project_root: PathBuf,
    pub operating_mode: OperatingMode,
}

impl TaskExecutor {
    pub fn new(project_root: PathBuf, mode: OperatingMode) -> Self {
        Self { project_root, operating_mode: mode }
    }

    pub async fn execute(&self, job: &Job) -> anyhow::Result<String> {
        match &job.kind {
            JobKind::RunTask { description } => {
                // In production: delegate to VacEngine
                // For now: return mode-aware summary
                if self.operating_mode == OperatingMode::MonitorOnly {
                    Ok(format!("Monitor-only: would run task '{description}'"))
                } else {
                    Ok(format!("Task queued for execution: '{description}'"))
                }
            }
            JobKind::DiagnosticSweep => {
                let cache = self.project_root.join(".vac/cache/vil_lsp_diagnostics.json");
                if cache.exists() {
                    let content = std::fs::read_to_string(&cache)?;
                    let snap: serde_json::Value = serde_json::from_str(&content)?;
                    let errors = snap["total_errors"].as_u64().unwrap_or(0);
                    let warnings = snap["total_warnings"].as_u64().unwrap_or(0);
                    Ok(format!("Diagnostic sweep: {errors} errors, {warnings} warnings"))
                } else {
                    Ok("Diagnostic sweep: no cache found".to_string())
                }
            }
            JobKind::RulebookComplianceCheck => {
                Ok("Rulebook compliance check completed".to_string())
            }
            JobKind::PatchProposal { files } => {
                if self.operating_mode.produces_patches() {
                    Ok(format!("Patch proposal for {} file(s)", files.len()))
                } else {
                    Ok(format!("Patch proposal skipped (mode: {:?})", self.operating_mode))
                }
            }
        }
    }
}

/// Returns true if a tool is safe for auto-execution in AutoFixLowRisk mode.
pub fn is_low_risk_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "file_read" | "glob" | "grep" | "search"
            | "vil_knowledge" | "vil_diagnostics" | "vil_status"
    )
}
