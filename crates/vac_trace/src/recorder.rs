//! Session recorder — captures task events, tool calls, and messages.

use crate::error::TraceResult;
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
        })
    }

    pub fn record(
        &mut self,
        record_type: RecordType,
        agent_id: Option<&str>,
        content: serde_json::Value,
    ) {
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
}
