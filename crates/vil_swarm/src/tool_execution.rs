//! Tool execution lane classification and status helpers.
//! Extracted from orchestrator.rs — generic control-plane, not VIL semantic policy.

use vil_llm::provider::ToolCall;

/// Lane classification for tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolLane {
    /// Parallel-safe read operations (Data Lane).
    Data,
    /// Serial write operations (Control Lane).
    Control,
}

/// Classify a tool into its execution lane.
pub fn classify_tool_lane(name: &str) -> ToolLane {
    match name {
        "file_read" | "glob" | "grep" | "search" | "vil_knowledge" | "vil_diagnostics"
        | "vil_lsp_query" => ToolLane::Data,
        _ => ToolLane::Control,
    }
}

/// Human-readable status string for a tool call in progress.
pub fn status_for_tool(name: &str) -> String {
    match name {
        "bash" => "Running shell command".to_string(),
        "cargo" => "Running cargo task".to_string(),
        "git" => "Running git command".to_string(),
        "file_edit" => "Editing files".to_string(),
        "file_write" => "Writing files".to_string(),
        "todo_write" => "Updating task list".to_string(),
        "task_done" => "Marking task complete".to_string(),
        other => format!("Using {}", other),
    }
}

/// Partition a list of tool calls into (parallel_reads, serial_writes).
pub fn partition_calls(calls: Vec<ToolCall>) -> (Vec<ToolCall>, Vec<ToolCall>) {
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    for call in calls {
        match classify_tool_lane(&call.name) {
            ToolLane::Data => reads.push(call),
            ToolLane::Control => writes.push(call),
        }
    }
    (reads, writes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_tools_go_to_data_lane() {
        assert_eq!(classify_tool_lane("file_read"), ToolLane::Data);
        assert_eq!(classify_tool_lane("glob"), ToolLane::Data);
        assert_eq!(classify_tool_lane("vil_knowledge"), ToolLane::Data);
    }

    #[test]
    fn write_tools_go_to_control_lane() {
        assert_eq!(classify_tool_lane("file_write"), ToolLane::Control);
        assert_eq!(classify_tool_lane("bash"), ToolLane::Control);
        assert_eq!(classify_tool_lane("unknown_tool"), ToolLane::Control);
    }

    #[test]
    fn partition_splits_correctly() {
        use serde_json::json;
        let calls = vec![
            ToolCall {
                id: "1".into(),
                name: "file_read".into(),
                arguments: json!({}),
            },
            ToolCall {
                id: "2".into(),
                name: "file_write".into(),
                arguments: json!({}),
            },
            ToolCall {
                id: "3".into(),
                name: "glob".into(),
                arguments: json!({}),
            },
        ];
        let (reads, writes) = partition_calls(calls);
        assert_eq!(reads.len(), 2);
        assert_eq!(writes.len(), 1);
    }
}
