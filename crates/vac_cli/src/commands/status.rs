//! `vac status` — Show active agents and progress.

use std::path::PathBuf;

pub async fn execute(project_root: PathBuf) -> anyhow::Result<()> {
    let engine = vac_core::VacEngine::new(project_root).await?;
    let status = engine.status().await?;

    println!("📊 VAC Status");
    println!("{}", "=".repeat(40));
    println!("Project:      {}", status.project_root.display());
    println!("Session:      {}", status.session_id);
    println!(
        "Initialized:  {}",
        if status.subsystems_initialized {
            "✓ Yes"
        } else {
            "❌ No"
        }
    );
    println!("\nTasks:");
    println!("  Total:     {}", status.total_tasks);
    println!("  Completed: {}", status.completed_tasks);
    println!("  Failed:    {}", status.failed_tasks);
    println!("\nResources:");
    println!("  Tokens used: {}", status.total_tokens_used);

    Ok(())
}
