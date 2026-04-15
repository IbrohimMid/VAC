//! `vac run "<task>"` — Execute a single task.

use std::path::PathBuf;
use vac_core::{Priority, ProfileName, ProfileOverride};

pub async fn execute(
    project_root: PathBuf,
    task_description: String,
    priority: String,
    profile: String,
    approve: bool,
    targets: Vec<String>,
) -> anyhow::Result<()> {
    let mut engine = vac_core::VacEngine::new(project_root).await?;

    if approve {
        // Build a policy engine that explicitly allows all tools
        let config_stub = vac_tools::router::ToolConfigStub {
            default_policy: "allow".into(),
            allow: std::collections::HashMap::new(),
            deny: std::collections::HashMap::new(),
        };

        let adapter = std::sync::Arc::new(
            vac_tools::router::DefaultPolicyEngine::new(config_stub)
        );
        engine.init_with_policy(Some(adapter)).await?;
    } else {
        engine.init_with_policy(None).await?;
    }

    let priority = match priority.to_lowercase().as_str() {
        "low" => Priority::Low,
        "high" => Priority::High,
        "critical" => Priority::Critical,
        _ => Priority::Normal,
    };

    let profile_name = ProfileName::from_str(&profile);
    let profile_override = ProfileOverride::resolve(&profile_name);

    println!("🤖 Running task: {}", task_description);
    if approve {
        println!("⚠️  Auto-approval enabled: Tools will run without confirmation.");
    }
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

    let (update_tx, mut update_rx) = tokio::sync::mpsc::unbounded_channel::<vac_core::engine::RuntimeUpdate>();
    
    // If --approve is set, we use an interactive approval channel.
    // In a real CLI, this would need a TUI or stdin prompt.
    // For now, we wire it to a dummy channel that auto-rejects if not interactive.
    let (approval_tx, approval_rx) = tokio::sync::mpsc::unbounded_channel::<vil_swarm::ApprovalResponse>();
    let _ = approval_tx; // Keep for future stdin-to-channel wiring

    tokio::spawn(async move {
        while let Some(update) = update_rx.recv().await {
            match update {
                vac_core::engine::RuntimeUpdate::Status(msg) => println!("⏳ {}", msg),
                vac_core::engine::RuntimeUpdate::ToolCall { name, .. } => println!("🛠️  Calling: {}", name),
                vac_core::engine::RuntimeUpdate::ApprovalRequired { tool_name, .. } => {
                    if !approve {
                        println!("🛡️  Tool '{}' requires approval but --approve is not set. Auto-rejecting.", tool_name);
                    } else {
                        println!("🛡️  Tool '{}' requires approval. (Interactive approval in 'vac run' is coming soon, use 'vac interactive' for now)", tool_name);
                    }
                }
                _ => {}
            }
        }
    });

    let result = if approve {
        engine.run_task_with_approvals(&task_description, Some(update_tx), None, Some(approval_rx)).await?
    } else {
        engine.run_task_with_updates(&task_description, Some(update_tx)).await?
    };

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
