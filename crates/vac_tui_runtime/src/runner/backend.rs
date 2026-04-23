//! Backend send/recv helpers — tool approval, runtime update dispatch.

use anyhow::Result;
use std::collections::HashMap;
use tokio::sync::mpsc;

use crate::{
    FunctionCall, InputEvent, LoadingOperation, ToolCall, ToolCallResult, ToolCallResultStatus,
};
use vac_core::RuntimeUpdate;

pub(super) async fn resolve_tool_approval(
    approvals: &vac_approvals::ApprovalHandle,
    tool_call_id: String,
    approved: bool,
    reason: Option<String>,
) -> Result<(), vac_core::VacError> {
    if approved {
        Ok(approvals.approve(tool_call_id).await?)
    } else {
        Ok(approvals.reject(tool_call_id, reason).await?)
    }
}

pub(super) async fn handle_runtime_update(
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
            // Provide a simple capability matrix based on model name
            let is_reasoning = model.contains("o1")
                || model.contains("o3")
                || model.contains("r1")
                || model.contains("deepseek");
            let is_streaming = !is_reasoning;
            let context_window =
                if model.contains("opus") || model.contains("sonnet") || model.contains("gemini") {
                    200000
                } else {
                    128000
                };
            let cost_class = if model.contains("opus")
                || model.contains("o1")
                || model.contains("r1")
            {
                "premium".to_string()
            } else if model.contains("haiku") || model.contains("mini") || model.contains("flash") {
                "cheap".to_string()
            } else {
                "standard".to_string()
            };

            let _ = input_tx_inner
                .send(InputEvent::SetCurrentModel(crate::Model {
                    id: model.clone(),
                    name: model,
                    provider,
                    supports_reasoning: is_reasoning,
                    supports_tool_calls: true,
                    supports_streaming: is_streaming,
                    context_window,
                    cost_class,
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
            envelope,
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
                call: tool_call.clone(),
                result: result_preview,
                status,
                envelope,
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
        RuntimeUpdate::ValidationResult { score, issues } => {
            let _ = input_tx_inner
                .send(InputEvent::ValidationResult(score, issues))
                .await;
        }
        RuntimeUpdate::LspStatus {
            available,
            binary_path,
        } => {
            let _ = input_tx_inner
                .send(InputEvent::LspStatus(available, binary_path))
                .await;
        }
        RuntimeUpdate::LspDiagnostics(snapshot) => {
            let _ = input_tx_inner
                .send(InputEvent::LspDiagnostics(snapshot))
                .await;
        }
        RuntimeUpdate::Cancelled => {
            let _ = input_tx_inner
                .send(InputEvent::EndLoadingOperation(
                    LoadingOperation::LlmRequest,
                ))
                .await;
            let _ = input_tx_inner.send(InputEvent::TaskCancelled).await;
        }
    }
}
