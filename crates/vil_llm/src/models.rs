//! Universal message format — provider-neutral representation.
//!
//! Provides a stable intermediate format between internal Message
//! and provider-specific formats (Anthropic, OpenAI, etc.).

use crate::provider::{Message, Role, ToolCall};
use serde::{Deserialize, Serialize};

/// Provider-neutral message format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: LlmRole,
    pub content: Vec<LlmContent>,
}

/// Universal role enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LlmRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Universal content block.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LlmContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

impl From<&Message> for LlmMessage {
    fn from(msg: &Message) -> Self {
        let role = match msg.role {
            Role::System => LlmRole::System,
            Role::User => LlmRole::User,
            Role::Assistant => LlmRole::Assistant,
            Role::Tool => LlmRole::Tool,
        };

        let mut content = Vec::new();

        // Add text content if present
        if !msg.content.is_empty() {
            content.push(LlmContent::Text {
                text: msg.content.clone(),
            });
        }

        // Add tool uses for assistant messages
        if msg.role == Role::Assistant && !msg.tool_calls.is_empty() {
            for tc in &msg.tool_calls {
                content.push(LlmContent::ToolUse {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    input: tc.arguments.clone(),
                });
            }
        }

        // Add tool result for tool messages
        if msg.role == Role::Tool {
            if let Some(ref id) = msg.tool_call_id {
                content.push(LlmContent::ToolResult {
                    tool_use_id: id.clone(),
                    content: msg.content.clone(),
                });
            }
        }

        Self { role, content }
    }
}

impl LlmMessage {
    /// Convert to OpenAI-compatible format.
    pub fn to_openai_format(&self) -> serde_json::Value {
        let role = match self.role {
            LlmRole::System => "system",
            LlmRole::User => "user",
            LlmRole::Assistant => "assistant",
            LlmRole::Tool => "tool",
        };

        let mut result = serde_json::json!({ "role": role });

        // Extract text content
        let text_content: Vec<String> = self
            .content
            .iter()
            .filter_map(|c| match c {
                LlmContent::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect();

        if !text_content.is_empty() {
            result["content"] = serde_json::json!(text_content.join("\n\n"));
        }

        // Add tool_calls for assistant
        let tool_calls: Vec<_> = self
            .content
            .iter()
            .filter_map(|c| match c {
                LlmContent::ToolUse { id, name, input } => Some(serde_json::json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": input.to_string()
                    }
                })),
                _ => None,
            })
            .collect();

        if !tool_calls.is_empty() {
            result["tool_calls"] = serde_json::json!(tool_calls);
        }

        // Add tool_call_id for tool messages
        if let Some(LlmContent::ToolResult { tool_use_id, .. }) = self.content.first() {
            result["tool_call_id"] = serde_json::json!(tool_use_id);
        }

        result
    }
}

impl From<&LlmMessage> for Message {
    fn from(msg: &LlmMessage) -> Self {
        let role = match msg.role {
            LlmRole::System => Role::System,
            LlmRole::User => Role::User,
            LlmRole::Assistant => Role::Assistant,
            LlmRole::Tool => Role::Tool,
        };

        // Extract text content
        let text: String = msg
            .content
            .iter()
            .filter_map(|c| match c {
                LlmContent::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect();

        // Extract tool calls
        let tool_calls: Vec<ToolCall> = msg
            .content
            .iter()
            .filter_map(|c| match c {
                LlmContent::ToolUse { id, name, input } => Some(ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: input.clone(),
                }),
                _ => None,
            })
            .collect();

        // Extract tool_call_id for tool messages
        let tool_call_id = msg.content.iter().find_map(|c| match c {
            LlmContent::ToolResult { tool_use_id, .. } => Some(tool_use_id.clone()),
            _ => None,
        });

        Message {
            role,
            content: text,
            name: None,
            tool_call_id,
            tool_calls,
        }
    }
}
