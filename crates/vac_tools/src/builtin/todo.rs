use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: TodoStatus,
    pub active_form: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoWriteInput {
    pub todos: Vec<TodoItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoWriteOutput {
    pub updated_count: usize,
    pub todos: Vec<TodoItem>,
}

#[derive(Debug, Clone, Default)]
pub struct TodoTool {
    todos: Arc<DashMap<String, TodoItem>>,
}

#[async_trait]
impl crate::registry::VilTool for TodoTool {
    fn spec(&self) -> vac_tool_core::ToolSpec {
        crate::registry::default_spec(self)
    }

    fn name(&self) -> &'static str {
        "todo_write"
    }

    fn description(&self) -> &'static str {
        "Write or update agent todo list"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {"type": "string"},
                            "content": {"type": "string"},
                            "status": {"type": "string", "enum": ["Pending", "InProgress", "Completed", "Cancelled"]},
                            "active_form": {"type": "string"}
                        }
                    }
                }
            },
            "required": ["todos"]
        })
    }

    fn trust_requirement(&self) -> &'static str {
        "agent"
    }

    fn risk_level(&self) -> &'static str {
        "safe"
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _context: &crate::registry::ToolContext,
    ) -> Result<serde_json::Value, crate::error::ToolError> {
        let input: TodoWriteInput = serde_json::from_value(input)?;
        let mut updated_count = 0;
        let mut todos = Vec::new();

        for mut todo in input.todos {
            if todo.id.is_empty() {
                todo.id = Uuid::new_v4().to_string();
            }
            self.todos.insert(todo.id.clone(), todo.clone());
            todos.push(todo);
            updated_count += 1;
        }

        debug!(updated = updated_count, "Todo write completed");

        let output = TodoWriteOutput {
            updated_count,
            todos,
        };

        Ok(serde_json::to_value(output)?)
    }
}
