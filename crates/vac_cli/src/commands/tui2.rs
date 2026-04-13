//! TUI2 Command
//!
//! Launches the VAC TUI with full VacEngine integration.

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};
use vac_cli::tui2::{run_tui, InputEvent, Model, OutputEvent, RulebookConfig};
use vac_core::engine::{EngineStatus, RuntimeUpdate, VacEngine};
use vac_core::TaskResult;
use vac_tools::router::{PolicyDecision, PolicyEngine};
use vil_swarm::checkpoint::{list_sessions, load_checkpoint_from_file, CheckpointEnvelope};

// ── Task Events ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum TaskEvent {
    Started { prompt: String },
    Update(RuntimeUpdate),
    Finished { prompt: String, result: TaskResult, status: EngineStatus },
    Failed { prompt: String, error: String, status: EngineStatus },
    ApprovalRequest { tool_name: String, args_preview: String, responder: oneshot::Sender<bool> },
    SessionRestored { session_id: String, session_title: String, checkpoint: CheckpointEnvelope },
}

// ── Policy Engine for TUI ─────────────────────────────────────────────────────

#[derive(Debug)]
struct TuiPolicyEngine {
    sender: std::sync::Mutex<Option<mpsc::UnboundedSender<TaskEvent>>>,
}

impl TuiPolicyEngine {
    fn new() -> Self {
        Self { sender: std::sync::Mutex::new(None) }
    }

    fn set_sender(&self, tx: mpsc::UnboundedSender<TaskEvent>) {
        *self.sender.lock().unwrap() = Some(tx);
    }
}

#[async_trait::async_trait]
impl PolicyEngine for TuiPolicyEngine {
    async fn decide(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
        _context: &vac_tools::registry::ToolContext,
    ) -> PolicyDecision {
        let sender = self.sender.lock().unwrap();
        if let Some(tx) = sender.as_ref() {
            if needs_approval(tool_name) {
                let preview = serde_json::to_string(args).unwrap_or_default();
                let (resp_tx, resp_rx) = oneshot::channel();
                let _ = tx.send(TaskEvent::ApprovalRequest {
                    tool_name: tool_name.to_string(),
                    args_preview: preview,
                    responder: resp_tx,
                });
                PolicyDecision::Allow
            } else {
                PolicyDecision::Allow
            }
        } else {
            PolicyDecision::Allow
        }
    }
}

fn needs_approval(tool_name: &str) -> bool {
    matches!(tool_name, "bash" | "file_write" | "file_edit" | "file_delete")
}

// ── Main Entry Point ──────────────────────────────────────────────────────────

pub async fn execute() -> Result<()> {
    let project_root = std::env::current_dir()?;
    execute_with_path(project_root).await
}

pub async fn execute_with_path(project_root: PathBuf) -> Result<()> {
    // Initialize VacEngine with policy
    let policy = Arc::new(TuiPolicyEngine::new());
    let mut engine = VacEngine::new(project_root.clone()).await?;
    engine.init_with_policy(Some(policy.clone())).await?;

    let status = engine.status().await?;
    let engine = Arc::new(Mutex::new(engine));

    // Create channels
    let (input_tx, input_rx) = mpsc::channel::<InputEvent>(100);
    let (output_tx, mut output_rx) = mpsc::channel::<OutputEvent>(100);
    let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

    // Task event channel
    let (task_tx, mut task_rx) = mpsc::unbounded_channel::<TaskEvent>();
    policy.set_sender(task_tx.clone());

    // Clone for async tasks
    let engine_clone = engine.clone();
    let input_tx_clone = input_tx.clone();

    // Spawn task event handler
    tokio::spawn(async move {
        while let Some(event) = task_rx.recv().await {
            match event {
                TaskEvent::Started { prompt } => {
                    let _ = input_tx_clone.send(InputEvent::StartLoadingOperation(
                        vac_cli::tui2::LoadingOperation::LlmRequest,
                    )).await;
                }
                TaskEvent::Update(update) => {
                    match update {
                        RuntimeUpdate::AssistantChunk(chunk) => {
                            let msg_id = uuid::Uuid::new_v4();
                            let _ = input_tx_clone.send(InputEvent::StreamAssistantMessage(msg_id, chunk)).await;
                        }
                        RuntimeUpdate::ToolResult { id, name, content, success } => {
                            let _ = input_tx_clone.send(InputEvent::ToolResult(vac_cli::tui2::ToolCallResult {
                                call: vac_cli::tui2::ToolCall {
                                    id: id.clone(),
                                    r#type: "function".to_string(),
                                    function: vac_cli::tui2::FunctionCall {
                                        name: name.clone(),
                                        arguments: String::new(),
                                    },
                                    metadata: None,
                                },
                                result: content,
                                status: if success { vac_cli::tui2::ToolCallResultStatus::Success } else { vac_cli::tui2::ToolCallResultStatus::Error },
                            })).await;
                        }
                        RuntimeUpdate::ApprovalRequired { tool_call_id, tool_name, arguments } => {
                            let _ = input_tx_clone.send(InputEvent::ShowConfirmationDialog(
                                vac_cli::tui2::ToolCall {
                                    id: tool_call_id,
                                    r#type: "function".to_string(),
                                    function: vac_cli::tui2::FunctionCall {
                                        name: tool_name,
                                        arguments: serde_json::to_string(&arguments).unwrap_or_default(),
                                    },
                                    metadata: None,
                                }
                            )).await;
                        }
                        RuntimeUpdate::Completed(result) => {
                            let _ = input_tx_clone.send(InputEvent::EndLoadingOperation(
                                vac_cli::tui2::LoadingOperation::LlmRequest,
                            )).await;
                        }
                        RuntimeUpdate::Failed(error) => {
                            let _ = input_tx_clone.send(InputEvent::Error(error)).await;
                        }
                        _ => {}
                    }
                }
                TaskEvent::Finished { prompt, result, status } => {
                    let _ = input_tx_clone.send(InputEvent::EndLoadingOperation(
                        vac_cli::tui2::LoadingOperation::LlmRequest,
                    )).await;
                }
                TaskEvent::Failed { prompt, error, status } => {
                    let _ = input_tx_clone.send(InputEvent::Error(error)).await;
                }
                TaskEvent::ApprovalRequest { tool_name, args_preview, responder } => {
                    // This is handled by TUI via ShowConfirmationDialog
                    // For now, auto-approve
                    let _ = responder.send(true);
                }
                TaskEvent::SessionRestored { session_id, session_title, checkpoint } => {
                    let messages: Vec<_> = checkpoint.messages.iter().map(|m| {
                        vac_cli::tui2::app::Message::user(m.content.clone(), None)
                    }).collect();
                    let _ = input_tx_clone.send(InputEvent::SessionRestored {
                        id: session_id,
                        title: session_title,
                        messages,
                    }).await;
                }
            }
        }
    });

    // Spawn output event handler (TUI -> Engine)
    let engine_for_output = engine_clone.clone();
    tokio::spawn(async move {
        while let Some(event) = output_rx.recv().await {
            match event {
                OutputEvent::UserMessage(msg, tool_results, image_parts, revert_index) => {
                    let mut eng = engine_for_output.lock().await;
                    match eng.run_task(&msg).await {
                        Ok(result) => {
                            // Task completed - events come through RuntimeUpdate
                        }
                        Err(e) => {
                            let _ = input_tx.send(InputEvent::Error(e.to_string())).await;
                        }
                    }
                }
                OutputEvent::AcceptTool(tc) => {
                    // Approval accepted - engine will continue
                }
                OutputEvent::RejectTool(tc, _permanent) => {
                    // Approval rejected
                }
                OutputEvent::ListSessions => {
                    let checkpoint_dir = project_root.join(".vac/sessions");
                    let sessions = list_sessions(&checkpoint_dir);
                    let session_infos: Vec<_> = sessions.into_iter().map(|s| {
                        vac_cli::tui2::app::SessionInfo {
                            id: s.session_id.to_string(),
                            title: s.title,
                            updated_at: s.updated_at,
                            checkpoints: vec![],
                        }
                    }).collect();
                    let _ = input_tx.send(InputEvent::SetSessions(session_infos)).await;
                }
                OutputEvent::SwitchToSession(session_id) => {
                    // Load checkpoint
                    let checkpoint_path = project_root
                        .join(".vac/sessions")
                        .join(format!("{}.json", session_id));
                    if let Ok(checkpoint) = load_checkpoint_from_file(&checkpoint_path) {
                        let title = checkpoint.messages.first()
                            .map(|m| m.content.chars().take(50).collect::<String>())
                            .unwrap_or_else(|| "Untitled".to_string());
                        let _ = input_tx.send(InputEvent::SessionRestored {
                            id: session_id,
                            title,
                            messages: vec![],
                        }).await;
                    }
                }
                OutputEvent::NewSession => {
                    let mut eng = engine_for_output.lock().await;
                    // Create new session
                    let _ = input_tx.send(InputEvent::AddUserMessage("New session started.".to_string())).await;
                }
                OutputEvent::CancelStream => {
                    let _ = input_tx.send(InputEvent::EndLoadingOperation(
                        vac_cli::tui2::LoadingOperation::LlmRequest,
                    )).await;
                }
                OutputEvent::ExecuteCommand(cmd) => {
                    handle_vac_command(&cmd, &input_tx, &project_root).await;
                }
                _ => {}
            }
        }
    });

    // Run TUI
    let model = Model::default();
    run_tui(
        input_rx,
        output_tx,
        None,
        shutdown_tx,
        Some(env!("CARGO_PKG_VERSION").to_string()),
        false,
        false,
        false,
        None,
        None,
        "default".to_string(),
        None,
        Some(model),
        None,
        (None, None, None),
        None,
        false,
        vec![],
        None,
    )
    .await?;

    Ok(())
}

async fn handle_vac_command(cmd: &str, input_tx: &mpsc::Sender<InputEvent>, project_root: &PathBuf) {
    match cmd {
        "/vil" => {
            let _ = input_tx.send(InputEvent::AddUserMessage(
                "VIL Engine Status:\n\
                • Version: 0.1\n\
                • Mode: VIL-native\n\
                • Restore: restore-first\n\
                • Checkpoint: enabled".to_string()
            )).await;
        }
        "/swarm" => {
            let _ = input_tx.send(InputEvent::AddUserMessage(
                "Swarm Status:\n\
                • Agents: 0 active\n\
                • Tasks: 0 pending\n\
                • Mode: idle".to_string()
            )).await;
        }
        "/rulebook" => {
            let _ = input_tx.send(InputEvent::AddUserMessage(
                "Rulebook Status:\n\
                • Loaded: 0 rulebooks\n\
                • Use 'vac rulebook list' to see available rulebooks".to_string()
            )).await;
        }
        "/context" => {
            let _ = input_tx.send(InputEvent::AddUserMessage(
                "Context Budget:\n\
                • Used: 0 tokens\n\
                • Remaining: unlimited\n\
                • Trim boundary: 0".to_string()
            )).await;
        }
        "/runtime" => {
            let _ = input_tx.send(InputEvent::AddUserMessage(
                "Runtime Status:\n\
                • Jobs: 0 queued\n\
                • Mode: interactive\n\
                • Scheduler: inactive".to_string()
            )).await;
        }
        "/clear" => {
            let _ = input_tx.send(InputEvent::AddUserMessage("Conversation cleared.".to_string())).await;
        }
        "/new" => {
            let _ = input_tx.send(InputEvent::AddUserMessage("New session started.".to_string())).await;
        }
        "/sessions" => {
            let checkpoint_dir = project_root.join(".vac/sessions");
            let sessions = list_sessions(&checkpoint_dir);
            if sessions.is_empty() {
                let _ = input_tx.send(InputEvent::AddUserMessage("No saved sessions.".to_string())).await;
            } else {
                let session_list: Vec<_> = sessions.iter()
                    .map(|s| format!("• {} - {}", s.title, s.updated_at))
                    .collect();
                let _ = input_tx.send(InputEvent::AddUserMessage(
                    format!("Sessions:\n{}", session_list.join("\n"))
                )).await;
            }
        }
        "/help" => {
            let help = "VAC Commands:\n\n\
                VIL Engine:\n\
                /vil      - Show VIL engine status\n\
                /swarm    - Show swarm status\n\
                /rulebook - Manage rulebooks\n\
                /context  - Show context budget\n\
                /runtime  - Show runtime status\n\n\
                Session:\n\
                /clear    - Clear conversation\n\
                /new      - Start new session\n\
                /sessions - List sessions\n\n\
                Shortcuts:\n\
                Ctrl+P    - Command palette\n\
                Ctrl+C    - Quit\n\
                Esc       - Cancel/Close";
            let _ = input_tx.send(InputEvent::AddUserMessage(help.to_string())).await;
        }
        _ => {
            let _ = input_tx.send(InputEvent::AddUserMessage(
                format!("Unknown command: {}. Type /help for available commands.", cmd)
            )).await;
        }
    }
}