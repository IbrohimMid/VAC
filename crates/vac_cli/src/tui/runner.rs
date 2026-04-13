//! TUI Runner - Entry point for VAC TUI

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use vac_core::engine::VacEngine;
use vac_core::RuntimeUpdate;

use super::{run_tui, InputEvent, OutputEvent, ToolCall, FunctionCall, ToolCallResult, ToolCallResultStatus, LoadingOperation};

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
pub async fn run_vac_tui(project_root: PathBuf, _resume: bool) -> Result<()> {
    // Initialize VacEngine
    let engine = VacEngine::new(project_root.clone()).await?;
    let engine = Arc::new(Mutex::new(engine));

    // Create channels
    let (input_tx, input_rx) = mpsc::channel::<InputEvent>(100);
    let (output_tx, mut output_rx) = mpsc::channel::<OutputEvent>(100);
    let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

    // Spawn task to handle output events
    let engine_clone = engine.clone();
    let input_tx_clone = input_tx.clone();
    tokio::spawn(async move {
        while let Some(event) = output_rx.recv().await {
            match event {
                OutputEvent::UserMessage(msg, _tools, _parts, _usize) => {
                    let engine = engine_clone.clone();
                    let input_tx = input_tx_clone.clone();
                    let msg = msg.clone();
                    
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
                OutputEvent::AcceptTool(tc) => {
                    let msg = format!("I have APPROVED the tool call '{}' with arguments '{}'. Please proceed.", tc.function.name, tc.function.arguments);
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
                OutputEvent::RejectTool(tc, _) => {
                    let msg = format!("I have REJECTED the tool call '{}'. Please revise your plan.", tc.function.name);
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
                        let _ = eng.load_session(uuid).await;
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