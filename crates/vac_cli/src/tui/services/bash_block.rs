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

/// Extract all fenced code blocks from text.
/// Supports ```bash, ```sh, ```shell, and plain ``` fences.
pub fn extract_bash_blocks(text: &str) -> Vec<BashBlock> {
    let mut blocks = Vec::new();
    let mut in_block = false;
    let mut lang = String::new();
    let mut current = Vec::new();

    for line in text.lines() {
        if !in_block {
            if let Some(fence) = line.strip_prefix("```") {
                let l = fence.trim().to_lowercase();
                if l.is_empty() || matches!(l.as_str(), "bash" | "sh" | "shell") {
                    in_block = true;
                    lang = if l.is_empty() { "bash".to_string() } else { l };
                    current.clear();
                }
            }
        } else if line.trim() == "```" {
            blocks.push(BashBlock {
                language: lang.clone(),
                content: current.join("\n"),
            });
            in_block = false;
            current.clear();
        } else {
            current.push(line.to_string());
        }
    }

    blocks
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
        let truncated = if line.len() > width.saturating_sub(2) {
            format!("{}…", &line[..width.saturating_sub(3)])
        } else {
            line.to_string()
        };
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
    extract_bash_blocks(text)
        .iter()
        .flat_map(|b| render_bash_block(b, width))
        .collect()
}
