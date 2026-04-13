//! Resume command — restore session info from checkpoint (restore-first semantics).
//! Does NOT auto-continue execution. Use TUI for interactive continuation.

use std::path::PathBuf;

pub async fn execute(project_root: PathBuf, checkpoint_path: PathBuf) -> anyhow::Result<()> {
    println!("Loading checkpoint from: {}", checkpoint_path.display());

    let checkpoint = vil_swarm::checkpoint::load_checkpoint_from_file(&checkpoint_path)
        .map_err(|e| anyhow::anyhow!("Failed to load checkpoint: {}", e))?;

    println!("\n{}", "=".repeat(60));
    println!("✓ Session restored (restore-first mode)");
    println!("{}", "=".repeat(60));

    // Display session info
    if let Some(run_id) = &checkpoint.run_id {
        println!("Session ID: {}", run_id);
    }

    println!("Messages: {}", checkpoint.messages.len());

    let meta = &checkpoint.metadata;
    if let Some(last_task) = meta.get("last_task_id") {
        println!("Last Task ID: {}", last_task);
    }
    if let Some(status) = meta.get("last_status") {
        println!("Last Status: {}", status);
    }
    if let Some(tokens) = meta.get("total_tokens") {
        println!("Total Tokens: {}", tokens);
    }
    if let Some(completed) = meta.get("completed_tasks") {
        println!("Completed Tasks: {}", completed);
    }

    println!("\n📝 Conversation history:");
    for (i, msg) in checkpoint.messages.iter().enumerate() {
        let role = format!("{:?}", msg.role);
        let preview = if msg.content.len() > 80 {
            format!("{}...", &msg.content[..77])
        } else {
            msg.content.clone()
        };
        println!("  [{}] {}: {}", i + 1, role, preview);
    }

    println!("\n{}", "=".repeat(60));
    println!("ℹ️  Session loaded. Use 'vac interactive' to continue.");
    println!("{}", "=".repeat(60));

    Ok(())
}
