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

        let adapter = std::sync::Arc::new(vac_tools::router::DefaultPolicyEngine::new(config_stub));
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

    let profile_name = ProfileName::parse(&profile);
    let profile_override = ProfileOverride::resolve(&profile_name);

    println!("🤖 Running task: {}", task_description);
    if approve {
        println!("⚠️  Permission Mode: AUTO-APPROVE (Tools will run without confirmation)");
    } else {
        println!("🛡️  Permission Mode: PROMPT (You will be prompted for tool execution)");
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

    let (update_tx, mut update_rx) =
        tokio::sync::mpsc::unbounded_channel::<vac_core::engine::RuntimeUpdate>();
    let approvals = engine.approval_handle();

    // If auto-approve is enabled, tools shouldn't require approval anyway (policy="allow").
    tokio::spawn(async move {
        while let Some(update) = update_rx.recv().await {
            match update {
                vac_core::engine::RuntimeUpdate::Status(msg) => {
                    println!("⏳ {}", msg);
                }
                vac_core::engine::RuntimeUpdate::ModelInfo { provider, model } => {
                    println!("🧠 Using Model: {} ({})", model, provider);
                }
                vac_core::engine::RuntimeUpdate::ToolCall { name, .. } => {
                    println!("🛠️  Running: {}", name);
                }
                vac_core::engine::RuntimeUpdate::ToolResult { name, success, .. } => {
                    if success {
                        println!("✅ Tool completed: {}", name);
                    } else {
                        println!("❌ Tool failed: {}", name);
                    }
                }
                vac_core::engine::RuntimeUpdate::ApprovalRequired {
                    tool_call_id,
                    tool_name,
                    ..
                } => {
                    if approve {
                        println!("\n[!] Unexpected approval required for: {}", tool_name);
                        continue;
                    }

                    println!("\n[!] Tool requires approval: {}", tool_name);

                    // Prompt user for approval interactively
                    use std::io::Write;
                    print!("Approve? [Y/n]: ");
                    let _ = std::io::stdout().flush();
                    let mut input = String::new();
                    if std::io::stdin().read_line(&mut input).is_ok() {
                        let input = input.trim().to_lowercase();
                        let approved = input.is_empty() || input == "y" || input == "yes";
                        if approved {
                            let _ = approvals.approve(tool_call_id).await;
                        } else {
                            let _ = approvals
                                .reject(tool_call_id, Some("Rejected by user via CLI".to_string()))
                                .await;
                        }
                    } else {
                        let _ = approvals
                            .reject(tool_call_id, Some("Input read error".to_string()))
                            .await;
                    }
                }
                _ => {}
            }
        }
    });

    let result = engine
        .run_task_with_approvals(&task_description, Some(update_tx), None, None)
        .await?;

    println!("\n{}", "=".repeat(60));
    match &result.status {
        vac_core::TaskStatus::Completed => println!("✓ Task completed successfully!"),
        vac_core::TaskStatus::Failed(reason) => println!("❌ Task failed: {}", reason),
        _ => println!("⚠️  Task ended with status: {:?}", result.status),
    }

    println!("\n📋 Summary: {}", result.summary);

    if !result.modified_files.is_empty() {
        println!("\n📝 Modified files:");
        for file in &result.modified_files {
            println!("   • {}", file);
        }
    }
    if !result.created_files.is_empty() {
        println!("\n✨ Created files:");
        for file in &result.created_files {
            println!("   • {}", file);
        }
    }
    if let Some(score) = result.validation_score {
        println!("\n🎯 IR Validation Score: {:.1}%", score * 100.0);
    }
    println!(
        "\n⏱️  Elapsed: {}ms | Tokens: {}",
        result.elapsed_ms, result.total_tokens_used
    );

    Ok(())
}
