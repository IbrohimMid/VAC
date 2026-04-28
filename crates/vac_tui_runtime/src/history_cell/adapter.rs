use super::VacHistoryCell;
use crate::app::Message;
use serde_json::Value;
use std::str::FromStr;
use vac_core::engine::RuntimeUpdate;

pub fn from_message(msg: &Message) -> Vec<VacHistoryCell> {
    let mut cells = Vec::new();
    match msg.role.as_str() {
        "user" => {
            cells.push(VacHistoryCell::UserMessage {
                content: msg.content.clone(),
            });
        }
        "assistant" => {
            if !msg.content.is_empty() {
                cells.push(VacHistoryCell::AssistantMarkdown {
                    content: msg.content.clone(),
                });
            }
        }
        _ => {
            cells.push(VacHistoryCell::Info {
                message: msg.content.clone(),
            });
        }
    }

    if let Some(tool_calls) = &msg.tool_calls {
        for tc in tool_calls {
            let args = Value::from_str(&tc.function.arguments).unwrap_or(Value::Null);
            cells.push(VacHistoryCell::ToolCall {
                id: tc.id.clone(),
                name: tc.function.name.clone(),
                arguments: args,
            });
        }
    }
    cells
}

pub fn from_runtime_update(update: &RuntimeUpdate) -> Option<VacHistoryCell> {
    match update {
        RuntimeUpdate::Status(status) => Some(VacHistoryCell::Info {
            message: status.clone(),
        }),
        RuntimeUpdate::AssistantChunk(chunk) => Some(VacHistoryCell::AssistantStream {
            chunk: chunk.clone(),
        }),
        RuntimeUpdate::ToolCall {
            id,
            name,
            arguments,
        } => Some(VacHistoryCell::ToolCall {
            id: id.clone(),
            name: name.clone(),
            arguments: arguments.clone(),
        }),
        RuntimeUpdate::ToolResult {
            id,
            name,
            success,
            content,
            ..
        } => Some(VacHistoryCell::ToolResult {
            id: id.clone(),
            name: name.clone(),
            success: *success,
            content: content.clone(),
        }),
        RuntimeUpdate::Failed(err) => Some(VacHistoryCell::Error {
            message: err.clone(),
        }),
        RuntimeUpdate::ApprovalRequired {
            tool_call_id,
            tool_name,
            arguments,
            explanation,
        } => Some(VacHistoryCell::ApprovalPrompt {
            tool_call_id: tool_call_id.clone(),
            tool_name: tool_name.clone(),
            arguments: arguments.clone(),
            explanation: explanation.clone(),
        }),
        RuntimeUpdate::LspDiagnostics(diag) => Some(VacHistoryCell::VilDiagnostic {
            diagnostic: format!("{:?}", diag),
        }),
        // Ignoring other updates for now
        _ => None,
    }
}
