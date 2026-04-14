//! VAC-native bash block detection and rendering.
//!
//! Detects fenced bash/shell code blocks in text and renders them
//! with appropriate styling for TUI display.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// A detected bash block extracted from text.
#[derive(Debug, Clone)]
pub struct BashBlock {
    pub language: String,
    pub content: String,
}

/// Truncate a string to max_chars, respecting UTF-8 character boundaries.
fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        s.chars().take(max_chars.saturating_sub(1)).chain(std::iter::once('…')).collect()
    }
}

/// Render a bash block as styled TUI lines.
pub fn render_bash_block(block: &BashBlock, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Header
    lines.push(Line::from(vec![
        Span::styled("┌─ ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            block.language.clone(),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}", "─".repeat(width.saturating_sub(block.language.len() + 4))),
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    // Content lines
    for line in block.content.lines() {
        let truncated = truncate_chars(line, width.saturating_sub(2));
        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled(truncated, Style::default().fg(Color::Cyan)),
        ]));
    }

    // Footer
    lines.push(Line::from(Span::styled(
        format!("└{}", "─".repeat(width.saturating_sub(1))),
        Style::default().fg(Color::DarkGray),
    )));

    lines
}

/// Render all bash blocks found in text.
pub fn render_bash_blocks_in_text(text: &str, width: usize) -> Vec<Line<'static>> {
    use super::message::split_content_segments;
    split_content_segments(text)
        .into_iter()
        .filter_map(|seg| match seg {
            super::message::ContentSegment::Code { language, content } => {
                Some(render_bash_block(&BashBlock { language, content }, width))
            }
            _ => None,
        })
        .flatten()
        .collect()
}
