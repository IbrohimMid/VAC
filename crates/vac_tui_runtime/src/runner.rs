//! TUI Runner - Entry point for VAC TUI

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use vac_core::RuntimeUpdate;
use vac_core::engine::VacEngine;

use super::{
    FunctionCall, InputEvent, LoadingOperation, OutputEvent, ToolCall, ToolCallResult,
    ToolCallResultStatus, run_tui,
};

/// Shared handle to the active task's update channel for structured approval routing.
type ActiveUpdateTx = Arc<Mutex<Option<mpsc::UnboundedSender<RuntimeUpdate>>>>;

mod runtime_tasks;
mod backend;
mod session_tasks;
mod shell_dispatch;
use self::session_tasks::{
    resume_session_into_tui, handle_list_sessions, handle_load_session_resume_list,
    handle_new_session, handle_cleanup_session,
};
use self::backend::{resolve_tool_approval, handle_runtime_update};
use self::runtime_tasks::{
    load_runtime_jobs, load_runtime_state, load_agent_tasks, load_agent_state,
    handle_cancel_runtime_job, handle_retry_runtime_job,
};

fn classify_init_warning(
    warning: &str,
) -> (
    crate::services::banner::BannerStyle,
    crate::services::banner::BannerSeverity,
) {
    let lower = warning.to_ascii_lowercase();
    if lower.contains("failed to connect mcp server") || lower.contains("mcp server") {
        let tls_related = lower.contains("tls")
            || lower.contains("certificate")
            || lower.contains("ca file")
            || lower.contains("mtls")
            || lower.contains("server_name")
            || lower.contains("identity");
        if tls_related {
            (
                crate::services::banner::BannerStyle::Error,
                crate::services::banner::BannerSeverity::Blocking,
            )
        } else {
            (
                crate::services::banner::BannerStyle::Warning,
                crate::services::banner::BannerSeverity::Suggested,
            )
        }
    } else {
        (
            crate::services::banner::BannerStyle::Warning,
            crate::services::banner::BannerSeverity::Suggested,
        )
    }
}


/// Run the VAC TUI with VacEngine integration
pub async fn run_vac_tui(project_root: PathBuf, resume: bool) -> Result<()> {
    // Initialize VacEngine
    let mut engine = VacEngine::new(project_root.clone()).await?;

    // Initialize engine (load tools, policies, etc.)
    let warnings = engine.init().await?;

    let approvals = engine.approval_handle();
    let session_id = engine.session_id().await.to_string();

    let engine = Arc::new(Mutex::new(engine));

    // Create channels
    let (input_tx, input_rx) = mpsc::channel::<InputEvent>(100);

    // Inject warnings into the TUI banner queue
    let input_tx_clone_for_warnings = input_tx.clone();
    tokio::spawn(async move {
        for warning in warnings {
            let (style, severity) = classify_init_warning(&warning);
            let _ = input_tx_clone_for_warnings
                .send(InputEvent::ShowBanner(warning, style, severity))
                .await;
        }
    });

    let (output_tx, mut output_rx) = mpsc::channel::<OutputEvent>(100);
    let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

    // Shared handle to active task's update channel for structured approval routing
    let active_update_tx: ActiveUpdateTx = Arc::new(Mutex::new(None));

    // Spawn task to handle output events
    let engine_clone = engine.clone();
    let input_tx_clone = input_tx.clone();
    let active_update_tx_clone = active_update_tx.clone();
    let approvals = approvals.clone();
    let runtime_project_root = project_root.clone();
    tokio::spawn(async move {
        while let Some(event) = output_rx.recv().await {
            match event {
                OutputEvent::UserMessage(msg, _tools, parts, _usize) => {
                    let engine = engine_clone.clone();
                    let input_tx = input_tx_clone.clone();
                    let msg = msg.clone();
                    let active_tx = active_update_tx_clone.clone();

                    let _ = input_tx
                        .send(InputEvent::StartLoadingOperation(
                            LoadingOperation::LlmRequest,
                        ))
                        .await;

                    tokio::spawn(async move {
                        let (update_tx, mut update_rx) = mpsc::unbounded_channel::<RuntimeUpdate>();
                        let input_tx_inner = input_tx.clone();
                        let stream_uuid = uuid::Uuid::new_v4();

                        // Store active channels for structured approval routing
                        *active_tx.lock().await = Some(update_tx.clone());

                        tokio::spawn(async move {
                            let mut active_tools: HashMap<String, ToolCall> = HashMap::new();
                            while let Some(update) = update_rx.recv().await {
                                handle_runtime_update(
                                    update,
                                    &input_tx_inner,
                                    stream_uuid,
                                    &mut active_tools,
                                )
                                .await;
                            }
                        });

                        let mut eng = engine.lock().await;
                        // Convert TUI ContentParts to LLM ImageParts for multimodal
                        // Generic data-URL parsing: "data:<media_type>;base64,<data>"
                        let image_parts: Vec<vil_llm::provider::ImagePart> = parts
                            .iter()
                            .filter_map(|p| {
                                p.image_url.as_ref().and_then(|url| {
                                    let raw = &url.url;
                                    let after_data = raw.strip_prefix("data:")?;
                                    let (meta, data) = after_data.split_once(";base64,")?;
                                    Some(vil_llm::provider::ImagePart {
                                        source_type: "base64".to_string(),
                                        media_type: meta.to_string(),
                                        data: data.to_string(),
                                    })
                                })
                            })
                            .collect();

                        if image_parts.is_empty() {
                            let _ = eng
                                .run_task_with_approvals(&msg, Some(update_tx), None, None)
                                .await;
                        } else {
                            let _ = eng
                                .run_task_with_images(
                                    &msg,
                                    Some(update_tx),
                                    None,
                                    None,
                                    image_parts,
                                )
                                .await;
                        }

                        // Clear active channels when task completes
                        *active_tx.lock().await = None;
                    });
                }
                OutputEvent::AcceptTool(tc) => {
                    if let Err(e) =
                        resolve_tool_approval(&approvals, tc.id.clone(), true, None).await
                    {
                        log::error!("Failed to approve tool call {}: {}", tc.id, e);
                    }
                }
                OutputEvent::RejectTool(tc, _, reason) => {
                    if let Err(e) =
                        resolve_tool_approval(&approvals, tc.id.clone(), false, reason).await
                    {
                        log::error!("Failed to reject tool call {}: {}", tc.id, e);
                    }
                }
                OutputEvent::SwitchToModel(model) => {
                    let mut eng = engine_clone.lock().await;
                    if eng.set_model_override(Some(model.id.clone())).await.is_ok() {
                        let _ = input_tx_clone
                            .send(InputEvent::ShowToast(crate::services::Toast::success(
                                format!("Model: {}", model.name),
                            )))
                            .await;
                        let _ = input_tx_clone
                            .send(InputEvent::SetCurrentModel(model))
                            .await;
                    }
                }
                OutputEvent::SwitchProfile(profile_name) => {
                    let engine = engine_clone.clone();
                    let project_root = runtime_project_root.clone();
                    let input_tx = input_tx_clone.clone();
                    tokio::spawn(async move {
                        // Set active profile on the swarm (thread-safe, no env var mutation)
                        {
                            let eng = engine.lock().await;
                            if let Some(swarm) = eng.swarm_mut() {
                                swarm.write().await.set_active_profile(profile_name.clone());
                            }
                        }

                        let profile_name_clone = profile_name.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            // Persist to .vac/config.toml
                            let vac_dir = project_root.join(".vac");
                            let _ = std::fs::create_dir_all(&vac_dir);
                            let config_path = vac_dir.join("config.toml");
                            let content = std::fs::read_to_string(&config_path).unwrap_or_default();
                            let mut table: toml::Table = content.parse().unwrap_or_default();
                            table
                                .entry("profile".to_string())
                                .or_insert_with(|| toml::Value::Table(toml::Table::new()))
                                .as_table_mut()
                                .map(|t| {
                                    t.insert(
                                        "active".to_string(),
                                        toml::Value::String(profile_name_clone),
                                    )
                                });
                            let _ = std::fs::write(&config_path, table.to_string());
                        })
                        .await;

                        let _ = input_tx
                            .send(InputEvent::ShowToast(crate::services::Toast::success(
                                format!("Profile: {}", profile_name),
                            )))
                            .await;
                    });
                }
                OutputEvent::ApplyRulebooks(selected_ids) => {
                    let engine = engine_clone.clone();
                    let project_root = runtime_project_root.clone();
                    let input_tx = input_tx_clone.clone();
                    tokio::spawn(async move {
                        let project_root_clone = project_root.clone();
                        let selected_ids_clone = selected_ids.clone();
                        // Load rulebooks and filter by selection
                        let filtered: Vec<_> = tokio::task::spawn_blocking(move || {
                            let config =
                                vac_core::VacConfig::load_with_fallback(&project_root_clone)
                                    .unwrap_or_default();
                            let all_books = vac_core::rulebook::RulebookLoader::load_all(
                                &project_root_clone,
                                &config.rulebook.paths,
                            );
                            if selected_ids_clone.is_empty() {
                                all_books
                            } else {
                                all_books
                                    .into_iter()
                                    .filter(|b| selected_ids_clone.contains(&b.id))
                                    .collect()
                            }
                        })
                        .await
                        .unwrap_or_default();

                        // Build resolved context with archetype from VIL project profile
                        let archetype_str = {
                            let profile =
                                vac_core::detector::VilProjectProfile::detect(&project_root);
                            let s = profile.archetype.to_string();
                            if s == "Unknown" { None } else { Some(s) }
                        };
                        let resolved = vac_core::rulebook::ResolvedRuleContext::build(
                            filtered,
                            archetype_str.as_deref(),
                        );
                        if let Some(overlay) = resolved.to_prompt_overlay() {
                            let eng = engine.lock().await;
                            if let Some(swarm) = eng.swarm_mut() {
                                swarm.write().await.set_rulebook(overlay);
                            }
                        }

                        let label = if selected_ids.is_empty() {
                            "all".to_string()
                        } else {
                            selected_ids.join(", ")
                        };
                        let _ = input_tx
                            .send(InputEvent::ShowToast(crate::services::Toast::success(
                                format!("Rulebooks: {}", label),
                            )))
                            .await;
                    });
                }
                OutputEvent::InvokeVilTool(tool_name, args) => {
                    let engine = engine_clone.clone();
                    let input_tx = input_tx_clone.clone();
                    tokio::spawn(async move {
                        let eng = engine.lock().await;
                        match eng.execute_tool_direct(&tool_name, args).await {
                            Ok(result) => {
                                let formatted = serde_json::to_string_pretty(&result)
                                    .unwrap_or_else(|_| result.to_string());
                                let content = format!(
                                    "**`{}`** result:\n\n```json\n{}\n```",
                                    tool_name, formatted
                                );
                                let _ = input_tx.send(InputEvent::AssistantMessage(content)).await;
                            }
                            Err(e) => {
                                let _ = input_tx
                                    .send(InputEvent::Error(format!(
                                        "Tool '{}' failed: {}",
                                        tool_name, e
                                    )))
                                    .await;
                            }
                        }
                    });
                }
                OutputEvent::ExecuteCommand(cmd, active_isolation_mode) => {
                    let input_tx = input_tx_clone.clone();
                    let (cols, rows) = crossterm::terminal::size().unwrap_or((120, 32));
                    let rows = rows.saturating_sub(8).max(8);
                    let cols = cols.saturating_sub(4).max(40);
                    let spec = shell_dispatch::resolve_shell_spec(
                        &cmd,
                        &active_isolation_mode,
                        &runtime_project_root,
                        &input_tx,
                    )
                    .await;
                    match spec {
                        shell_dispatch::ShellSpecOutcome::Skip => continue,
                        shell_dispatch::ShellSpecOutcome::Spec(shell_spec) => {
                            shell_dispatch::launch_pty(cmd, shell_spec, &input_tx, rows, cols)
                                .await;
                        }
                    }
                }
                OutputEvent::ExportBundle(path) => {
                    let input_tx = input_tx_clone.clone();
                    let project_root = runtime_project_root.clone();
                    tokio::spawn(async move {
                        let result = tokio::task::spawn_blocking(move || {
                            vac_core::bundle::export_bundle_to_path(
                                &project_root,
                                None,
                                Some(&path),
                                true,
                            )
                        })
                        .await
                        .unwrap_or_else(|e| {
                            Err(vac_core::VacError::Task(format!("join error: {e}")))
                        });
                        match result {
                            Ok(out_path) => {
                                let _ = input_tx
                                    .send(InputEvent::ShowToast(crate::services::Toast::success(
                                        format!("Bundle diekspor: {}", out_path.display()),
                                    )))
                                    .await;
                                let _ = input_tx
                                    .send(InputEvent::AssistantMessage(format!(
                                        "Bundle diekspor ke `{}`",
                                        out_path.display()
                                    )))
                                    .await;
                            }
                            Err(e) => {
                                let _ = input_tx
                                    .send(InputEvent::ShowToast(crate::services::Toast::error(
                                        format!("Gagal export bundle: {e}"),
                                    )))
                                    .await;
                            }
                        }
                    });
                }
                OutputEvent::ImportBundle(path) => {
                    let input_tx = input_tx_clone.clone();
                    let project_root = runtime_project_root.clone();
                    tokio::spawn(async move {
                        let path_for_msg = path.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            vac_core::bundle::import_bundle_from_path(&project_root, &path)
                        })
                        .await
                        .unwrap_or_else(|e| {
                            Err(vac_core::VacError::Task(format!("join error: {e}")))
                        });
                        match result {
                            Ok(session_id) => {
                                let _ = input_tx
                                    .send(InputEvent::ShowToast(crate::services::Toast::success(
                                        format!("Bundle diimpor (session_id={})", session_id),
                                    )))
                                    .await;
                                let _ = input_tx
                                    .send(InputEvent::AssistantMessage(format!(
                                        "Bundle diimpor dari `{}`",
                                        path_for_msg.display()
                                    )))
                                    .await;
                            }
                            Err(e) => {
                                let _ = input_tx
                                    .send(InputEvent::ShowToast(crate::services::Toast::error(
                                        format!("Gagal import bundle: {e}"),
                                    )))
                                    .await;
                            }
                        }
                    });
                }
                OutputEvent::ListSessions => {
                    handle_list_sessions(&engine_clone, &runtime_project_root, &input_tx_clone).await;
                }
                OutputEvent::ListRuntimeJobs => {
                    let jobs = load_runtime_jobs(&runtime_project_root).await;
                    let _ = input_tx_clone.send(InputEvent::SetRuntimeJobs(jobs)).await;
                }
                OutputEvent::LoadSessionResumeList => {
                    handle_load_session_resume_list(&runtime_project_root, &input_tx_clone).await;
                }
                OutputEvent::ListAgentTasks => {
                    let tasks = load_agent_tasks(&runtime_project_root).await;
                    let _ = input_tx_clone.send(InputEvent::SetAgentTasks(tasks)).await;
                }
                OutputEvent::LoadRuntimeState => {
                    let snapshot = load_runtime_state(&runtime_project_root).await;
                    let _ = input_tx_clone
                        .send(InputEvent::SetRuntimeState(snapshot))
                        .await;
                }
                OutputEvent::LoadAgentState => {
                    let snapshot = load_agent_state(&runtime_project_root).await;
                    let _ = input_tx_clone
                        .send(InputEvent::SetAgentState(snapshot))
                        .await;
                }
                OutputEvent::CancelRuntimeJob(id) => {
                    handle_cancel_runtime_job(&runtime_project_root, &input_tx_clone, id).await;
                }
                OutputEvent::RetryRuntimeJob(id) => {
                    handle_retry_runtime_job(&runtime_project_root, &input_tx_clone, id).await;
                }
                OutputEvent::NewSession => {
                    handle_new_session(&engine_clone, &input_tx_clone).await;
                }
                OutputEvent::ResumeSession(id) => {
                    resume_session_into_tui(
                        engine_clone.clone(),
                        active_update_tx_clone.clone(),
                        input_tx_clone.clone(),
                        id,
                    )
                    .await;
                }
                OutputEvent::SwitchToSession(id) => {
                    resume_session_into_tui(
                        engine_clone.clone(),
                        active_update_tx_clone.clone(),
                        input_tx_clone.clone(),
                        id,
                    )
                    .await;
                }
                OutputEvent::CleanupSession(id) => {
                    handle_cleanup_session(&runtime_project_root, &input_tx_clone, id).await;
                    // Refresh session list after cleanup
                    handle_list_sessions(&engine_clone, &runtime_project_root, &input_tx_clone).await;
                }
                _ => {}
            }
        }
    });

    // Handle session restore if requested
    if resume {
        if let Ok(Some(session)) = vac_core::session::Session::load_latest(&project_root) {
            let _ = output_tx
                .send(OutputEvent::ResumeSession(session.id.to_string()))
                .await;
        }
    }

    // Periodic checkpoint write (every 30s)
    let engine_checkpoint = engine.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let eng = engine_checkpoint.lock().await;
            let session = eng.session().read().await;
            if let Err(e) = session.save() {
                log::error!("Failed to save session checkpoint: {}", e);
            }
        }
    });

    // Run TUI
    {
        let eng = engine.lock().await;
        let models = eng
            .available_models()
            .into_iter()
            .map(|(provider, model)| {
                let is_reasoning = model.contains("o1")
                    || model.contains("o3")
                    || model.contains("r1")
                    || model.contains("deepseek");
                let is_streaming = !is_reasoning;
                let context_window = if model.contains("opus")
                    || model.contains("sonnet")
                    || model.contains("gemini")
                {
                    200000
                } else {
                    128000
                };
                let cost_class =
                    if model.contains("opus") || model.contains("o1") || model.contains("r1") {
                        "premium".to_string()
                    } else if model.contains("haiku")
                        || model.contains("mini")
                        || model.contains("flash")
                    {
                        "cheap".to_string()
                    } else {
                        "standard".to_string()
                    };
                crate::Model {
                    id: model.clone(),
                    name: model,
                    provider,
                    supports_reasoning: is_reasoning,
                    supports_tool_calls: true,
                    supports_streaming: is_streaming,
                    context_window,
                    cost_class,
                }
            })
            .collect::<Vec<_>>();
        let _ = input_tx
            .send(InputEvent::AvailableModelsLoaded(models.clone()))
            .await;

        // Phase 3: Send StartupHydrated with real runtime state
        let active_model_name = models.first().map(|m| m.name.clone());
        let default_model_id = models.first().map(|m| m.id.clone());
        let session_count = eng.list_sessions().await.map(|s| s.len()).unwrap_or(0);
        let status = eng.status().await.ok();
        let provider_status = if status.as_ref().is_some_and(|s| s.subsystems_initialized) {
            "ready".to_string()
        } else {
            "loading...".to_string()
        };

        let project_root_clone = project_root.clone();
        let (config, selected_rulebooks) = tokio::task::spawn_blocking(move || {
            let config =
                vac_core::VacConfig::load_with_fallback(&project_root_clone).unwrap_or_default();
            let books = vac_core::rulebook::RulebookLoader::load_all(
                &project_root_clone,
                &config.rulebook.paths,
            );
            let selected_rulebooks: Vec<String> = books.iter().map(|b| b.id.clone()).collect();
            (config, selected_rulebooks)
        })
        .await
        .unwrap_or_default();
        let mcp_server_count = config.mcp_servers.as_ref().map_or(0, |s| s.len());
        let active_rulebook = if selected_rulebooks.is_empty() {
            None
        } else {
            Some(selected_rulebooks.join(", "))
        };

        let startup_snapshot = crate::app::StartupSnapshot {
            boot_time: chrono::Utc::now(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            has_vil_engine: vac_core::detector::VilProjectProfile::detect(&project_root)
                .is_vil_project,
            active_rulebook,
            environment: if cfg!(debug_assertions) {
                "development".to_string()
            } else {
                "production".to_string()
            },
            active_model: active_model_name,
            default_model: default_model_id,
            active_profile: Some("default".to_string()),
            selected_rulebooks,
            mcp_server_count,
            session_count,
            pending_approvals_count: 0,
            provider_status,
            queue_depth: 0,
        };
        let _ = input_tx
            .send(InputEvent::StartupHydrated(startup_snapshot))
            .await;
    }

    // Run environment startup checks
    fn read_toml_str(path: &std::path::Path, keys: &[&str]) -> Option<String> {
        let content = std::fs::read_to_string(path).ok()?;
        let table = content.parse::<toml::Table>().ok()?;
        let mut current = &toml::Value::Table(table);
        for key in keys {
            current = current.get(key)?;
        }
        current.as_str().map(String::from)
    }

    let corpus_root = if let Ok(env_root) = std::env::var("VIL_KNOWLEDGE_ROOT") {
        let p = std::path::PathBuf::from(env_root);
        if p.exists() { Some(p) } else { None }
    } else {
        let config_path = project_root.join(".vac/config.toml");
        read_toml_str(&config_path, &["knowledge", "root"])
            .map(std::path::PathBuf::from)
            .filter(|p| p.exists())
    };

    if corpus_root.is_none() {
        let _ = input_tx
            .send(InputEvent::ShowToast(crate::services::Toast::info(
                "VIL Knowledge missing. Using bootstrap fallback.".to_string(),
            )))
            .await;
    }

    let project_root_clone = project_root.clone();
    let config = tokio::task::spawn_blocking(move || {
        vac_core::VacConfig::load_with_fallback(&project_root_clone).unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    if config.mcp_servers.as_ref().is_none_or(|s| s.is_empty()) {
        let _ = input_tx
            .send(InputEvent::ShowToast(crate::services::Toast::info(
                "No MCP servers configured.".to_string(),
            )))
            .await;
    }

    run_tui(
        input_rx,
        output_tx.clone(),
        None,
        shutdown_tx,
        None,
        false,
        false,
        true,
        None,
        None,
        "default".to_string(),
        None,
        None,
        Some(session_id),
        None,
        (None, None, None),
        None,
        false,
        vec![],
        None,
        project_root,
    )
    .await?;

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    #[tokio::test]
    async fn switch_to_session_invalid_uuid_emits_error() {
        let dir = tempfile::tempdir().unwrap();
        let engine = VacEngine::new(dir.path().to_path_buf()).await.unwrap();
        let engine = Arc::new(Mutex::new(engine));
        let active_update_tx: ActiveUpdateTx = Arc::new(Mutex::new(None));

        let (input_tx, mut input_rx) = mpsc::channel::<InputEvent>(8);
        resume_session_into_tui(engine, active_update_tx, input_tx, "not-a-uuid".to_string()).await;

        let ev = timeout(std::time::Duration::from_secs(1), input_rx.recv())
            .await
            .unwrap()
            .unwrap();
        match ev {
            InputEvent::Error(msg) => assert!(msg.contains("Invalid session id")),
            _ => panic!("unexpected event"),
        }
    }

    #[tokio::test]
    async fn switch_to_session_emits_session_restored() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let mut engine = VacEngine::new(root.clone()).await.unwrap();
        engine.init().await.unwrap();
        let engine = Arc::new(Mutex::new(engine));
        let active_update_tx: ActiveUpdateTx = Arc::new(Mutex::new(None));

        let session = vac_core::session::Session::new(root.clone());
        let id = session.id.to_string();
        session.save().unwrap();

        let (input_tx, mut input_rx) = mpsc::channel::<InputEvent>(16);
        resume_session_into_tui(engine, active_update_tx, input_tx, id.clone()).await;

        let first = timeout(std::time::Duration::from_secs(1), input_rx.recv())
            .await
            .unwrap()
            .unwrap();
        match first {
            InputEvent::SessionRestored { id: got, .. } => assert_eq!(got, id),
            _ => panic!("unexpected first event"),
        }
    }
}
