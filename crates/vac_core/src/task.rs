//! Task representation and lifecycle.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

/// Task priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Normal
    }
}

/// A task submitted to the VAC engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub description: String,
    pub priority: Priority,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Optional constraints (e.g., target files, modules)
    pub constraints: TaskConstraints,
    /// Parent task ID if this is a subtask
    pub parent_task: Option<TaskId>,
}

impl Task {
    pub fn new(description: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: TaskId::new(),
            description: description.into(),
            priority: Priority::default(),
            status: TaskStatus::Pending,
            created_at: now,
            updated_at: now,
            constraints: TaskConstraints::default(),
            parent_task: None,
        }
    }

    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_constraints(mut self, constraints: TaskConstraints) -> Self {
        self.constraints = constraints;
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskConstraints {
    /// Restrict to specific files/directories
    pub target_paths: Vec<String>,
    /// Restrict to specific modules
    pub target_modules: Vec<String>,
    /// Maximum number of files to modify
    pub max_files_modified: Option<usize>,
    /// Require human approval before applying changes
    pub require_approval: bool,
}

/// Task execution status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Planning,
    Executing,
    Validating,
    Completed,
    Failed(String),
    Cancelled,
}

/// Result of a completed task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: TaskId,
    pub status: TaskStatus,
    pub summary: String,
    /// Files that were modified
    pub modified_files: Vec<String>,
    /// Files that were created
    pub created_files: Vec<String>,
    /// Validation results (from IR validation passes)
    pub validation_score: Option<f64>,
    /// Time elapsed in milliseconds
    pub elapsed_ms: u64,
    /// Token usage across all LLM calls
    pub total_tokens_used: u64,
    /// Agent contributions
    pub agent_contributions: Vec<AgentContribution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContribution {
    pub agent_id: String,
    pub agent_role: String,
    pub actions_taken: Vec<String>,
    pub tokens_used: u64,
}
