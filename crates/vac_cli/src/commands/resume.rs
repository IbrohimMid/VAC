//! Resume command — restore agent loop from checkpoint.

use std::path::PathBuf;
use vil_swarm::run_state::AgentRunState;

pub async fn execute(project_root: PathBuf, checkpoint_path: PathBuf) -> anyhow::Result<()> {
    println!("Loading checkpoint from: {}", checkpoint_path.display());

    let state = AgentRunState::from_checkpoint(&checkpoint_path)
        .map_err(|e| anyhow::anyhow!("Failed to load checkpoint: {}", e))?;

    println!(
        "✓ Checkpoint loaded: {} iterations, {} tokens, stage: {}",
        state.iterations, state.total_tokens, state.stage
    );

    // Extract last user message as task description
    let task_description = state.messages
        .iter()
        .rev()
        .find(|m| format!("{:?}", m.role).to_lowercase().contains("user"))
        .map(|m| m.content.clone())
        .unwrap_or_else(|| "Continue from checkpoint".to_string());

    println!("🤖 Resuming task: {}", task_description);
    
    // Initialize engine and continue execution
    let mut engine = vac_core::VacEngine::new(project_root).await?;
    engine.init().await?;
    
    let result = engine.run_task(&task_description).await?;
    
    println!("\n{}", "=".repeat(60));
    match &result.status {
        vac_core::TaskStatus::Completed => println!("✓ Task completed successfully!"),
        vac_core::TaskStatus::Failed(reason) => println!("❌ Task failed: {}", reason),
        _ => println!("⚠️  Task ended with status: {:?}", result.status),
    }

    println!("\n📋 Summary: {}", result.summary);
    
    Ok(())
}
