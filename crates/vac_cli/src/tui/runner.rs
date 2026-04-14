//! TUI Runner - Entry point for VAC TUI

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use vac_core::engine::VacEngine;
use vac_core::RuntimeUpdate;

use super::{run_tui, InputEvent, OutputEvent, ToolCall, FunctionCall, ToolCallResult, ToolCallResultStatus, LoadingOperation};

/// Shared handle to the active task's update channel for structured approval routing.
type ActiveUpdateTx = Arc<Mutex<Option<mpsc::UnboundedSender<RuntimeUpdate>>>>;

/// Shared handle to the active task's approval channel for structured approval flow.
type ActiveApprovalTx = Arc<Mutex<Option<mpsc::UnboundedSender<vil_swarm::ApprovalResponse>>>>;

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
        RuntimeUpdate::AssistantChunk(chunk) => {
            let _ = input_tx_inner
                .send(InputEvent::StreamAssistantMessage(stream_uuid, chunk))
                .await;
        }
        RuntimeUpdate::ToolCall { id, name, arguments } => {
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
            let _ = input_tx_inner.send(InputEvent::RunToolCall(tool_call)).await;
        }
        RuntimeUpdate::ToolResult { id, name, content, success } => {
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
            let _ = input_tx_inner.send(InputEvent::ToolResult(tool_result)).await;
        }
        RuntimeUpdate::Completed(result) => {
            let _ = input_tx_inner.send(InputEvent::EndLoadingOperation(LoadingOperation::LlmRequest)).await;
            if !result.summary.is_empty() {
                let _ = input_tx_inner
                    .send(InputEvent::StreamAssistantMessage(
                        stream_uuid,
                        format!("\n\n**Result:**\n{}", result.summary),
                    ))
                    .await;
            }
        }
        RuntimeUpdate::Failed(err) => {
            let _ = input_tx_inner.send(InputEvent::EndLoadingOperation(LoadingOperation::LlmRequest)).await;
            let _ = input_tx_inner.send(InputEvent::Error(err)).await;
        }
        RuntimeUpdate::ApprovalRequired { tool_call_id, tool_name, arguments } => {
            let tool_call = ToolCall {
                id: tool_call_id,
                r#type: "function".to_string(),
                function: FunctionCall {
                    name: tool_name,
                    arguments: arguments.to_string(),
                },
                metadata: None,
            };
            let _ = input_tx_inner.send(InputEvent::ShowConfirmationDialog(tool_call)).await;
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
    
    let engine = Arc::new(Mutex::new(engine));

    // Create channels
    let (input_tx, input_rx) = mpsc::channel::<InputEvent>(100);
    let (output_tx, mut output_rx) = mpsc::channel::<OutputEvent>(100);
    let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

    // Shared handle to active task's update channel for structured approval routing
    let active_update_tx: ActiveUpdateTx = Arc::new(Mutex::new(None));

    // Shared handle to active task's approval channel for structured approval flow
    let active_approval_tx: ActiveApprovalTx = Arc::new(Mutex::new(None));

    // Handle session restore if requested
    if resume {
        // TODO: Implement proper session restore from last session
        // For now, just log that restore was requested
        log::info!("Session restore requested but not yet fully implemented");
    }

    // Spawn task to handle output events
    let engine_clone = engine.clone();
    let input_tx_clone = input_tx.clone();
    let active_update_tx_clone = active_update_tx.clone();
    let active_approval_tx_clone = active_approval_tx.clone();
    tokio::spawn(async move {
        while let Some(event) = output_rx.recv().await {
            match event {
                OutputEvent::UserMessage(msg, _tools, _parts, _usize) => {
                    let engine = engine_clone.clone();
                    let input_tx = input_tx_clone.clone();
                    let msg = msg.clone();
                    let active_tx = active_update_tx_clone.clone();
                    let active_approval = active_approval_tx_clone.clone();

                    let _ = input_tx.send(InputEvent::StartLoadingOperation(LoadingOperation::LlmRequest)).await;

                    tokio::spawn(async move {
                        let (update_tx, mut update_rx) = mpsc::unbounded_channel::<RuntimeUpdate>();
                        let (approval_tx, approval_rx) = mpsc::unbounded_channel::<vil_swarm::ApprovalResponse>();
                        let input_tx_inner = input_tx.clone();
                        let stream_uuid = uuid::Uuid::new_v4();

                        // Store active channels for structured approval routing
                        *active_tx.lock().await = Some(update_tx.clone());
                        *active_approval.lock().await = Some(approval_tx);

                        tokio::spawn(async move {
                            let mut active_tools: HashMap<String, ToolCall> = HashMap::new();
                            while let Some(update) = update_rx.recv().await {
                                handle_runtime_update(update, &input_tx_inner, stream_uuid, &mut active_tools).await;
                            }
                        });

                        let mut eng = engine.lock().await;
                        let _ = eng.run_task_with_approvals(&msg, Some(update_tx), None, Some(approval_rx)).await;

                        // Clear active channels when task completes
                        *active_tx.lock().await = None;
                        *active_approval.lock().await = None;
                    });
                }
                OutputEvent::AcceptTool(tc) => {
                    // Structured approval flow: send directly to swarm's approval channel
                    let approval_tx = active_approval_tx_clone.lock().await;
                    if let Some(ref tx) = *approval_tx {
                        let _ = tx.send(vil_swarm::ApprovalResponse {
                            tool_call_id: tc.id.clone(),
                            approved: true,
                            reason: None,
                        });
                    }
                }
                OutputEvent::RejectTool(tc, _) => {
                    // Structured rejection flow: send directly to swarm's approval channel
                    let approval_tx = active_approval_tx_clone.lock().await;
                    if let Some(ref tx) = *approval_tx {
                        let _ = tx.send(vil_swarm::ApprovalResponse {
                            tool_call_id: tc.id.clone(),
                            approved: false,
                            reason: Some(format!("User rejected tool '{}'", tc.function.name)),
                        });
                    }
                }
                OutputEvent::ExecuteCommand(cmd) => {
                    let msg = format!("Execute command: {}", cmd);
                    let engine = engine_clone.clone();
                    let input_tx = input_tx_clone.clone();
                    
                    let _ = input_tx.send(InputEvent::StartLoadingOperation(LoadingOperation::LlmRequest)).await;

                    tokio::spawn(async move {
                        let (update_tx, mut update_rx) = mpsc::unbounded_channel::<RuntimeUpdate>();
                        let input_tx_inner = input_tx.clone();
                        let stream_uuid = uuid::Uuid::new_v4();

                        tokio::spawn(async move {
                            let mut active_tools: HashMap<String, ToolCall> = HashMap::new();
                            while let Some(update) = update_rx.recv().await {
                                handle_runtime_update(update, &input_tx_inner, stream_uuid, &mut active_tools).await;
                            }
                        });

                        let mut eng = engine.lock().await;
                        let _ = eng.run_task_with_updates(&msg, Some(update_tx)).await;
                    });
                }
                OutputEvent::ListSessions => {
                    let _ = input_tx_clone.send(InputEvent::SetSessions(vec![])).await;
                }
                OutputEvent::NewSession => {
                }
                OutputEvent::ResumeSession(id) => {
                    if let Ok(uuid) = uuid::Uuid::parse_str(&id) {
                        let mut eng = engine_clone.lock().await;
                        if let Ok(_) = eng.load_session(uuid).await {
                            // TODO: Extract transcript from loaded session and send SessionRestored event
                            // For now, just notify that session was restored
                            let _ = input_tx_clone.send(InputEvent::SessionRestored {
                                id: id.clone(),
                                title: format!("Session {}", &id[..8]),
                                messages: vec![],
                            }).await;
                        }
                    }
                }
                _ => {}
            }
        }
    });

    // Run TUI
    run_tui(
        input_rx,
        output_tx,
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
        None,
        (None, None, None),
        None,
        false,
        vec![],
        None,
    ).await?;

    Ok(())
}