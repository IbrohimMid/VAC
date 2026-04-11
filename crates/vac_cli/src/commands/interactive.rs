//! `vac interactive` — REPL mode with streaming output.

use std::io::{self, Write};
use std::path::PathBuf;

pub async fn execute(project_root: PathBuf, _resume: bool) -> anyhow::Result<()> {
    let mut engine = vac_core::VacEngine::new(project_root).await?;
    engine.init().await?;

    println!("🤖 VAC Interactive Mode");
    println!("   Type your task or question. Type 'exit' or 'quit' to leave.");
    println!("   Type 'status' for engine status.");
    println!("   Type 'help' for more commands.");
    println!("{}", "=".repeat(60));

    let stdin = io::stdin();
    let mut input = String::new();

    loop {
        print!("\n❯ ");
        io::stdout().flush()?;

        input.clear();
        if stdin.read_line(&mut input)? == 0 {
            break; // EOF
        }

        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }

        match trimmed.to_lowercase().as_str() {
            "exit" | "quit" | "q" => {
                println!("👋 Goodbye!");
                break;
            }
            "status" => {
                let status = engine.status().await?;
                println!("\n📊 Engine Status:");
                println!("   Session: {}", status.session_id);
                println!(
                    "   Tasks: {} total, {} completed, {} failed",
                    status.total_tasks, status.completed_tasks, status.failed_tasks
                );
                println!("   Tokens used: {}", status.total_tokens_used);
                println!("   Initialized: {}", status.subsystems_initialized);
            }
            "help" => {
                println!("\n❓ Available commands:");
                println!("   <task>    Execute a development task");
                println!("   status    Show engine status");
                println!("   help      Show this help");
                println!("   exit      Exit interactive mode");
            }
            task => {
                let result = engine.run_task(task).await?;
                println!("\n✓ {}", result.summary);
                if !result.modified_files.is_empty() {
                    println!("   Modified: {}", result.modified_files.join(", "));
                }
                if let Some(score) = result.validation_score {
                    println!("   Validation: {:.1}%", score * 100.0);
                }
                println!(
                    "   ⏱️ {}ms | {} tokens",
                    result.elapsed_ms, result.total_tokens_used
                );
            }
        }
    }

    Ok(())
}
