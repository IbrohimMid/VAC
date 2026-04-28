//! `vac run "<task>"` — Execute a single task.
//!
//! Two engine paths coexist under a feature flag (R0 convergence):
//!
//! - **Legacy** (default) — `VacEngine::run_task_with_approvals` is
//!   the driver. Stable. Matches the shipped behaviour operators rely
//!   on today.
//! - **Session** — `VAC_ENGINE=session` env var OR `--engine session`
//!   flag routes the submit through `vac_session_engine::submit_one`
//!   with `VacEngineAdapter` so every task produces a durable
//!   transcript under `.vac/sessions/` and the same `SubmitEvent`
//!   stream `vac session-run` emits. Retires the dual-path problem
//!   described in `docs/completion-blueprint.md` ring 0.
//!
//! The flag / env exists so both paths can run side-by-side while
//! integration tests move over. Once every test passes on the session
//! path, the legacy arm becomes the one to delete.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;
use vac_core::{Priority, ProfileName, ProfileOverride};

pub async fn execute(
    project_root: PathBuf,
    task_description: String,
    priority: String,
    profile: String,
    approve: bool,
    targets: Vec<String>,
    budget_tokens: Option<u64>,
    backend: Option<String>,
) -> anyhow::Result<()> {
    if let Some(b) = backend {
        if b == "candle" {
            #[cfg(feature = "candle")]
            {
                use vil_inference::engine::InferenceBackend;
                use vil_inference::engine::InferenceRequest;
                let candle_backend = vil_inference::backends::CandleBackend::new_cpu();
                // We need to load a model. For the integration test, we can use VAC_CANDLE_TEST_MODEL
                // or just load from a default path. The test will probably set VAC_CANDLE_TEST_MODEL.
                let model_dir = std::env::var_os("VAC_CANDLE_TEST_MODEL")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."));

                candle_backend.load(&model_dir).await?;
                let request = InferenceRequest::new(&task_description, 10);
                let out = candle_backend.infer(&request).await?;
                println!("{}", out);
                return Ok(());
            }
            #[cfg(not(feature = "candle"))]
            {
                anyhow::bail!("Candle backend requires the 'candle' feature");
            }
        } else {
            anyhow::bail!("Unsupported backend: {}", b);
        }
    }

    let mut engine = vac_core::VacEngine::new(project_root.clone()).await?;

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

                    if let Ok(input) = crate::io::read_line_async().await {
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

    // Drive the task through `vac_session_engine::submit_one`
    // with a `VacEngineAdapter` wrapping the real `VacEngine`.
    let result = run_via_session_engine(
        project_root.clone(),
        engine,
        &task_description,
        update_tx,
        budget_tokens,
    )
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

/// Audit C1 fix — the CLI `vac run` path now delegates to the
/// single shared `vac_tui_runtime::runner::engine_adapter::
/// run_via_session_engine_with_broadcast` instead of carrying a
/// parallel implementation that passed `CompactConfig::default()`
/// (no gate, no dispatcher, no agent_dispatcher). Before this,
/// `vac run` could not execute any tool call — it hit
/// `UnsupportedDispatcher` silently. Now CLI and TUI share the
/// same VIL single-spine submit path with HookGate + live tool
/// dispatcher + agent dispatcher uniformly wired.
async fn run_via_session_engine(
    project_root: PathBuf,
    engine: vac_core::VacEngine,
    task_description: &str,
    update_tx: tokio::sync::mpsc::UnboundedSender<vac_core::engine::RuntimeUpdate>,
    budget_tokens: Option<u64>,
) -> anyhow::Result<vac_core::TaskResult> {
    let engine_arc = Arc::new(Mutex::new(engine));

    // Audit P0.3 — auto-wire teleport bridge when
    // `VAC_TELEPORT_BIND` env is set. A single env variable turns
    // `vac run` into a live remote-attachable session; clients
    // connect via `vac teleport --attach <token>` and see the
    // same SubmitChunks the local terminal is streaming. Product
    // closure for the teleport remote story — no separate
    // `vac teleport --serve` process needed.
    let teleport_broadcast = if std::env::var("VAC_TELEPORT_BIND").is_ok() {
        Some(super::teleport::start_live_teleport_bridge(&project_root).await?)
    } else {
        None
    };

    vac_tui_runtime::runner::engine_adapter::run_via_session_engine_with_broadcast(
        project_root,
        engine_arc,
        task_description,
        update_tx,
        teleport_broadcast,
        budget_tokens,
    )
    .await
    .map(|r| {
        // Preserve the historical unused-warning silence pattern —
        // future rings may read tool-call history off the engine
        // after the submit completes.
        r
    })
}

// Audit C1 fix: `submit_event_to_runtime_update` removed — the
// shared TUI entry point now owns the SubmitChunk →
// RuntimeUpdate translation (via `chunk_to_runtime_update`) so
// the two drivers cannot drift.
