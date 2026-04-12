//! `vac run "<task>"` — Execute a single task.

use std::path::PathBuf;
use vac_core::{Priority, ProfileName, ProfileOverride};

pub async fn execute(
    project_root: PathBuf,
    task_description: String,
    priority: String,
    profile: String,
    _require_approval: bool,
    targets: Vec<String>,
) -> anyhow::Result<()> {
    let mut engine = vac_core::VacEngine::new(project_root).await?;
    engine.init().await?;

    let priority = match priority.to_lowercase().as_str() {
        "low" => Priority::Low,
        "high" => Priority::High,
        "critical" => Priority::Critical,
        _ => Priority::Normal,
    };

    let profile_name = ProfileName::from_str(&profile);
    let profile_override = ProfileOverride::resolve(&profile_name);

    println!("🤖 Running task: {}", task_description);
    println!("   Priority: {:?} | Profile: {}", priority, profile_name);
    if profile_override.planner_gate {
        println!("   ⚡ Planner gate: ENFORCED");
    }
    if profile_override.validator_blocks {
        println!("   🛡️  Validator: BLOCKING");
    }

    if !targets.is_empty() {
        println!("   Targets: {}", targets.join(", "));
    }

    let result = engine.run_task(&task_description).await?;

    println!("\n{}", "=".repeat(60));
    match &result.status {
        vac_core::TaskStatus::Completed => println!("✓ Task completed successfully!"),
        vac_core::TaskStatus::Failed(reason) => println!("❌ Task failed: {}", reason),
        _ => println!("⚠️  Task ended with status: {:?}", result.status),
    }

    println!("\n📋 Summary: {}", result.summary);

    if !result.modified_files.is_empty() {
        println!("\n📝 Modified files:");
        for file in &result.modified_files { println!("   • {}", file); }
    }
    if !result.created_files.is_empty() {
        println!("\n✨ Created files:");
        for file in &result.created_files { println!("   • {}", file); }
    }
    if let Some(score) = result.validation_score {
        println!("\n🎯 IR Validation Score: {:.1}%", score * 100.0);
    }
    println!("\n⏱️  Elapsed: {}ms | Tokens: {}", result.elapsed_ms, result.total_tokens_used);

    Ok(())
}
