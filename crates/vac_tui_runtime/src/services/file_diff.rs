//! File Diff Rendering
//!
//! Minimal implementation for showing file diffs in TUI

use crate::services::theme::{StyleKey, Theme};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// Render a simple diff between old and new content
pub fn render_diff(theme: &Theme, old_content: &str, new_content: &str, _max_width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Header
    lines.push(Line::from(vec![Span::styled(
        "--- Old",
        theme.style(StyleKey::DiffRemoved).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(vec![Span::styled(
        "+++ New",
        theme.style(StyleKey::DiffAdded).add_modifier(Modifier::BOLD),
    )]));

    // Simple line-by-line comparison (naive)
    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();
    
    // Very basic diff for now
    for line in old_lines {
        lines.push(Line::from(vec![Span::styled(
            format!("- {}", line),
            theme.style(StyleKey::DiffRemoved),
        )]));
    }
    for line in new_lines {
        lines.push(Line::from(vec![Span::styled(
            format!("+ {}", line),
            theme.style(StyleKey::DiffAdded),
        )]));
    }

    lines
}
