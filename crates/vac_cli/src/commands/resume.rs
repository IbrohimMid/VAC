//! Resume command — restore session info from checkpoint and resume execution.

use std::path::PathBuf;

pub async fn execute(project_root: PathBuf, checkpoint_path: PathBuf) -> anyhow::Result<()> {
    println!("Loading checkpoint from: {}", checkpoint_path.display());

    let session_id_str = checkpoint_path.file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid checkpoint filename"))?;
        
    let session_id = uuid::Uuid::parse_str(session_id_str.replace("_state", "").as_str())
        .map_err(|e| anyhow::anyhow!("Failed to parse session ID: {}", e))?;

    println!("\n{}", "=".repeat(60));
    println!("✓ Session restored (headless mode)");
    println!("{}", "=".repeat(60));

    let mut engine = vac_core::engine::VacEngine::new(project_root).await?;
    // We don't have interactive approval here, so we pass None
    // To support headless resume with approvals, the user should use 'vac interactive'
    
    let result = engine.resume_run_state(session_id, None, None, None).await?;
    
    println!("\nTask Result: {:?}", result.status);
    println!("Summary: {}", result.summary);

    Ok(())
}
