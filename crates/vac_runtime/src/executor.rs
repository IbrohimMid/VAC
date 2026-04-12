use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

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

    pub fn allows_auto_write(&self) -> bool {
        matches!(self, Self::AutoFixLowRisk)
    }

    pub fn produces_patches(&self) -> bool {
        matches!(self, Self::PatchProposal | Self::AutoFixLowRisk)
    }
}

pub struct TaskExecutor {
    pub project_root: PathBuf,
    pub operating_mode: OperatingMode,
    /// Live engine handle — None until attach_engine() is called.
    engine: Option<Arc<Mutex<vac_core::VacEngine>>>,
}

impl TaskExecutor {
    pub fn new(project_root: PathBuf, mode: OperatingMode) -> Self {
        Self { project_root, operating_mode: mode, engine: None }
    }

    /// Attach a live VacEngine. Must be called before executing RunTask jobs.
    pub fn attach_engine(&mut self, engine: Arc<Mutex<vac_core::VacEngine>>) {
        self.engine = Some(engine);
    }

    pub async fn execute(&self, job: &Job) -> anyhow::Result<String> {
        match &job.kind {
            JobKind::RunTask { description } => {
                match self.operating_mode {
                    OperatingMode::MonitorOnly => {
                        Ok(format!("Monitor-only: would run '{description}' (engine not invoked)"))
                    }
                    OperatingMode::SuggestOnly => {
                        Ok(format!("Suggest-only: task '{description}' noted but not executed"))
                    }
                    OperatingMode::PatchProposal | OperatingMode::AutoFixLowRisk => {
                        // Delegate to real engine
                        let engine = self.engine.as_ref()
                            .ok_or_else(|| anyhow::anyhow!(
                                "TaskExecutor has no engine attached — call attach_engine() first"
                            ))?;
                        let result = engine.lock().await.run_task(description).await
                            .map_err(|e| anyhow::anyhow!("Engine error: {e}"))?;
                        Ok(format!(
                            "Task completed: {} | modified: {} | tokens: {}",
                            result.summary,
                            result.modified_files.len(),
                            result.total_tokens_used
                        ))
                    }
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
                    Ok("Diagnostic sweep: no cache found (vil-lsp not running)".to_string())
                }
            }
            JobKind::RulebookComplianceCheck => {
                let books = vac_core::rulebook::RulebookLoader::load_all(&self.project_root, &[]);
                let result = vac_core::rulebook::validate_rulebooks(&books);
                if result.is_valid() {
                    Ok(format!("Rulebook compliance: {} book(s) valid", books.len()))
                } else {
                    Ok(format!(
                        "Rulebook compliance: {} error(s) — {}",
                        result.errors.len(),
                        result.errors.join("; ")
                    ))
                }
            }
            JobKind::PatchProposal { files } => {
                if !self.operating_mode.produces_patches() {
                    return Ok(format!("Patch proposal skipped (mode: {:?})", self.operating_mode));
                }
                let engine = self.engine.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("No engine attached for patch proposal"))?;
                let task = format!("Review and propose patches for: {}", files.join(", "));
                let result = engine.lock().await.run_task(&task).await
                    .map_err(|e| anyhow::anyhow!("Engine error: {e}"))?;
                Ok(format!(
                    "Patch proposal: {} file(s) modified — {}",
                    result.modified_files.len(),
                    result.summary
                ))
            }
        }
    }
}

/// Returns true if a tool is safe for auto-execution in AutoFixLowRisk mode.
pub fn is_low_risk_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "file_read" | "glob" | "grep" | "search"
            | "vil_knowledge" | "vil_diagnostics" | "vil_status" | "vil_lsp_query"
    )
}
