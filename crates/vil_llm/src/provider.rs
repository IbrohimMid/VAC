//! LLM provider trait and common types.

use crate::error::LlmResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImagePart {
    /// "base64"
    pub source_type: String,
    /// e.g. "image/jpeg", "image/png"
    pub media_type: String,
    /// Base64-encoded image data
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    pub name: Option<String>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub image_parts: Vec<ImagePart>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: vec![],
            image_parts: vec![],
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: vec![],
            image_parts: vec![],
        }
    }
    pub fn user_with_images(content: impl Into<String>, images: Vec<ImagePart>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: vec![],
            image_parts: images,
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: vec![],
            image_parts: vec![],
        }
    }

    pub fn assistant_with_tool_calls(
        content: impl Into<String>,
        tool_calls: Vec<ToolCall>,
    ) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls,
            image_parts: vec![],
        }
    }

    pub fn tool(
        tool_name: impl Into<String>,
        tool_call_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            name: Some(tool_name.into()),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: vec![],
            image_parts: vec![],
        }
    }
}

/// Prompt-caching hint. Today only Anthropic `ephemeral` is meaningful — other
/// providers ignore the field. Stored alongside `LlmRequest` instead of inline
/// on `Message` so routing layers can flag messages without mutating them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CacheControlHint {
    /// Anthropic ephemeral breakpoint (`cache_control: {type: "ephemeral"}`).
    #[default]
    Ephemeral,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub messages: Vec<Message>,
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stop_sequences: Vec<String>,
    pub tools: Vec<ToolDefinition>,
    /// Indices into `messages` that should carry a `cache_control` breakpoint
    /// when the provider supports prompt caching (Anthropic). Out-of-range
    /// indices are silently ignored. Kept sorted by the helpers but not
    /// enforced so callers can re-use the field.
    #[serde(default)]
    pub cache_control_blocks: Vec<usize>,
    /// Hint shared by every entry in `cache_control_blocks`. Defaults to
    /// `Ephemeral`, which is the only variant today.
    #[serde(default)]
    pub cache_control_hint: CacheControlHint,
}

impl LlmRequest {
    pub fn new(messages: Vec<Message>) -> Self {
        Self {
            messages,
            model: None,
            max_tokens: None,
            temperature: None,
            stop_sequences: vec![],
            tools: vec![],
            cache_control_blocks: vec![],
            cache_control_hint: CacheControlHint::Ephemeral,
        }
    }

    /// Create from universal LlmMessage format
    pub fn from_llm_messages(messages: Vec<crate::models::LlmMessage>) -> Self {
        Self {
            messages: messages.iter().map(Message::from).collect(),
            model: None,
            max_tokens: None,
            temperature: None,
            stop_sequences: vec![],
            tools: vec![],
            cache_control_blocks: vec![],
            cache_control_hint: CacheControlHint::Ephemeral,
        }
    }

    /// Convert to universal LlmMessage format
    pub fn to_llm_messages(&self) -> Vec<crate::models::LlmMessage> {
        self.messages
            .iter()
            .map(crate::models::LlmMessage::from)
            .collect()
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = Some(model.to_string());
        self
    }

    pub fn with_max_tokens(mut self, max: u32) -> Self {
        self.max_tokens = Some(max);
        self
    }

    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    /// Flag one or more message indices for prompt-cache breakpoints.
    /// Out-of-range indices are filtered out so callers don't have to
    /// coordinate with message-list mutations.
    pub fn with_cache_control(mut self, indices: impl IntoIterator<Item = usize>) -> Self {
        let len = self.messages.len();
        self.cache_control_blocks = indices.into_iter().filter(|i| *i < len).collect();
        self.cache_control_blocks.sort_unstable();
        self.cache_control_blocks.dedup();
        self
    }

    /// True if the given message index should carry a cache-control marker.
    pub fn is_cache_marked(&self, idx: usize) -> bool {
        self.cache_control_blocks.binary_search(&idx).is_ok()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: String,
    pub model: String,
    pub finish_reason: FinishReason,
    pub usage: TokenUsage,
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinishReason {
    Stop,
    MaxTokens,
    ToolUse,
    Error,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    /// Tokens served from the provider's prompt cache (Anthropic
    /// `cache_read_input_tokens`). `0` on providers that don't report it.
    #[serde(default)]
    pub cached_tokens: u64,
    /// Tokens written to the provider's prompt cache this request
    /// (Anthropic `cache_creation_input_tokens`). Informational — the
    /// write is already billed as input tokens on Anthropic's wire.
    #[serde(default)]
    pub cache_creation_tokens: u64,
    /// `cached_tokens / (prompt_tokens + cached_tokens)`. Zero when no
    /// tokens were sent at all. Range: `[0.0, 1.0]`.
    #[serde(default)]
    pub cache_hit_rate: f64,
}

impl TokenUsage {
    /// Recompute `cache_hit_rate` from the token counts currently set.
    /// Safe on zero input (returns `0.0`). Call after populating
    /// `prompt_tokens` and `cached_tokens`.
    pub fn recompute_cache_hit_rate(&mut self) {
        let denom = self.prompt_tokens + self.cached_tokens;
        self.cache_hit_rate = if denom == 0 {
            0.0
        } else {
            self.cached_tokens as f64 / denom as f64
        };
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone)]
pub enum StreamChunk {
    Text(String),
    /// Announce a tool call is starting. Still emitted for low-level observability.
    ToolCallStart {
        id: String,
        name: String,
    },
    /// Raw partial tool-argument JSON fragment. Kept for low-level/debug consumers;
    /// high-level consumers should rely on `ToolCallComplete`, which only fires
    /// after the assembler has a fully parsed tool call.
    ToolCallDelta {
        id: String,
        arguments_delta: String,
    },
    /// A fully aggregated, parsed tool call. The preferred variant for UI consumers —
    /// never contains half-parsed JSON.
    ToolCallComplete(ToolCall),
    Done {
        usage: TokenUsage,
        finish_reason: FinishReason,
    },
    Error(String),
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn complete(&self, request: &LlmRequest) -> LlmResult<LlmResponse>;

    async fn stream(&self, request: &LlmRequest) -> LlmResult<mpsc::Receiver<StreamChunk>>;

    /// Whether the provider supports vision / image input on its default model.
    /// Default: `false`.
    fn supports_vision(&self) -> bool {
        false
    }

    /// Whether the provider supports tool/function calling on its default model.
    /// Default: `true` (most modern frontier providers do).
    fn supports_tools(&self) -> bool {
        true
    }

    /// Advertised max context window (in tokens) for the provider's default model.
    /// Default: `128_000`.
    fn max_context_tokens(&self) -> u32 {
        128_000
    }
}
