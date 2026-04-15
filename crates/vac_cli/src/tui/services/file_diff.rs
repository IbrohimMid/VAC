//! File Diff Rendering
//!
//! Minimal implementation for showing file diffs in TUI

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Render a simple diff between old and new content
pub fn render_diff(old_content: &str, new_content: &str, max_width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Header
    lines.push(Line::from(vec![Span::styled(
        "--- Old",
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(vec![Span::styled(
        "+++ New",
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    // Simple line-by-line comparison using `similar`
    let diff = similar::TextDiff::from_lines(old_content, new_content);

    for change in diff.iter_all_changes() {
        let line = change.value();
        let truncated = truncate_line(line.trim_end_matches('\n'), max_width.saturating_sub(2));

        match change.tag() {
            similar::ChangeTag::Delete => {
                lines.push(Line::from(vec![
                    Span::styled("- ", Style::default().fg(Color::Red)),
                    Span::styled(truncated, Style::default().fg(Color::Red)),
                ]));
            }
            similar::ChangeTag::Insert => {
                lines.push(Line::from(vec![
                    Span::styled("+ ", Style::default().fg(Color::Green)),
                    Span::styled(truncated, Style::default().fg(Color::Green)),
                ]));
            }
            similar::ChangeTag::Equal => {
                lines.push(Line::from(vec![Span::raw("  "), Span::raw(truncated)]));
            }
        }
    }

    lines
}

fn truncate_line(line: &str, max_width: usize) -> String {
    if line.len() <= max_width {
        line.to_string()
    } else {
        format!("{}...", &line[..max_width.saturating_sub(3)])
    }
}

/// Preview diff for a file operation
pub fn preview_file_diff(
    file_path: &str,
    old_content: &str,
    new_content: &str,
    max_width: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // File header - clone to make 'static
    lines.push(Line::from(vec![
        Span::styled("File: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(file_path.to_string(), Style::default().fg(Color::Cyan)),
    ]));
    lines.push(Line::from(""));

    // Diff content
    lines.extend(render_diff(old_content, new_content, max_width));

    lines
}
