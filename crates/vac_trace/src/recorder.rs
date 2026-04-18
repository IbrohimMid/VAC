//! Session recorder — captures task events, tool calls, and messages.

use crate::error::TraceResult;
use crate::redaction::RedactionEngine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

pub struct TraceRecorder {
    session_id: Uuid,
    records: Vec<TraceRecord>,
    output_path: PathBuf,
    #[allow(dead_code)]
    enable_signing: bool,
    redaction: Option<RedactionEngine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceRecord {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub record_type: RecordType,
    pub agent_id: Option<String>,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecordType {
    TaskStart,
    TaskComplete,
    TaskFailed,
    ToolCall,
    ToolResult,
    LlmRequest,
    LlmResponse,
    AgentMessage,
    ContextRetrieval,
    ValidationResult,
    PolicyDecision,
    LspStarted,
    LspDiagnosticsSnapshot,
    LspPostEditRecheck,
    SubagentSpawned,
    SandboxCreated,
    SandboxDestroyed,
    PatchProposed,
    PatchMerged,
    Error,
}

impl TraceRecorder {
    pub fn new(output_path: PathBuf, enable_signing: bool) -> TraceResult<Self> {
        std::fs::create_dir_all(&output_path)?;
        Ok(Self {
            session_id: Uuid::new_v4(),
            records: Vec::new(),
            output_path,
            enable_signing,
            redaction: None,
        })
    }

    pub fn with_redaction(mut self, strip_paths: bool, custom_patterns: &[String]) -> Self {
        self.redaction = Some(RedactionEngine::new(strip_paths, custom_patterns));
        self
    }

    pub fn record(
        &mut self,
        record_type: RecordType,
        agent_id: Option<&str>,
        mut content: serde_json::Value,
    ) {
        if let Some(engine) = &self.redaction {
            content = engine.redact_value(&content);
        }
        self.records.push(TraceRecord {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            record_type,
            agent_id: agent_id.map(String::from),
            content,
        });
    }

    pub fn record_task(&mut self, task_id: &str, description: &str) -> TraceResult<()> {
        self.record(
            RecordType::TaskStart,
            None,
            serde_json::json!({
                "task_id": task_id,
                "description": description,
            }),
        );
        Ok(())
    }

    /// Record a tool call event
    pub fn record_tool_call(&mut self, tool_name: &str, arguments: &serde_json::Value) {
        self.record(
            RecordType::ToolCall,
            None,
            serde_json::json!({
                "tool": tool_name,
                "arguments": arguments,
            }),
        );
    }

    /// Record a tool result event
    pub fn record_tool_result(&mut self, tool_name: &str, content: &str, success: bool) {
        self.record(
            RecordType::ToolResult,
            None,
            serde_json::json!({
                "tool": tool_name,
                "content": content,
                "success": success,
            }),
        );
    }

    /// Record an LLM request event
    pub fn record_llm_request(&mut self, provider: &str, model: &str, message_count: usize) {
        self.record(
            RecordType::LlmRequest,
            None,
            serde_json::json!({
                "provider": provider,
                "model": model,
                "message_count": message_count,
            }),
        );
    }

    /// Record an LLM response event
    pub fn record_llm_response(&mut self, provider: &str, model: &str) {
        self.record(
            RecordType::LlmResponse,
            None,
            serde_json::json!({
                "provider": provider,
                "model": model,
            }),
        );
    }

    /// Record task completion
    pub fn record_task_complete(&mut self, task_id: &str, summary: &str) {
        self.record(
            RecordType::TaskComplete,
            None,
            serde_json::json!({
                "task_id": task_id,
                "summary": summary,
            }),
        );
    }

    /// Record task failure
    pub fn record_task_failed(&mut self, task_id: &str, error: &str) {
        self.record(
            RecordType::TaskFailed,
            None,
            serde_json::json!({
                "task_id": task_id,
                "error": error,
            }),
        );
    }

    /// Record a policy decision
    pub fn record_policy_decision(&mut self, tool_name: &str, decision: &str, reason: &str) {
        self.record(
            RecordType::PolicyDecision,
            None,
            serde_json::json!({
                "tool": tool_name,
                "decision": decision,
                "reason": reason,
            }),
        );
    }

    pub fn flush(&self) -> TraceResult<()> {
        let path = self.output_path.join(format!("{}.json", self.session_id));
        let content = serde_json::to_string_pretty(&self.records)
            .map_err(|e| crate::error::TraceError::Recording(e.to_string()))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    pub fn record_sandbox_created(&mut self, id: &str, mode: &str) {
        self.record(
            RecordType::SandboxCreated,
            None,
            serde_json::json!({ "id": id, "mode": mode }),
        );
    }

    pub fn record_sandbox_destroyed(&mut self, id: &str) {
        self.record(
            RecordType::SandboxDestroyed,
            None,
            serde_json::json!({ "id": id }),
        );
    }

    pub fn record_patch_proposed(&mut self, id: &str, files_count: usize) {
        self.record(
            RecordType::PatchProposed,
            None,
            serde_json::json!({ "sandbox_id": id, "files": files_count }),
        );
    }

    pub fn record_patch_merged(&mut self, id: &str) {
        self.record(
            RecordType::PatchMerged,
            None,
            serde_json::json!({ "sandbox_id": id }),
        );
    }
}
