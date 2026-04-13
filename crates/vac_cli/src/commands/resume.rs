//! Resume command — restore agent loop from checkpoint.

use std::path::PathBuf;
use vil_swarm::run_state::AgentRunState;

pub async fn execute(_project_root: PathBuf, checkpoint_path: PathBuf) -> anyhow::Result<()> {
    println!("Loading checkpoint from: {}", checkpoint_path.display());

    let state = AgentRunState::from_checkpoint(&checkpoint_path)
        .map_err(|e| anyhow::anyhow!("Failed to load checkpoint: {}", e))?;

    println!(
        "✓ Checkpoint loaded: {} iterations, {} tokens, stage: {}",
        state.iterations, state.total_tokens, state.stage
    );

    println!("Resuming agent loop...");
    
    // TODO: Wire this into orchestrator.agent_loop_with_context with restored state
    // For now, just validate the checkpoint can be loaded
    
    println!("✓ Resume capability verified. Full orchestrator integration pending.");
    
    Ok(())
}
