//! `vac runtime` — background task runtime management.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn execute_status(project_root: PathBuf) -> anyhow::Result<()> {
    let config = vac_core::VacConfig::load_with_fallback(&project_root)?;
    println!("VAC Runtime Status");
    println!("  enabled:        {}", config.runtime.enable);
    println!("  operating_mode: {}", config.runtime.operating_mode);
    println!("  max_jobs:       {}", config.runtime.max_concurrent_jobs);
    if !config.runtime.enable {
        println!("\n  Runtime is disabled. Set [runtime] enable = true in .vac/config.toml to activate.");
    }
    Ok(())
}

pub async fn execute_jobs(project_root: PathBuf) -> anyhow::Result<()> {
    let config = vac_core::VacConfig::load_with_fallback(&project_root)?;
    if !config.runtime.enable {
        println!("Runtime is disabled. No jobs.");
        return Ok(());
    }
    println!("No persistent job queue in this session.");
    println!("Use `vac runtime start` (future) to attach a persistent scheduler.");
    Ok(())
}

/// Start the runtime scheduler with a live engine attached.
/// This is the production entry point for autonomous operation.
pub async fn execute_start(project_root: PathBuf) -> anyhow::Result<()> {
    let config = vac_core::VacConfig::load_with_fallback(&project_root)?;
    if !config.runtime.enable {
        anyhow::bail!("Runtime is disabled. Set [runtime] enable = true in .vac/config.toml");
    }

    println!("🚀 Starting VAC runtime scheduler...");
    println!("   mode: {}", config.runtime.operating_mode);

    // Initialize engine
    let mut engine = vac_core::VacEngine::new(project_root.clone()).await?;
    engine.init().await?;
    let engine = Arc::new(Mutex::new(engine));

    // Build executor with engine attached
    let mode = vac_runtime::OperatingMode::from_str(&config.runtime.operating_mode);
    let mut executor = vac_runtime::TaskExecutor::new(project_root, mode);
    executor.attach_engine(engine);

    let queue = Arc::new(vac_runtime::TaskQueue::new());
    let scheduler = vac_runtime::Scheduler::new(queue.clone(), Arc::new(executor));
    scheduler.start();

    println!("✓ Scheduler running. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await?;
    scheduler.stop();
    println!("\n✓ Runtime stopped.");
    Ok(())
}
