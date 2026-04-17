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

async fn load_runtime_jobs(project_root: &std::path::Path) -> Vec<vac_runtime::Job> {
    vac_runtime::TaskQueue::with_storage(project_root.join(".vac/queue.json"))
        .list()
        .await
}

fn load_runtime_state(project_root: &std::path::Path) -> Option<vac_runtime::AutopilotStateFile> {
    let content = std::fs::read_to_string(project_root.join(".vac/autopilot.state")).ok()?;
    serde_json::from_str(&content).ok()
}

async fn resume_session_into_tui(
    engine: Arc<Mutex<VacEngine>>,
    active_update_tx: ActiveUpdateTx,
    input_tx: mpsc::Sender<InputEvent>,
    id: String,
) {
    let uuid = match uuid::Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(e) => {
            let _ = input_tx
                .send(InputEvent::Error(format!("Invalid session id: {e}")))
                .await;
            return;
        }
    };

    let project_root = {
        let eng = engine.lock().await;
        match eng.status().await {
            Ok(status) => status.project_root,
            Err(e) => {
                let _ = input_tx
                    .send(InputEvent::Error(format!(
                        "Failed to read engine status: {e}"
                    )))
                    .await;
                return;
            }
        }
    };

    let title = match vac_core::session::Session::load(&project_root, uuid) {
        Ok(Some(s)) => format!("Session {}", &s.id.to_string()[..8]),
        _ => format!("Session {}", &id[..id.len().min(8)]),
    };

    let _ = input_tx
        .send(InputEvent::SessionRestored {
            id: id.clone(),
            title,
            messages: vec![],
        })
        .await;

    tokio::spawn(async move {
        let (update_tx, mut update_rx) = tokio::sync::mpsc::unbounded_channel::<RuntimeUpdate>();
        let input_tx_inner = input_tx.clone();
        let stream_uuid = uuid::Uuid::new_v4();

        *active_update_tx.lock().await = Some(update_tx.clone());

        tokio::spawn(async move {
            let mut active_tools: HashMap<String, ToolCall> = HashMap::new();
            while let Some(update) = update_rx.recv().await {
                handle_runtime_update(update, &input_tx_inner, stream_uuid, &mut active_tools)
                    .await;
            }
        });

        let result = {
            let mut eng = engine.lock().await;
            eng.resume_run_state(uuid, Some(update_tx), None, None)
                .await
        };

        *active_update_tx.lock().await = None;

        if let Err(e) = result {
            let _ = input_tx
                .send(InputEvent::Error(format!("Failed to resume session: {e}")))
                .await;
        }
    });
}

async fn resolve_tool_approval(
    approvals: &vac_core::ApprovalHandle,
    tool_call_id: String,
    approved: bool,
    reason: Option<String>,
) -> Result<(), vac_core::VacError> {
    if approved {
        approvals.approve(tool_call_id).await
    } else {
        approvals.reject(tool_call_id, reason).await
    }
}

async fn handle_runtime_update(
    update: RuntimeUpdate,
    input_tx_inner: &mpsc::Sender<InputEvent>,
    stream_uuid: uuid::Uuid,
    active_tools: &mut HashMap<String, ToolCall>,
) {
    match update {
        RuntimeUpdate::Status(status_msg) => {
            let _ = input_tx_inner
                .send(InputEvent::StreamAssistantMessage(
                    stream_uuid,
                    format!("> {}\n", status_msg),
                ))
                .await;
        }
        RuntimeUpdate::ModelInfo { provider, model } => {
            let _ = input_tx_inner
                .send(InputEvent::SetCurrentModel(crate::tui::Model {
                    id: model.clone(),
                    name: model,
                    provider,
                    supports_reasoning: false,
                }))
                .await;
        }
        RuntimeUpdate::AssistantChunk(chunk) => {
            let _ = input_tx_inner
                .send(InputEvent::StreamAssistantMessage(stream_uuid, chunk))
                .await;
        }
        RuntimeUpdate::ToolCall {
            id,
            name,
            arguments,
        } => {
            let tool_call = ToolCall {
                id: id.clone(),
                r#type: "function".to_string(),
                function: FunctionCall {
                    name: name.clone(),
                    arguments: arguments.to_string(),
                },
                metadata: None,
            };
            active_tools.insert(id.clone(), tool_call.clone());
            let _ = input_tx_inner
                .send(InputEvent::RunToolCall(tool_call))
                .await;
        }
        RuntimeUpdate::ToolResult {
            id,
            name,
            content,
            success,
        } => {
            let status = if success {
                ToolCallResultStatus::Success
            } else {
                ToolCallResultStatus::Error
            };

            let tool_call = active_tools.remove(&id).unwrap_or_else(|| ToolCall {
                id: id.clone(),
                r#type: "function".to_string(),
                function: FunctionCall {
                    name,
                    arguments: "{}".to_string(),
                },
                metadata: None,
            });

            let result_preview = if content.len() > 100 {
                format!("{}...", &content[..100])
            } else {
                content.clone()
            };

            let tool_result = ToolCallResult {
                call: tool_call,
                result: result_preview,
                status,
            };
            let _ = input_tx_inner
                .send(InputEvent::ToolResult(tool_result))
                .await;
        }
        RuntimeUpdate::Completed(result) => {
            let _ = input_tx_inner
                .send(InputEvent::EndLoadingOperation(
                    LoadingOperation::LlmRequest,
                ))
                .await;
            let _ = input_tx_inner.send(InputEvent::TaskCompleted(result)).await;
        }
        RuntimeUpdate::Failed(err) => {
            let _ = input_tx_inner
                .send(InputEvent::EndLoadingOperation(
                    LoadingOperation::LlmRequest,
                ))
                .await;
            let _ = input_tx_inner.send(InputEvent::Error(err)).await;
        }
        RuntimeUpdate::ApprovalRequired {
            tool_call_id,
            tool_name,
            arguments,
            explanation,
        } => {
            let tool_call = ToolCall {
                id: tool_call_id,
                r#type: "function".to_string(),
                function: FunctionCall {
                    name: tool_name,
                    arguments: arguments.to_string(),
                },
                metadata: None,
            };
            let _ = input_tx_inner
                .send(InputEvent::ShowConfirmationDialogWithExplanation(
                    tool_call,
                    explanation,
                ))
                .await;
        }
        _ => {}
    }
}

/// Run the VAC TUI with VacEngine integration
pub async fn run_vac_tui(project_root: PathBuf, resume: bool) -> Result<()> {
    // Initialize VacEngine
    let mut engine = VacEngine::new(project_root.clone()).await?;

    // Initialize engine (load tools, policies, etc.)
    engine.init().await?;
    let approvals = engine.approval_handle();
    let session_id = engine.session_id().await.to_string();

    let engine = Arc::new(Mutex::new(engine));

    // Create channels
    let (input_tx, input_rx) = mpsc::channel::<InputEvent>(100);
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
                            .send(InputEvent::ShowToast(crate::tui::services::Toast::success(
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

                        // Persist to .vac/config.toml
                        let vac_dir = project_root.join(".vac");
                        let _ = std::fs::create_dir_all(&vac_dir);
                        let config_path = vac_dir.join("config.toml");
                        let content = std::fs::read_to_string(&config_path).unwrap_or_default();
                        let mut table: toml::Table = content.parse().unwrap_or_default();
                        table
                            .entry("profile")
                            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
                            .as_table_mut()
                            .map(|t| {
                                t.insert(
                                    "active".to_string(),
                                    toml::Value::String(profile_name.clone()),
                                )
                            });
                        let _ = std::fs::write(&config_path, table.to_string());

                        let _ = input_tx
                            .send(InputEvent::ShowToast(crate::tui::services::Toast::success(
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
                        // Load rulebooks and filter by selection
                        let config = vac_core::VacConfig::load_with_fallback(&project_root)
                            .unwrap_or_default();
                        let all_books = vac_core::rulebook::RulebookLoader::load_all(
                            &project_root,
                            &config.rulebook.paths,
                        );
                        let filtered: Vec<_> = if selected_ids.is_empty() {
                            all_books
                        } else {
                            all_books
                                .into_iter()
                                .filter(|b| selected_ids.contains(&b.id))
                                .collect()
                        };

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
                            .send(InputEvent::ShowToast(crate::tui::services::Toast::success(
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
                    let shell_spec = match vac_core::VacConfig::load_with_fallback(
                        &runtime_project_root,
                    ) {
                        Ok(mut config) => {
                            // Auto-detect interactive vs batch
                            let is_interactive = cmd.is_empty();

                            let mut env_mode_str = active_isolation_mode.as_str();
                            if active_isolation_mode.starts_with("isolated") {
                                if is_interactive {
                                    env_mode_str = "isolated_interactive";
                                } else {
                                    env_mode_str = "isolated_batch";
                                }
                            }

                            if let Ok(env_mode) =
                                serde_json::from_str::<vac_core::ExecutionEnvironment>(&format!(
                                    "\"{}\"",
                                    env_mode_str
                                ))
                            {
                                config.runtime.execution_environment = env_mode;
                            }

                            // Handle default mount presets
                            if active_isolation_mode.contains("(Rust)") {
                                config
                                    .runtime
                                    .mount_presets
                                    .push(vac_core::config::MountPreset::Rust);
                            } else if active_isolation_mode.contains("(Node)") {
                                config
                                    .runtime
                                    .mount_presets
                                    .push(vac_core::config::MountPreset::Node);
                            } else if active_isolation_mode.contains("(Python)") {
                                config
                                    .runtime
                                    .mount_presets
                                    .push(vac_core::config::MountPreset::Python);
                            }

                            if config.runtime.execution_environment
                                == vac_core::ExecutionEnvironment::IsolatedInteractive
                                || config.runtime.execution_environment
                                    == vac_core::ExecutionEnvironment::IsolatedBatch
                            {
                                let isolation = vac_runtime::IsolationManager::new(
                                    runtime_project_root.clone(),
                                    config.runtime.clone(),
                                );

                                if is_interactive {
                                    match isolation.build_interactive_shell_spec() {
                                        Ok(spec) => Some(spec),
                                        Err(err) => {
                                            let _ = input_tx
                                                .send(InputEvent::ShellError(
                                                    "system".to_string(),
                                                    format!(
                                                        "Failed to prepare isolated shell: {err}"
                                                    ),
                                                ))
                                                .await;
                                            continue;
                                        }
                                    }
                                } else {
                                    let env = std::collections::HashMap::new();
                                    match isolation.build_container_command(
                                        std::path::Path::new("sh"),
                                        &["-c".to_string(), cmd.clone()],
                                        true,
                                        &env,
                                    ) {
                                        Ok(command) => {
                                            let program =
                                                command.get_program().to_string_lossy().to_string();
                                            let args = command
                                                .get_args()
                                                .map(|a| a.to_string_lossy().to_string())
                                                .collect();
                                            Some(vac_runtime::IsolationLaunchSpec {
                                                program,
                                                args,
                                                cwd: runtime_project_root.clone(),
                                                env,
                                            })
                                        }
                                        Err(err) => {
                                            let _ = input_tx
                                                .send(InputEvent::ShellError(
                                                    "system".to_string(),
                                                    format!("Failed to build batch command: {err}"),
                                                ))
                                                .await;
                                            continue;
                                        }
                                    }
                                }
                            } else {
                                None
                            }
                        }
                        Err(err) => {
                            let _ = input_tx
                                .send(InputEvent::ShellError(
                                    "system".to_string(),
                                    format!("Failed to load runtime config for shell: {err}"),
                                ))
                                .await;
                            continue;
                        }
                    };

                    let shell_result = crate::tui::services::run_pty_command(
                        cmd.clone(),
                        shell_spec,
                        {
                            let (shell_tx, mut shell_rx) = tokio::sync::mpsc::channel(100);
                            let input_tx_inner = input_tx.clone();
                            tokio::spawn(async move {
                                while let Some(event) = shell_rx.recv().await {
                                    match event {
                                        crate::tui::services::ShellEvent::Output(id, text) => {
                                            let _ = input_tx_inner
                                                .send(InputEvent::ShellOutput(id, text))
                                                .await;
                                        }
                                        crate::tui::services::ShellEvent::Error(id, text) => {
                                            let _ = input_tx_inner
                                                .send(InputEvent::ShellError(id, text))
                                                .await;
                                        }
                                        crate::tui::services::ShellEvent::Completed(id, code) => {
                                            let _ = input_tx_inner
                                                .send(InputEvent::ShellCompleted(id, code))
                                                .await;
                                        }
                                        crate::tui::services::ShellEvent::WaitingForInput(id) => {
                                            let _ = input_tx_inner
                                                .send(InputEvent::ShellWaitingForInput(id))
                                                .await;
                                        }
                                    }
                                }
                            });
                            shell_tx
                        },
                        rows,
                        cols,
                    )
                    .map_err(|err| err.to_string());

                    match shell_result {
                        Ok(shell) => {
                            let _ = input_tx.send(InputEvent::ShellStarted(shell)).await;
                        }
                        Err(err_msg) => {
                            let _ = input_tx
                                .send(InputEvent::ShellError(
                                    "system".to_string(),
                                    format!("Failed to start shell: {err_msg}"),
                                ))
                                .await;
                        }
                    }
                }
                OutputEvent::ListSessions => {
                    let eng = engine_clone.lock().await;
                    if let Ok(sessions) = eng.list_sessions().await {
                        let session_infos = sessions
                            .into_iter()
                            .map(|s| {
                                let id_str = s.id.to_string();
                                let checkpoint_dir = std::path::Path::new(".vac/checkpoints");
                                let state_file =
                                    checkpoint_dir.join(format!("{}_state.json", id_str));
                                let has_checkpoint = state_file.exists();
                                // Collect checkpoint files for this session (sorted newest first)
                                let checkpoints: Vec<String> = if checkpoint_dir.exists() {
                                    let mut files: Vec<_> = std::fs::read_dir(checkpoint_dir)
                                        .into_iter()
                                        .flatten()
                                        .flatten()
                                        .filter(|e| {
                                            e.file_name().to_string_lossy().starts_with(&id_str)
                                        })
                                        .filter_map(|e| {
                                            let name = e.file_name().to_string_lossy().to_string();
                                            let modified = e.metadata().ok()?.modified().ok()?;
                                            Some((modified, name))
                                        })
                                        .collect();
                                    files.sort_by(|a, b| b.0.cmp(&a.0));
                                    files.into_iter().map(|(_, name)| name).take(5).collect()
                                } else {
                                    vec![]
                                };
                                let last_activity =
                                    s.updated_at.format("%Y-%m-%d %H:%M").to_string();
                                crate::tui::app::SessionInfo {
                                    id: id_str.clone(),
                                    title: format!("Session {}", &id_str[..8]),
                                    updated_at: s.updated_at.to_rfc3339(),
                                    checkpoints,
                                    task_count: s.tasks.len(),
                                    last_activity,
                                    has_checkpoint,
                                }
                            })
                            .collect();
                        let _ = input_tx_clone
                            .send(InputEvent::SetSessions(session_infos))
                            .await;
                    } else {
                        let _ = input_tx_clone.send(InputEvent::SetSessions(vec![])).await;
                    }
                }
                OutputEvent::ListRuntimeJobs => {
                    let jobs = load_runtime_jobs(&runtime_project_root).await;
                    let _ = input_tx_clone.send(InputEvent::SetRuntimeJobs(jobs)).await;
                }
                OutputEvent::LoadRuntimeState => {
                    let snapshot = load_runtime_state(&runtime_project_root);
                    let _ = input_tx_clone
                        .send(InputEvent::SetRuntimeState(snapshot))
                        .await;
                }
                OutputEvent::CancelRuntimeJob(id) => {
                    let queue = vac_runtime::TaskQueue::with_storage(
                        runtime_project_root.join(".vac/queue.json"),
                    );
                    let cancelled = queue.cancel(id).await;
                    let toast = if cancelled {
                        crate::tui::services::Toast::success(format!("Cancelled job {}", id))
                    } else {
                        crate::tui::services::Toast::error(format!("Failed to cancel job {}", id))
                    };
                    let jobs = load_runtime_jobs(&runtime_project_root).await;
                    let snapshot = load_runtime_state(&runtime_project_root);
                    let _ = input_tx_clone.send(InputEvent::SetRuntimeJobs(jobs)).await;
                    let _ = input_tx_clone
                        .send(InputEvent::SetRuntimeState(snapshot))
                        .await;
                    let _ = input_tx_clone.send(InputEvent::ShowToast(toast)).await;
                }
                OutputEvent::RetryRuntimeJob(id) => {
                    let queue = vac_runtime::TaskQueue::with_storage(
                        runtime_project_root.join(".vac/queue.json"),
                    );
                    let retried = queue.retry(id).await;
                    let toast = if retried {
                        crate::tui::services::Toast::success(format!("Retried job {}", id))
                    } else {
                        crate::tui::services::Toast::error(format!("Failed to retry job {}", id))
                    };
                    let jobs = load_runtime_jobs(&runtime_project_root).await;
                    let snapshot = load_runtime_state(&runtime_project_root);
                    let _ = input_tx_clone.send(InputEvent::SetRuntimeJobs(jobs)).await;
                    let _ = input_tx_clone
                        .send(InputEvent::SetRuntimeState(snapshot))
                        .await;
                    let _ = input_tx_clone.send(InputEvent::ShowToast(toast)).await;
                }
                OutputEvent::NewSession => {
                    let eng = engine_clone.lock().await;
                    if let Ok(status) = eng.status().await {
                        let mut current_session = eng.session().write().await;
                        *current_session = vac_core::session::Session::new(status.project_root);
                        let _ = input_tx_clone
                            .send(InputEvent::SessionRestored {
                                id: current_session.id.to_string(),
                                title: "New Session".to_string(),
                                messages: vec![],
                            })
                            .await;
                    }
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
            .map(|(provider, model)| crate::tui::Model {
                id: model.clone(),
                name: model,
                provider,
                supports_reasoning: false,
            })
            .collect::<Vec<_>>();
        let _ = input_tx
            .send(InputEvent::AvailableModelsLoaded(models))
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
            .send(InputEvent::ShowToast(crate::tui::services::Toast::info(
                "VIL Knowledge missing. Using bootstrap fallback.".to_string(),
            )))
            .await;
    }

    let config = vac_core::VacConfig::load_with_fallback(&project_root).unwrap_or_default();
    if config.mcp_servers.as_ref().map_or(true, |s| s.is_empty()) {
        let _ = input_tx
            .send(InputEvent::ShowToast(crate::tui::services::Toast::info(
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
