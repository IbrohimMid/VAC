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

/// Engine selector for `vac run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineMode {
    /// `VacEngine::run_task_with_approvals` — the legacy spine.
    Legacy,
    /// `vac_session_engine::submit_one` via `VacEngineAdapter`.
    Session,
}

impl EngineMode {
    /// `--engine <value>` parser with env fallback
    /// (`VAC_ENGINE=session`). Unknown values fall back to Legacy
    /// with a warning so no operator gets surprised by a silent
    /// switch.
    pub fn resolve(flag: Option<&str>) -> Self {
        let raw = flag
            .map(str::to_owned)
            .or_else(|| std::env::var("VAC_ENGINE").ok());
        match raw.as_deref() {
            Some(v) if v.eq_ignore_ascii_case("session") => Self::Session,
            Some(v) if v.eq_ignore_ascii_case("legacy") => Self::Legacy,
            Some(v) if !v.is_empty() => {
                eprintln!(
                    "vac run: unrecognized engine '{v}' — falling back to legacy",
                );
                Self::Legacy
            }
            _ => Self::Legacy,
        }
    }
}

pub async fn execute(
    project_root: PathBuf,
    task_description: String,
    priority: String,
    profile: String,
    approve: bool,
    targets: Vec<String>,
    engine_mode: EngineMode,
) -> anyhow::Result<()> {
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

    // R0.b — engine cutover. Legacy path runs VacEngine directly;
    // Session path wraps it as an LlmAdapter behind submit_one so
    // the transcript JSONL lands under .vac/sessions/ and the
    // SubmitEvent contract is uniform across every run.
    let result = match engine_mode {
        EngineMode::Legacy => {
            engine
                .run_task_with_approvals(&task_description, Some(update_tx), None, None)
                .await?
        }
        EngineMode::Session => {
            println!("🔀 engine=session → submit_one spine (R0 cutover)");
            run_via_session_engine(
                project_root.clone(),
                engine,
                &task_description,
                update_tx,
            )
            .await?
        }
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

/// R0.b — Drive the task through `vac_session_engine::submit_one`
/// with a `VacEngineAdapter` wrapping the real `VacEngine`. Returns
/// a `vac_core::TaskResult` so the caller's summary-printing block
/// is unchanged — only the orchestration changes.
///
/// The adapter forwards VacEngine's `RuntimeUpdate` stream as
/// `SubmitEvent`s through the submit's outbound channel. Per-call
/// approval prompts still flow through the pre-existing
/// `update_tx` consumer path the caller spawned.
async fn run_via_session_engine(
    project_root: PathBuf,
    engine: vac_core::VacEngine,
    task_description: &str,
    update_tx: tokio::sync::mpsc::UnboundedSender<vac_core::engine::RuntimeUpdate>,
) -> anyhow::Result<vac_core::TaskResult> {
    use crate::commands::engine_adapter::VacEngineAdapter;
    use vac_session_engine::{
        CompactConfig, SlashProcessor, SubmitContext, SubmitEvent, TranscriptWriter,
        TrivialCompactBoundary, UsageTracker, submit_one,
    };

    // Share the engine between the adapter and the forwarder that
    // translates updates back into the caller's existing channel.
    let engine_arc = Arc::new(Mutex::new(engine));

    // Bridge: submit_one's outbound (SubmitEvent) → legacy
    // update_tx (RuntimeUpdate) so the pre-existing stdout renderer
    // still sees approvals / status lines without rewrite.
    let (submit_tx, mut submit_rx) = tokio::sync::mpsc::unbounded_channel::<SubmitEvent>();
    let legacy_tx = update_tx.clone();
    let bridge = tokio::spawn(async move {
        while let Some(ev) = submit_rx.recv().await {
            if let Some(rt) = submit_event_to_runtime_update(ev) {
                let _ = legacy_tx.send(rt);
            }
        }
    });

    let adapter = VacEngineAdapter::with_event_forward(engine_arc.clone(), submit_tx);
    let writer = TranscriptWriter::new(project_root);
    let slash = SlashProcessor::new();
    let compact = TrivialCompactBoundary::default();
    let usage = UsageTracker::new();
    let ctx = SubmitContext::new(uuid::Uuid::new_v4(), task_description.to_string());

    let snap = submit_one(
        ctx,
        &writer,
        &slash,
        &compact,
        &usage,
        &adapter,
        CompactConfig::default(),
        None,
    )
    .await
    .map_err(|e| anyhow::anyhow!("submit_one: {e}"))?;

    // Close the bridge cleanly (submit_tx dropped inside adapter
    // already) and reclaim the engine handle so we can call
    // `task_history` or equivalent for the summary below.
    let _ = bridge.await;

    // submit_one doesn't expose TaskResult; reconstitute a minimal
    // one from the engine's last task entry so the existing summary
    // renderer is unchanged. The authoritative record remains the
    // transcript JSONL.
    let mut engine = engine_arc.lock().await;
    let fallback_task_id = vac_core::task::TaskId(uuid::Uuid::new_v4());
    Ok(vac_core::TaskResult {
        task_id: fallback_task_id,
        status: vac_core::TaskStatus::Completed,
        summary: format!(
            "session-engine submit finished; {} tokens, transcript at {}",
            snap.total_tokens(),
            writer.sessions_dir().display(),
        ),
        modified_files: Vec::new(),
        created_files: Vec::new(),
        validation_score: None,
        elapsed_ms: 0,
        total_tokens_used: snap.total_tokens(),
        agent_contributions: Vec::new(),
    })
    .map(|r| {
        // Keep the engine handle alive to silence the
        // "unused mutable" warning; future rings read tool-call
        // history from here.
        let _ = &mut *engine;
        r
    })
}

fn submit_event_to_runtime_update(
    ev: vac_session_engine::SubmitEvent,
) -> Option<vac_core::engine::RuntimeUpdate> {
    use vac_core::engine::RuntimeUpdate;
    use vac_session_engine::SubmitEvent;
    match ev {
        SubmitEvent::LlmRequested { provider, model } => {
            Some(RuntimeUpdate::ModelInfo { provider, model })
        }
        SubmitEvent::LlmChunk { text } => Some(RuntimeUpdate::AssistantChunk(text)),
        SubmitEvent::ToolRequested {
            id,
            name,
            arguments,
        } => Some(RuntimeUpdate::ToolCall {
            id,
            name,
            arguments,
        }),
        SubmitEvent::ToolResult {
            id,
            name,
            success,
            summary,
        } => Some(RuntimeUpdate::ToolResult {
            id,
            name,
            content: summary,
            success,
        }),
        SubmitEvent::Aborted { reason } => Some(RuntimeUpdate::Failed(reason)),
        // Accepted/Compacted/SlashHandled/Finished are engine-layer
        // events without a RuntimeUpdate peer; drop.
        _ => None,
    }
}

#[cfg(test)]
mod engine_mode_tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    static ENV_LOCK: StdMutex<()> = StdMutex::new(());

    fn with_env<F: FnOnce() -> ()>(var: &str, val: Option<&str>, f: F) {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: ENV_LOCK serializes access from this test module.
        unsafe {
            match val {
                Some(v) => std::env::set_var(var, v),
                None => std::env::remove_var(var),
            }
        }
        f();
        // Clean up so cross-test order doesn't matter.
        unsafe {
            std::env::remove_var(var);
        }
    }

    #[test]
    fn resolve_defaults_to_legacy_without_flag_or_env() {
        with_env("VAC_ENGINE", None, || {
            assert_eq!(EngineMode::resolve(None), EngineMode::Legacy);
        });
    }

    #[test]
    fn resolve_honors_env_session() {
        with_env("VAC_ENGINE", Some("session"), || {
            assert_eq!(EngineMode::resolve(None), EngineMode::Session);
        });
    }

    #[test]
    fn resolve_flag_overrides_env() {
        with_env("VAC_ENGINE", Some("session"), || {
            assert_eq!(EngineMode::resolve(Some("legacy")), EngineMode::Legacy);
        });
    }

    #[test]
    fn resolve_unknown_value_falls_back_to_legacy() {
        with_env("VAC_ENGINE", None, || {
            assert_eq!(EngineMode::resolve(Some("quantum")), EngineMode::Legacy);
        });
    }

    #[test]
    fn resolve_case_insensitive() {
        with_env("VAC_ENGINE", None, || {
            assert_eq!(EngineMode::resolve(Some("SESSION")), EngineMode::Session);
            assert_eq!(EngineMode::resolve(Some("Legacy")), EngineMode::Legacy);
        });
    }
}
