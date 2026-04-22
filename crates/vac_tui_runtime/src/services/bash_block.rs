//! VAC-native bash block detection and rendering.
//!
//! Detects fenced bash/shell code blocks in text and renders them
//! with appropriate styling for TUI display.

use crate::services::theme::{StyleKey, Theme};
use ratatui::style::{Modifier, Style};
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
        s.chars()
            .take(max_chars.saturating_sub(1))
            .chain(std::iter::once('…'))
            .collect()
    }
}

/// Render a bash block as styled TUI lines.
pub fn render_bash_block(theme: &Theme, block: &BashBlock, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Header
    lines.push(Line::from(vec![
        Span::styled("┌─ ", theme.style(StyleKey::Muted)),
        Span::styled(
            block.language.clone(),
            theme.style(StyleKey::Warning).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                " {}",
                "─".repeat(width.saturating_sub(block.language.len() + 4))
            ),
            theme.style(StyleKey::Muted),
        ),
    ]));

    // Content lines
    for line in block.content.lines() {
        let truncated = truncate_chars(line, width.saturating_sub(2));
        lines.push(Line::from(vec![
            Span::styled("│ ", theme.style(StyleKey::Muted)),
            Span::styled(truncated, theme.style(StyleKey::Accent)),
        ]));
    }

    // Footer
    lines.push(Line::from(Span::styled(
        format!("└{}", "─".repeat(width.saturating_sub(1))),
        theme.style(StyleKey::Muted),
    )));

    lines
}

/// Render all bash blocks found in text.
pub fn render_bash_blocks_in_text(theme: &Theme, text: &str, width: usize) -> Vec<Line<'static>> {
    use super::message::split_content_segments;
    split_content_segments(text)
        .into_iter()
        .filter_map(|seg| match seg {
            super::message::ContentSegment::Code { language, content } => Some(render_bash_block(
                theme,
                &BashBlock { language, content },
                width,
            )),
            _ => None,
        })
        .flatten()
        .collect()
}
