//! VIL Agent Protocol (VAP) — Tri-Lane message types.

use crate::agent::AgentId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub type TaskId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VapMessage {
    Trigger(TriggerPayload),
    Data(DataPayload),
    Control(ControlPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerPayload {
    pub task_id: TaskId,
    pub source_agent: AgentId,
    pub target_agent: AgentId,
    pub task_type: TaskType,
    pub priority: u8,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    Plan,
    Implement,
    Test,
    Review,
    Document,
    Validate,
    Deploy,
    Monitor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPayload {
    pub task_id: TaskId,
    pub source_agent: AgentId,
    pub content_type: ContentType,
    pub data: String,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentType {
    Code,
    Ir,
    Patch,
    TestResult,
    AnalysisReport,
    Documentation,
    Plan,
    Raw,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlPayload {
    pub task_id: TaskId,
    pub source_agent: AgentId,
    pub event: ControlEvent,
    pub severity: Severity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlEvent {
    Heartbeat,
    Error(String),
    MetricsSnapshot(HashMap<String, f64>),
    Checkpoint,
    TaskComplete,
    TaskFailed(String),
    AgentPaused,
    AgentResumed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Critical,
}
