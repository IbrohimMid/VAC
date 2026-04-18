//! Todo Extractor
//!
//! Extracts todo items from `<todo>` XML tags in assistant messages and
//! parses them into `TodoItem` structs for display in the side panel.

use vac_changeset::{TodoItem, TodoStatus};

fn extract_todos_from_xml(text: &str) -> Option<String> {
    let start_tag = "<todo>";
    let end_tag = "</todo>";
    let start_idx = text.find(start_tag)?;
    let content_start = start_idx + start_tag.len();
    let end_idx = text[content_start..].find(end_tag)?;
    Some(text[content_start..content_start + end_idx].to_string())
}

fn parse_todo_line(line: &str) -> Option<TodoItem> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    if line.starts_with("- [ ]") {
        let text = line.strip_prefix("- [ ]")?.trim().to_string();
        if text.is_empty() {
            return None;
        }
        Some(TodoItem::new(text).with_status(TodoStatus::Pending))
    } else if line.starts_with("- [x]") || line.starts_with("- [X]") {
        let text = line
            .strip_prefix("- [x]")
            .or_else(|| line.strip_prefix("- [X]"))?
            .trim()
            .to_string();
        if text.is_empty() {
            return None;
        }
        Some(TodoItem::new(text).with_status(TodoStatus::Done))
    } else if line.starts_with("- [/]") {
        let text = line.strip_prefix("- [/]")?.trim().to_string();
        if text.is_empty() {
            return None;
        }
        Some(TodoItem::new(text).with_status(TodoStatus::InProgress))
    } else {
        None
    }
}

/// Extract todos from a text containing `<todo>...</todo>`. The first pending
/// item is promoted to `InProgress` so the UI has a clear "current" marker.
pub fn extract_todos(text: &str) -> Vec<TodoItem> {
    let mut todos = Vec::new();
    if let Some(body) = extract_todos_from_xml(text) {
        for line in body.lines() {
            if let Some(item) = parse_todo_line(line) {
                todos.push(item);
            }
        }
    }
    let mut found_first_pending = false;
    for todo in &mut todos {
        if todo.status != TodoStatus::Done {
            if !found_first_pending {
                todo.status = TodoStatus::InProgress;
                found_first_pending = true;
            } else {
                todo.status = TodoStatus::Pending;
            }
        }
    }
    todos
}
