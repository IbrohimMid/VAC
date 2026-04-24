pub mod agent_list;
pub mod bash;
pub mod canonical_lint;
pub mod cargo;
pub mod cron_crud;
pub mod hook_registry;
pub mod monitor;
pub mod web_fetch;
pub mod web_search;
pub mod file_edit;
pub mod file_read;
pub mod file_write;
pub mod git;
pub mod glob;
pub mod grep;
pub mod knowledge;
pub mod plan_mode;
pub mod schedule_cron;
pub mod search;
pub mod sequential_think;
pub mod signal_list;
pub mod signal_tail;
pub mod skill_tool;
pub mod task_suite;
pub mod tool_search;
pub mod utility;
pub mod worktree;
#[cfg(test)]
pub(crate) mod test_util;
pub mod skill_runner;
pub mod task_done;
pub mod todo;
pub mod vil_audit;
pub mod vil_diagnostics;
pub mod vil_ir_diff;
pub mod vil_lsp_query;
pub mod vil_plumbing;
pub mod vil_repair;
pub mod vil_status;
pub mod rust_analysis;

use crate::ToolError;
use crate::registry::ToolRegistry;
use std::sync::Arc;

pub async fn register_builtin_tools(registry: &Arc<ToolRegistry>) -> Result<(), ToolError> {
    registry.register(file_read::FileReadTool::new()).await?;
    registry.register(file_write::FileWriteTool::new()).await?;
    registry.register(file_edit::FileEditTool).await?;
    registry.register(glob::GlobTool).await?;
    registry.register(grep::GrepTool).await?;
    registry.register(knowledge::KnowledgeTool::new()).await?;
    registry.register(bash::BashTool::new()).await?;
    registry.register(git::GitTool::new()).await?;
    registry.register(cargo::CargoTool::new()).await?;
    registry.register(search::SearchTool::new()).await?;
    registry.register(signal_tail::SignalTailTool::new()).await?;
    registry.register(signal_list::SignalListTool::new()).await?;
    registry.register(plan_mode::EnterPlanModeTool::new()).await?;
    registry.register(plan_mode::ExitPlanModeTool::new()).await?;
    registry.register(worktree::EnterWorktreeTool::new()).await?;
    registry.register(worktree::ExitWorktreeTool::new()).await?;
    registry.register(schedule_cron::ScheduleCronTool::new()).await?;
    registry.register(task_suite::TaskCreateTool::new()).await?;
    registry.register(task_suite::TaskListTool::new()).await?;
    registry.register(task_suite::TaskStopTool::new()).await?;
    registry.register(task_suite::TaskOutputTool::new()).await?;
    registry
        .register(tool_search::ToolSearchTool::new(registry.clone()))
        .await?;
    registry.register(utility::SleepTool::new()).await?;
    registry.register(utility::SendMessageTool::new()).await?;
    registry.register(task_done::TaskDoneTool::new()).await?;
    registry.register(todo::TodoTool::default()).await?;
    registry
        .register(sequential_think::SequentialThinkTool::new())
        .await?;
    registry
        .register(vil_status::VilStatusTool::new(registry.clone()))
        .await?;
    registry
        .register(vil_diagnostics::VilDiagnosticsTool::new())
        .await?;
    registry
        .register(vil_lsp_query::VilLspQueryTool::new())
        .await?;
    registry.register(vil_ir_diff::VilIrDiffTool::new()).await?;
    registry.register(vil_audit::VilAuditTool::new()).await?;
    registry
        .register(vil_plumbing::VilPlumbingTool::new())
        .await?;
    registry.register(vil_repair::VilRepairTool::new()).await?;
    registry
        .register(canonical_lint::CanonicalLintTool::new())
        .await?;
    // NS.1 — session-primitive wrappers.
    registry.register(agent_list::AgentListTool::new()).await?;
    registry.register(web_fetch::WebFetchTool::new()).await?;
    registry.register(web_search::WebSearchTool::new()).await?;
    registry.register(cron_crud::CronListTool::new()).await?;
    registry.register(cron_crud::CronDeleteTool::new()).await?;
    registry.register(monitor::MonitorTool::new()).await?;
    registry.register(hook_registry::HookListTool::new()).await?;
    registry.register(hook_registry::HookDeleteTool::new()).await?;

    let skills_dir = std::path::PathBuf::from(".vac/skills");
    registry
        .register(skill_runner::SkillRunnerTool::new(
            skills_dir,
            registry.clone(),
        ))
        .await?;

    let host = std::sync::Arc::new(crate::rust_analysis::PortablePtyHost::new().unwrap_or_else(|_| {
        tracing::warn!("Failed to initialize PortablePtyHost, falling back to StubAnalysisHost");
        // We can't return StubAnalysisHost here because the types differ, but wait, both implement AnalysisHost.
        panic!("Failed to initialize PortablePtyHost");
    }));
    registry.register(rust_analysis::RustSymbolLookup::new(host.clone())).await?;
    registry.register(rust_analysis::RustDiagnostics::new(host)).await?;

    Ok(())
}
