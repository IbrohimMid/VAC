//! Type Adapters
//!
//! Provides type mappings between Stakpak TUI types and VAC types.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Model representation for TUI (adapted from Stakpak's stakai::Model)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub supports_reasoning: bool,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            id: "default".to_string(),
            name: "Default Model".to_string(),
            provider: "vac".to_string(),
            supports_reasoning: false,
        }
    }
}

/// Tool call representation (adapted from Stakpak's ToolCall)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// Tool call result (adapted from Stakpak's ToolCallResult)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub call_id: String,
    pub result: String,
    pub status: ToolCallStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToolCallStatus {
    Success,
    Error,
    Pending,
}

/// Token usage tracking
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LLMTokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// Content part for multimodal messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentPart {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: Option<String>,
    pub image_url: Option<ImageUrl>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
}

/// Rulebook representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRuleBook {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

/// Billing response (stub for VAC)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BillingResponse {
    pub credits_remaining: u64,
    pub credits_used: u64,
}

/// Ask user question (for interactive prompts)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserQuestion {
    pub label: String,
    pub question: String,
    pub options: Vec<AskUserOption>,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserAnswer {
    pub value: String,
    pub custom: bool,
}

/// Task pause info for subagent tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPauseInfo {
    pub task_id: String,
    pub reason: String,
    pub details: Option<String>,
}