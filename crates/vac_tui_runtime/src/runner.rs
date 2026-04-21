//! TUI Runner - Entry point for VAC TUI

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use vac_core::RuntimeUpdate;
use vac_core::engine::VacEngine;

use super::{InputEvent, OutputEvent, run_tui};

/// Shared handle to the active task's update channel for structured approval routing.
type ActiveUpdateTx = Arc<Mutex<Option<mpsc::UnboundedSender<RuntimeUpdate>>>>;

mod backend;
mod bundle_tasks;
mod message_tasks;
mod profile_tasks;
mod runtime_tasks;
mod session_tasks;
mod shell_dispatch;
mod startup;
mod vil_tasks;

#[cfg(test)]
#[path = "runner/tests.rs"]
mod tests;

use self::runtime_tasks::{
    handle_cancel_runtime_job, handle_retry_runtime_job, load_agent_state, load_agent_tasks,
    load_runtime_jobs, load_runtime_state,
};
use self::session_tasks::{
    handle_cleanup_session, handle_list_sessions, handle_load_session_resume_list,
    handle_new_session, resume_session_into_tui,
};

/// Input/output recording mode for a TUI session (PR-T18 wiring).
///
/// Set at most one of `record_dir` / `replay_file`:
/// - `record_dir`: append a JSONL recording of user inputs to this directory
///   so the session can be replayed later.
/// - `replay_file`: ignore the real terminal and drive the event loop from
///   this file, synthesising crossterm events through the live mapper.
#[derive(Debug, Clone, Default)]
pub struct TuiIoMode {
    pub record_dir: Option<PathBuf>,
    pub replay_file: Option<PathBuf>,
}

/// Run the VAC TUI with VacEngine integration
pub async fn run_vac_tui(project_root: PathBuf, resume: bool) -> Result<()> {
    run_vac_tui_with_io(project_root, resume, TuiIoMode::default()).await
}

/// Run the VAC TUI with explicit input recording/replay configuration.
pub async fn run_vac_tui_with_io(
    project_root: PathBuf,
    resume: bool,
    io_mode: TuiIoMode,
) -> Result<()> {
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
            let (style, severity) = startup::classify_init_warning(&warning);
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
                    message_tasks::handle_user_message(
                        engine_clone.clone(),
                        input_tx_clone.clone(),
                        active_update_tx_clone.clone(),
                        msg,
                        parts,
                    )
                    .await;
                }
                OutputEvent::AcceptTool(tc) => {
                    message_tasks::handle_accept_tool(&approvals, tc).await;
                }
                OutputEvent::RejectTool(tc, _, reason) => {
                    message_tasks::handle_reject_tool(&approvals, tc, reason).await;
                }
                OutputEvent::SwitchToModel(model) => {
                    profile_tasks::handle_switch_to_model(
                        engine_clone.clone(),
                        input_tx_clone.clone(),
                        model,
                    )
                    .await;
                }
                OutputEvent::SwitchProfile(profile_name) => {
                    profile_tasks::handle_switch_profile(
                        engine_clone.clone(),
                        runtime_project_root.clone(),
                        input_tx_clone.clone(),
                        profile_name,
                    );
                }
                OutputEvent::ApplyRulebooks(selected_ids) => {
                    profile_tasks::handle_apply_rulebooks(
                        engine_clone.clone(),
                        runtime_project_root.clone(),
                        input_tx_clone.clone(),
                        selected_ids,
                    );
                }
                OutputEvent::InvokeVilTool(tool_name, args) => {
                    vil_tasks::handle_invoke_vil_tool(
                        engine_clone.clone(),
                        input_tx_clone.clone(),
                        tool_name,
                        args,
                    );
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
                    bundle_tasks::handle_export_bundle(
                        runtime_project_root.clone(),
                        input_tx_clone.clone(),
                        path,
                    );
                }
                OutputEvent::ImportBundle(path) => {
                    bundle_tasks::handle_import_bundle(
                        runtime_project_root.clone(),
                        input_tx_clone.clone(),
                        path,
                    );
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

    // Run environment startup checks (VIL knowledge root, MCP servers)
    startup::run_startup_checks(&project_root, &input_tx).await;

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
        io_mode,
    )
    .await?;

    Ok(())
}
