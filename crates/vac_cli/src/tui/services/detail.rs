//! Detail panel service — multi-mode contextual panel.
//! VAC-native, not copied from Stakpak.

use ratatui::text::Line;
use vac_core::engine::TaskHistoryEntry;
use vac_core::TaskStatus;

/// Detail panel mode.
#[derive(Clone, PartialEq)]
/// Detail panel mode — state machine for right panel content.
/// 
/// State transitions:
/// - None → TaskDetail (Enter on history)
/// - None → RevertConfirm (R on history)
/// - TaskDetail → RevertConfirm (R in detail)
/// - TaskDetail → None (Esc)
/// - RevertConfirm → None (Esc or n)
/// - RevertConfirm → None (y, after revert)
/// - Any → LiveChanges (on write tool call)
/// - LiveChanges → None (Esc)
pub enum DetailMode {
    None,
    TaskDetail(usize),
    ErrorDetail(String),
    RevertConfirm(usize),
    LiveChanges,
}

impl Default for DetailMode {
    fn default() -> Self { Self::None }
}

impl DetailMode {
    pub fn is_some(&self) -> bool {
        !matches!(self, Self::None)
    }
}

/// Render detail content based on mode.
/// Returns (lines, total_rows) for scroll calculation.
pub fn render_detail_content(
    mode: &DetailMode,
    history: &[TaskHistoryEntry],
    live_files: &[String],
    project_root: &std::path::Path,
    max_width: usize,
) -> (Vec<Line<'static>>, usize) {
    match mode {
        DetailMode::None => (vec![], 0),
        DetailMode::LiveChanges => {
            let content = if live_files.is_empty() {
                "Waiting for writes...".to_string()
            } else {
                format!("✏️  Writing:\n{}", live_files.join("\n"))
            };
            let lines: Vec<Line> = content.lines()
                .map(|l| wrap_line(l, max_width))
                .flatten()
                .collect();
            let total = lines.len();
            (lines, total)
        }
        DetailMode::ErrorDetail(reason) => {
            let content = format!("❌ {}\n\nPress Esc to dismiss.", reason);
            let lines: Vec<Line> = content.lines()
                .map(|l| wrap_line(l, max_width))
                .flatten()
                .collect();
            let total = lines.len();
            (lines, total)
        }
        DetailMode::TaskDetail(idx) => {
            let lines = if let Some(e) = history.get(*idx) {
                let status = match &e.status {
                    TaskStatus::Completed => "✓ Completed".to_string(),
                    TaskStatus::Failed(r) => format!("✗ {r}"),
                    other => format!("{other:?}"),
                };
                let content = format!("{}\n\n{}\nTokens: {}\n\n[R] Revert  [Esc] Close", e.description, status, e.total_tokens_used);
                content.lines().map(|l| wrap_line(l, max_width)).flatten().collect()
            } else {
                vec![Line::from("No task selected")]
            };
            let total = lines.len();
            (lines, total)
        }
        DetailMode::RevertConfirm(idx) => {
            let desc = history.get(*idx).map(|e| e.description.as_str()).unwrap_or("?");
            let task_id = history.get(*idx).map(|e| e.task_id.to_string()).unwrap_or_default();
            
            // Load snapshot manifest for preview
            let manifest_path = project_root.join(".vac/snapshots").join(format!("{}.manifest.json", task_id));
            let preview = if manifest_path.exists() {
                match vac_core::snapshot::SnapshotManifest::load(&manifest_path) {
                    Ok(manifest) => {
                        let file_count = manifest.files.len();
                        let file_list: Vec<String> = manifest.files.iter()
                            .take(5)
                            .map(|f| format!("  • {}", f.original_path.display()))
                            .collect();
                        let more = if file_count > 5 { format!("\n  ... and {} more", file_count - 5) } else { String::new() };
                        format!("\nFiles to restore ({}):\n{}{}", file_count, file_list.join("\n"), more)
                    }
                    Err(_) => "\n⚠ Manifest not found".to_string(),
                }
            } else {
                "\n⚠ No snapshot available".to_string()
            };
            
            let truncated: String = desc.chars().take(60).collect();
            let content = format!("Revert to before:\n\"{}\"{}\n\n[y] Confirm  [n/Esc] Cancel", truncated, preview);
            let lines: Vec<Line> = content.lines()
                .map(|l| wrap_line(l, max_width))
                .flatten()
                .collect();
            let total = lines.len();
            (lines, total)
        }
    }
}

/// Wrap a line to max_width, returning multiple lines if needed.
fn wrap_line(text: &str, max_width: usize) -> Vec<Line<'static>> {
    if max_width == 0 {
        return vec![Line::from(text.to_string())];
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_width {
        return vec![Line::from(text.to_string())];
    }
    let mut lines = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let end = (start + max_width).min(chars.len());
        let seg: String = chars[start..end].iter().collect();
        lines.push(Line::from(seg));
        start = end;
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use vac_core::engine::TaskHistoryEntry;
    use vac_core::TaskStatus;
    use std::path::PathBuf;

    #[test]
    fn test_detail_mode_is_some() {
        assert!(!DetailMode::None.is_some());
        assert!(DetailMode::TaskDetail(0).is_some());
        assert!(DetailMode::ErrorDetail("test".into()).is_some());
        assert!(DetailMode::RevertConfirm(0).is_some());
        assert!(DetailMode::LiveChanges.is_some());
    }

    #[test]
    fn test_render_detail_content_wrapping() {
        let long_text = "a".repeat(100);
        let mode = DetailMode::ErrorDetail(long_text);
        let (lines, total) = render_detail_content(&mode, &[], &[], &PathBuf::from("/tmp"), 50);
        assert!(total > 2); // Should wrap into multiple lines
        assert!(lines.len() > 2);
    }

    #[test]
    fn test_revert_confirm_preview() {
        let entry = TaskHistoryEntry {
            task_id: uuid::Uuid::new_v4(),
            description: "Test task".into(),
            status: TaskStatus::Completed,
            total_tokens_used: 100,
            updated_at: chrono::Utc::now(),
            summary: None,
        };
        let mode = DetailMode::RevertConfirm(0);
        let (lines, _) = render_detail_content(&mode, &[entry], &[], &PathBuf::from("/tmp"), 80);
        let content: String = lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n");
        assert!(content.contains("Revert to before"));
        assert!(content.contains("[y] Confirm"));
    }
}
