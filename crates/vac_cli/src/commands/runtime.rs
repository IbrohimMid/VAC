//! `vac runtime` — background task runtime management.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

fn runtime_queue_path(project_root: &Path) -> PathBuf {
    project_root.join(".vac/queue.json")
}

pub async fn load_jobs(project_root: &Path) -> Vec<vac_runtime::Job> {
    vac_runtime::TaskQueue::with_storage(runtime_queue_path(project_root))
        .list()
        .await
}

pub fn load_autopilot_state(project_root: &Path) -> Option<vac_runtime::AutopilotStateFile> {
    let path = project_root.join(".vac/autopilot.state");
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub async fn execute_status(project_root: PathBuf, format: &str) -> anyhow::Result<()> {
    let config = vac_core::VacConfig::load_with_fallback(&project_root)?;
    let isolation =
        vac_runtime::IsolationManager::new(project_root.clone(), config.runtime.clone());
    if format == "json" {
        let status = serde_json::json!({
            "enabled": config.runtime.enable,
            "task_intent_mode": config.runtime.task_intent_mode,
            "environment_mode": config.runtime.environment_mode,
            "execution_environment": config.runtime.execution_environment,
            "network_policy": config.runtime.network_policy,
            "shell_execution_mode": config.runtime.shell_execution_mode(),
            "write_capability": config.runtime.write_capability(),
            "allowed_mcp_classes": config.runtime.allowed_mcp_classes(),
            "max_jobs": config.runtime.max_concurrent_jobs,
            "isolation": isolation.status_json(),
        });
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }

    println!("VAC Runtime Status");
    println!("  enabled:        {}", config.runtime.enable);
    println!("  task_intent:    {}", config.runtime.task_intent_mode);
    println!("  environment:    {}", config.runtime.environment_mode);
    println!(
        "  execution:      {:?}",
        config.runtime.execution_environment
    );
    println!("  network:        {:?}", config.runtime.network_policy);
    println!(
        "  shell:          {}",
        config.runtime.shell_execution_mode()
    );
    println!("  writes:         {}", config.runtime.write_capability());
    println!(
        "  mcp:            {}",
        config.runtime.allowed_mcp_classes().join(", ")
    );
    println!("  max_jobs:       {}", config.runtime.max_concurrent_jobs);
    if isolation.is_isolated() {
        println!("  container:      {}", isolation.container_runtime());
        println!(
            "  image:          {}",
            config
                .runtime
                .container_image
                .as_deref()
                .unwrap_or("<unset>")
        );
    }
    if !config.runtime.enable {
        println!(
            "\n  Runtime is disabled. Set [runtime] enable = true in .vac/config.toml to activate."
        );
    }
    Ok(())
}

pub async fn execute_jobs(project_root: PathBuf, format: &str) -> anyhow::Result<()> {
    let config = vac_core::VacConfig::load_with_fallback(&project_root)?;
    let jobs = load_jobs(&project_root).await;
    let state = load_autopilot_state(&project_root);

    if format == "json" {
        let jobs_json = serde_json::json!({
            "enabled": config.runtime.enable,
            "jobs": jobs,
            "state": state,
        });
        println!("{}", serde_json::to_string_pretty(&jobs_json)?);
        return Ok(());
    }

    if !config.runtime.enable {
        println!(
            "Runtime is disabled. Set [runtime] enable = true in .vac/config.toml to activate."
        );
    }

    if jobs.is_empty() {
        println!("No jobs in the queue.");
    } else {
        println!("VAC Runtime Jobs ({} total):", jobs.len());
        for job in jobs {
            let status = match job.status {
                vac_runtime::JobStatus::Queued => "Queued",
                vac_runtime::JobStatus::Running => "Running",
                vac_runtime::JobStatus::Completed => "Completed",
                vac_runtime::JobStatus::Failed(_) => "Failed",
                vac_runtime::JobStatus::Cancelled => "Cancelled",
            };
            let kind_str = match &job.kind {
                vac_runtime::JobKind::RunTask { description } => {
                    format!("RunTask: {}", description)
                }
                vac_runtime::JobKind::DiagnosticSweep => "DiagnosticSweep".to_string(),
                vac_runtime::JobKind::RulebookComplianceCheck => {
                    "RulebookComplianceCheck".to_string()
                }
                vac_runtime::JobKind::PatchProposal { .. } => "PatchProposal".to_string(),
                vac_runtime::JobKind::ToolCall { tool_name, .. } => {
                    format!("ToolCall: {}", tool_name)
                }
            };
            println!(
                "  [{}] {} ({}) - {:?}",
                status, job.id, kind_str, job.created_at
            );
        }
    }
    if let Some(state) = state {
        println!("\nAutopilot State:");
        println!("  mode:   {}", state.mode);
        println!("  queue:  {}", state.queue_len);
        println!("  state:  {:?}", state.state);
    }
    Ok(())
}

pub async fn execute_cancel(project_root: PathBuf, id: uuid::Uuid) -> anyhow::Result<()> {
    let queue = vac_runtime::TaskQueue::with_storage(runtime_queue_path(&project_root));
    if queue.cancel(id).await {
        println!("Job {} cancelled successfully.", id);
    } else {
        println!(
            "Failed to cancel job {}. It may not exist, or it is already completed/failed.",
            id
        );
    }
    Ok(())
}

pub async fn execute_retry(project_root: PathBuf, id: uuid::Uuid) -> anyhow::Result<()> {
    let queue = vac_runtime::TaskQueue::with_storage(runtime_queue_path(&project_root));
    if queue.retry(id).await {
        println!("Job {} queued for retry.", id);
    } else {
        println!(
            "Failed to retry job {}. It may not exist, or it is not in a failed/cancelled state.",
            id
        );
    }
    Ok(())
}

pub async fn execute_inspect(
    project_root: PathBuf,
    id: uuid::Uuid,
    format: &str,
) -> anyhow::Result<()> {
    let queue = vac_runtime::TaskQueue::with_storage(runtime_queue_path(&project_root));

    if let Some(job) = queue.get(id).await {
        if format == "json" {
            println!("{}", serde_json::to_string_pretty(&job)?);
        } else {
            let status = match &job.status {
                vac_runtime::JobStatus::Queued => "Queued".to_string(),
                vac_runtime::JobStatus::Running => "Running".to_string(),
                vac_runtime::JobStatus::Completed => "Completed".to_string(),
                vac_runtime::JobStatus::Failed(e) => format!("Failed: {}", e),
                vac_runtime::JobStatus::Cancelled => "Cancelled".to_string(),
            };

            let kind_str = match &job.kind {
                vac_runtime::JobKind::RunTask { description } => {
                    format!("RunTask: {}", description)
                }
                vac_runtime::JobKind::DiagnosticSweep => "DiagnosticSweep".to_string(),
                vac_runtime::JobKind::RulebookComplianceCheck => {
                    "RulebookComplianceCheck".to_string()
                }
                vac_runtime::JobKind::PatchProposal { .. } => "PatchProposal".to_string(),
                vac_runtime::JobKind::ToolCall { tool_name, .. } => {
                    format!("ToolCall: {}", tool_name)
                }
            };

            println!("Job Inspection: {}", job.id);
            println!("  Kind:       {}", kind_str);
            println!("  Status:     {}", status);
            println!("  Created:    {}", job.created_at);
            if let Some(started) = job.started_at {
                println!("  Started:    {}", started);
            }
            if let Some(completed) = job.completed_at {
                println!("  Completed:  {}", completed);
            }
            println!("  Retries:    {}/{}", job.retry_count, job.max_retries);
            if let Some(summary) = &job.result_summary {
                println!("  Summary:    {}", summary);
            }
        }
    } else {
        println!("Job {} not found.", id);
    }
    Ok(())
}

/// Start the runtime scheduler with a live engine attached.
/// This is the production entry point for autonomous operation.
pub async fn execute_start(project_root: PathBuf) -> anyhow::Result<()> {
    let config = vac_core::VacConfig::load_with_fallback(&project_root)?;
    if !config.runtime.enable {
        anyhow::bail!("Runtime is disabled. Set [runtime] enable = true in .vac/config.toml");
    }

    if should_wrap_in_isolation(&config.runtime) {
        let exe = std::env::current_exe()?;
        let isolation =
            vac_runtime::IsolationManager::new(project_root.clone(), config.runtime.clone());
        let tty = matches!(
            config.runtime.execution_environment,
            vac_core::ExecutionEnvironment::IsolatedInteractive
        );
        let args = vec![
            "--project".to_string(),
            project_root.display().to_string(),
            "runtime".to_string(),
            "start".to_string(),
        ];
        let code = isolation.run_foreground(&exe, &args, tty, std::collections::HashMap::new())?;
        if code != 0 {
            anyhow::bail!("Isolated runtime exited with code {code}");
        }
        return Ok(());
    }

    let autopilot = vac_core::config::AutopilotConfig::load(&project_root)?;

    println!("🚀 Starting VAC runtime scheduler...");
    println!("   task_intent: {}", config.runtime.task_intent_mode);
    println!("   environment: {}", config.runtime.environment_mode);
    println!("   execution: {:?}", config.runtime.execution_environment);
    println!("   autopilot.mode: {}", autopilot.mode);
    println!(
        "   autopilot.poll_interval_secs: {}",
        autopilot.poll_interval_secs
    );

    // Initialize engine
    let mut engine = vac_core::VacEngine::new(project_root.clone()).await?;
    let warnings = engine.init().await?;
    for warning in warnings {
        eprintln!("Warning: {}", warning);
    }
    let engine = Arc::new(Mutex::new(engine));

    // Build executor with engine attached
    let mode = vac_runtime::OperatingMode::parse(config.runtime.operating_mode());
    let environment_mode = vac_runtime::EnvironmentMode::parse(&config.runtime.environment_mode);
    let mut executor = vac_runtime::TaskExecutor::new_with_modes(
        project_root,
        mode,
        environment_mode,
        config.runtime.execution_environment,
    );
    executor.attach_engine(engine);

    let queue = Arc::new(vac_runtime::TaskQueue::with_storage(runtime_queue_path(
        &executor.project_root,
    )));
    let scheduler = vac_runtime::Scheduler::new(
        queue.clone(),
        Arc::new(executor),
        vac_runtime::SchedulerConfig {
            mode: autopilot.mode,
            poll_interval_secs: autopilot.poll_interval_secs,
        },
    );
    scheduler.start();

    println!("✓ Scheduler running. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await?;
    scheduler.stop();
    println!("\n✓ Runtime stopped.");
    Ok(())
}

fn should_wrap_in_isolation(runtime: &vac_core::RuntimeConfig) -> bool {
    runtime.execution_environment != vac_core::ExecutionEnvironment::Host
        && std::env::var("VAC_SKIP_ISOLATION_WRAPPER").ok().as_deref() != Some("1")
}
