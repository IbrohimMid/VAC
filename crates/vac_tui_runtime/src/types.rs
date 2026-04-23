//! Stub Types for Stakpak Dependencies
//!
//! These types replace Stakpak's internal types with VAC-compatible equivalents.
//! The goal is to maintain API compatibility while using VAC's semantics.

use serde::{Deserialize, Serialize};

// ========== Model (from stakai) ==========

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub supports_reasoning: bool,
    #[serde(default)]
    pub supports_tool_calls: bool,
    #[serde(default)]
    pub supports_streaming: bool,
    #[serde(default)]
    pub context_window: usize,
    #[serde(default)]
    pub cost_class: String,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            id: "vac-default".to_string(),
            name: "VAC Default".to_string(),
            provider: "vac".to_string(),
            supports_reasoning: false,
            supports_tool_calls: true,
            supports_streaming: true,
            context_window: 128000,
            cost_class: "standard".to_string(),
        }
    }
}

// ========== ToolCall Types (from stakpak_shared) ==========

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub r#type: String,
    pub function: FunctionCall,
    pub metadata: Option<ToolCallMetadata>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallMetadata {
    pub pause_info: Option<TaskPauseInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub call: ToolCall,
    pub result: String,
    pub status: ToolCallResultStatus,
    pub envelope: Option<vac_tool_core::ToolResultEnvelope>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToolCallResultStatus {
    Success,
    Error,
    Pending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResultProgress {
    pub call_id: String,
    pub delta: String,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallStreamInfo {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

// ========== Content Types ==========

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentPart {
    pub r#type: String,
    pub text: Option<String>,
    pub image_url: Option<ImageUrl>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
}

// ========== Token Usage ==========

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LLMTokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

// ========== Ask User Types ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserQuestion {
    pub label: String,
    pub question: String,
    pub r#type: String,
    pub options: Vec<AskUserOption>,
    pub required: bool,
    #[serde(default)]
    pub kind: Option<crate::services::ask_user::AskUserQuestionKind>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserOption {
    pub value: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserAnswer {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub selected: Option<Vec<String>>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub question_metadata: std::collections::HashMap<String, String>,
}

// ========== Task Pause Info ==========

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskPauseInfo {
    pub task_id: String,
    pub reason: String,
    pub details: Option<String>,
}

// ========== Billing ==========

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BillingResponse {
    pub credits_remaining: u64,
    pub credits_used: u64,
}

// ========== Rulebook ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRuleBook {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

// ========== Secret Manager Stub ==========

pub struct SecretManager;

impl SecretManager {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SecretManager {
    fn default() -> Self {
        Self::new()
    }
}
