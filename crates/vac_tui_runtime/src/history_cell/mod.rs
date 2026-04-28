pub mod adapter;
pub mod render;

use serde_json::Value;

#[derive(Debug, Clone)]
pub enum VacHistoryCell {
    UserMessage {
        content: String,
    },
    AssistantStream {
        chunk: String,
    },
    AssistantMarkdown {
        content: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
    ToolResult {
        id: String,
        name: String,
        success: bool,
        content: String,
    },
    ApprovalPrompt {
        tool_call_id: String,
        tool_name: String,
        arguments: Value,
        explanation: Option<String>,
    },
    ProposedPlan {
        plan: String,
    },
    TodoList {
        items: Vec<String>,
    },
    Error {
        message: String,
    },
    Info {
        message: String,
    },
    VilDiagnostic {
        diagnostic: String,
    },
}
