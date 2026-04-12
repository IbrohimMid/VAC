//! `vac runtime` — background task runtime management.

use std::path::PathBuf;

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

pub async fn execute_jobs(_project_root: PathBuf) -> anyhow::Result<()> {
    println!("No jobs in queue (runtime not attached to persistent queue in this session).");
    println!("Start a persistent runtime session to see queued jobs.");
    Ok(())
}
