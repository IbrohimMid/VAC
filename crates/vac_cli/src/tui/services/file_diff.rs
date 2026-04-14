//! File Diff Rendering
//!
//! Minimal implementation for showing file diffs in TUI

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Render a simple diff between old and new content
pub fn render_diff(old_content: &str, new_content: &str, max_width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    
    // Header
    lines.push(Line::from(vec![
        Span::styled("--- Old", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("+++ New", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(""));
    
    // Simple line-by-line comparison
    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();
    
    let max_lines = old_lines.len().max(new_lines.len());
    
    for i in 0..max_lines {
        let old_line = old_lines.get(i).copied().unwrap_or("");
        let new_line = new_lines.get(i).copied().unwrap_or("");
        
        if old_line != new_line {
            // Show removed line
            if !old_line.is_empty() {
                let truncated = truncate_line(old_line, max_width.saturating_sub(2));
                lines.push(Line::from(vec![
                    Span::styled("- ", Style::default().fg(Color::Red)),
                    Span::styled(truncated, Style::default().fg(Color::Red)),
                ]));
            }
            
            // Show added line
            if !new_line.is_empty() {
                let truncated = truncate_line(new_line, max_width.saturating_sub(2));
                lines.push(Line::from(vec![
                    Span::styled("+ ", Style::default().fg(Color::Green)),
                    Span::styled(truncated, Style::default().fg(Color::Green)),
                ]));
            }
        } else if !old_line.is_empty() {
            // Context line (unchanged)
            let truncated = truncate_line(old_line, max_width.saturating_sub(2));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::raw(truncated),
            ]));
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
