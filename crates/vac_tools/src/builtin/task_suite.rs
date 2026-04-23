//! F1.5 — Task suite: `task_create`, `task_list`, `task_stop`,
//! `task_output`.
//!
//! Claude Code pattern: background task management as a suite of
//! tools, not a TUI tab. Operators and agents both reach the same
//! task registry via the same protocol.
//!
//! Registry is a single TOML file under `.vac/tasks/index.toml` plus
//! per-task output log at `.vac/tasks/<id>/output.log`. Tasks are not
//! spawned by these tools directly — they create registry entries
//! that the `vac_runtime` scheduler picks up. Keeps this crate free of
//! a tokio runtime dependency for spawn.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::registry::{ToolContext, VilTool};

const INDEX_FILE: &str = ".vac/tasks/index.toml";
const TASKS_DIR: &str = ".vac/tasks";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TaskState {
    Queued,
    Running,
    Completed,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskEntry {
    id: String,
    title: String,
    prompt: String,
    state: TaskState,
    created_at_epoch_s: u64,
    created_by_session: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rulebook: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct TaskIndex {
    #[serde(default)]
    tasks: Vec<TaskEntry>,
}

fn index_path(ctx: &ToolContext) -> std::path::PathBuf {
    ctx.working_dir.join(INDEX_FILE)
}

fn task_dir(ctx: &ToolContext, id: &str) -> std::path::PathBuf {
    ctx.working_dir.join(TASKS_DIR).join(id)
}

fn load_index(ctx: &ToolContext) -> Result<TaskIndex, ToolError> {
    let path = index_path(ctx);
    if !path.exists() {
        return Ok(TaskIndex::default());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| ToolError::ExecutionFailed(format!("read index: {e}")))?;
    toml::from_str(&content).map_err(|e| ToolError::ExecutionFailed(format!("parse index: {e}")))
}

fn save_index(ctx: &ToolContext, idx: &TaskIndex) -> Result<(), ToolError> {
    let path = index_path(ctx);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ToolError::ExecutionFailed(format!("mkdir: {e}")))?;
    }
    let serialized = toml::to_string_pretty(idx)
        .map_err(|e| ToolError::ExecutionFailed(format!("serialize: {e}")))?;
    std::fs::write(&path, serialized)
        .map_err(|e| ToolError::ExecutionFailed(format!("write index: {e}")))
}

// ── task_create ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CreateInput {
    title: String,
    prompt: String,
    #[serde(default)]
    rulebook: Option<String>,
}

pub struct TaskCreateTool;

impl TaskCreateTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskCreateTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for TaskCreateTool {
    fn name(&self) -> &str {
        "task_create"
    }
    fn description(&self) -> &str {
        "Queue a background task for the runtime scheduler. Returns a task id usable with task_list / task_stop / task_output."
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "title": {"type": "string"},
                "prompt": {"type": "string"},
                "rulebook": {"type": "string"},
            },
            "required": ["title", "prompt"]
        })
    }
    fn trust_requirement(&self) -> &str {
        "ask_once"
    }
    fn risk_level(&self) -> &str {
        "mutating"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let input: CreateInput = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid arguments: {e}")))?;
        let id = uuid::Uuid::new_v4().to_string();
        let mut idx = load_index(context)?;
        idx.tasks.push(TaskEntry {
            id: id.clone(),
            title: input.title.clone(),
            prompt: input.prompt.clone(),
            state: TaskState::Queued,
            created_at_epoch_s: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            created_by_session: context.session_id.to_string(),
            rulebook: input.rulebook,
        });
        save_index(context, &idx)?;

        let _ = std::fs::create_dir_all(task_dir(context, &id));
        Ok(serde_json::json!({
            "id": id,
            "title": input.title,
            "state": "queued",
        }))
    }
}

// ── task_list ───────────────────────────────────────────────────────

pub struct TaskListTool;

impl TaskListTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskListTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for TaskListTool {
    fn name(&self) -> &str {
        "task_list"
    }
    fn description(&self) -> &str {
        "List all background tasks with state and title. Pair with task_output to read logs."
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "state": { "type": "string", "description": "Optional filter: queued|running|completed|failed|stopped." }
            },
            "required": []
        })
    }
    fn trust_requirement(&self) -> &str {
        "safe"
    }
    fn risk_level(&self) -> &str {
        "safe"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let filter = args
            .get("state")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let idx = load_index(context)?;
        let tasks: Vec<_> = idx
            .tasks
            .into_iter()
            .filter(|t| match &filter {
                Some(f) => {
                    let state_str = match &t.state {
                        TaskState::Queued => "queued",
                        TaskState::Running => "running",
                        TaskState::Completed => "completed",
                        TaskState::Failed => "failed",
                        TaskState::Stopped => "stopped",
                    };
                    state_str == f
                }
                None => true,
            })
            .map(|t| {
                serde_json::json!({
                    "id": t.id,
                    "title": t.title,
                    "state": match t.state {
                        TaskState::Queued => "queued",
                        TaskState::Running => "running",
                        TaskState::Completed => "completed",
                        TaskState::Failed => "failed",
                        TaskState::Stopped => "stopped",
                    },
                    "created_at_epoch_s": t.created_at_epoch_s,
                })
            })
            .collect();
        Ok(serde_json::json!({ "tasks": tasks, "total": tasks.len() }))
    }
}

// ── task_stop ───────────────────────────────────────────────────────

pub struct TaskStopTool;

impl TaskStopTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskStopTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for TaskStopTool {
    fn name(&self) -> &str {
        "task_stop"
    }
    fn description(&self) -> &str {
        "Mark a task as stopped. The runtime scheduler observes the state change and terminates the executor at the next checkpoint."
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" }
            },
            "required": ["id"]
        })
    }
    fn trust_requirement(&self) -> &str {
        "ask_once"
    }
    fn risk_level(&self) -> &str {
        "mutating"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::ExecutionFailed("missing id".into()))?
            .to_string();
        let mut idx = load_index(context)?;
        let task = idx
            .tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| ToolError::ExecutionFailed(format!("task {id} not found")))?;
        task.state = TaskState::Stopped;
        save_index(context, &idx)?;
        Ok(serde_json::json!({ "id": id, "state": "stopped" }))
    }
}

// ── task_output ─────────────────────────────────────────────────────

pub struct TaskOutputTool;

impl TaskOutputTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskOutputTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VilTool for TaskOutputTool {
    fn name(&self) -> &str {
        "task_output"
    }
    fn description(&self) -> &str {
        "Read the tail of a task's output log. Returns empty content if the task hasn't started yet."
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "n": { "type": "integer", "description": "Tail size in lines (default 50)." }
            },
            "required": ["id"]
        })
    }
    fn trust_requirement(&self) -> &str {
        "safe"
    }
    fn risk_level(&self) -> &str {
        "safe"
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<serde_json::Value, ToolError> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::ExecutionFailed("missing id".into()))?
            .to_string();
        let n = args.get("n").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

        let log_path = task_dir(context, &id).join("output.log");
        if !log_path.exists() {
            return Ok(serde_json::json!({
                "id": id,
                "lines": [],
                "empty_reason": "no output yet",
            }));
        }
        let content = std::fs::read_to_string(&log_path)
            .map_err(|e| ToolError::ExecutionFailed(format!("read log: {e}")))?;
        let lines: Vec<&str> = content.lines().collect();
        let skip = lines.len().saturating_sub(n);
        let tail: Vec<String> = lines.iter().skip(skip).map(|s| s.to_string()).collect();
        Ok(serde_json::json!({
            "id": id,
            "lines": tail,
            "total_lines": lines.len(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::test_util::make_ctx;

    #[tokio::test]
    async fn create_then_list_shows_task() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());

        TaskCreateTool::new()
            .execute(
                serde_json::json!({"title": "refactor foo", "prompt": "do thing"}),
                &ctx,
            )
            .await
            .unwrap();

        let out = TaskListTool::new()
            .execute(serde_json::json!({}), &ctx)
            .await
            .unwrap();
        assert_eq!(out["total"], 1);
        assert_eq!(out["tasks"][0]["title"], "refactor foo");
        assert_eq!(out["tasks"][0]["state"], "queued");
    }

    #[tokio::test]
    async fn stop_updates_state() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());

        let out = TaskCreateTool::new()
            .execute(
                serde_json::json!({"title": "x", "prompt": "y"}),
                &ctx,
            )
            .await
            .unwrap();
        let id = out["id"].as_str().unwrap().to_string();

        TaskStopTool::new()
            .execute(serde_json::json!({"id": id}), &ctx)
            .await
            .unwrap();

        let listed = TaskListTool::new()
            .execute(serde_json::json!({}), &ctx)
            .await
            .unwrap();
        assert_eq!(listed["tasks"][0]["state"], "stopped");
    }

    #[tokio::test]
    async fn output_empty_when_no_log() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        let out = TaskOutputTool::new()
            .execute(serde_json::json!({"id": "nonexistent"}), &ctx)
            .await
            .unwrap();
        assert!(out["empty_reason"].is_string());
    }

    #[tokio::test]
    async fn filter_by_state() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = make_ctx(tmp.path().to_path_buf(), uuid::Uuid::new_v4());
        TaskCreateTool::new()
            .execute(serde_json::json!({"title": "a", "prompt": "a"}), &ctx)
            .await
            .unwrap();
        let out = TaskCreateTool::new()
            .execute(serde_json::json!({"title": "b", "prompt": "b"}), &ctx)
            .await
            .unwrap();
        let id = out["id"].as_str().unwrap().to_string();
        TaskStopTool::new()
            .execute(serde_json::json!({"id": id}), &ctx)
            .await
            .unwrap();

        let queued = TaskListTool::new()
            .execute(serde_json::json!({"state": "queued"}), &ctx)
            .await
            .unwrap();
        assert_eq!(queued["total"], 1);

        let stopped = TaskListTool::new()
            .execute(serde_json::json!({"state": "stopped"}), &ctx)
            .await
            .unwrap();
        assert_eq!(stopped["total"], 1);
    }
}
