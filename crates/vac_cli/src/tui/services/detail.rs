//! Detail panel service — multi-mode contextual panel.
//! VAC-native, not copied from Stakpak.

use ratatui::text::Line;
use vac_core::engine::TaskHistoryEntry;
use vac_core::TaskStatus;

/// Detail panel mode.
#[derive(Clone, PartialEq)]
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
