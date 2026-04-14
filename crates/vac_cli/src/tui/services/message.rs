//! VAC-native message rendering module.
//!
//! Provides formatting and rendering for TUI message bubbles.
//! Uses VAC types directly (no stakpak_shared dependency).

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::tui::types::{ToolCall, ToolCallResult, ToolCallResultStatus};
use crate::tui::services::render_markdown_to_lines_safe;

/// Truncate a string to max_chars, respecting UTF-8 character boundaries.
fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        s.chars().take(max_chars.saturating_sub(1)).chain(std::iter::once('…')).collect()
    }
}

/// Render a user message as styled lines with cyan prefix bar.
pub fn render_user_message(content: &str, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("▌ ", Style::default().fg(Color::Cyan)),
        Span::styled("You", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ]));
    for line in content.lines() {
        let truncated = truncate_chars(line, width);
        lines.push(Line::from(vec![
            Span::styled("▌ ", Style::default().fg(Color::Cyan)),
            Span::raw(truncated),
        ]));
    }
    lines
}

/// A segment of content — either text or a code block.
#[derive(Debug, Clone)]
pub enum ContentSegment {
    Text(String),
    Code { language: String, content: String },
}

/// Split content into segments, detecting fenced code blocks in a single pass.
pub fn split_content_segments(content: &str) -> Vec<ContentSegment> {
    let mut segments = Vec::new();
    let mut in_block = false;
    let mut current_text = String::new();
    let mut lang = String::new();
    let mut current_code = String::new();

    for line in content.lines() {
        if !in_block {
            // Check for fence start (allow up to 3 spaces of indentation per CommonMark)
            let trimmed = line.trim_start();
            let indent = line.len() - trimmed.len();
            if indent <= 3 {
                if let Some(fence) = trimmed.strip_prefix("```") {
                    let l = fence.trim().to_lowercase();
                    if l.is_empty() || matches!(l.as_str(), "bash" | "sh" | "shell") {
                        // Flush accumulated text
                        if !current_text.is_empty() {
                            segments.push(ContentSegment::Text(std::mem::take(&mut current_text)));
                        }
                        in_block = true;
                        lang = if l.is_empty() { "bash".to_string() } else { l };
                        current_code.clear();
                        continue;
                    }
                }
            }
            // Not a fence — accumulate text
            if !current_text.is_empty() {
                current_text.push('\n');
            }
            current_text.push_str(line);
        } else if line.trim() == "```" {
            // End of code block
            segments.push(ContentSegment::Code {
                language: std::mem::take(&mut lang),
                content: std::mem::take(&mut current_code),
            });
            in_block = false;
        } else {
            // Inside code block
            if !current_code.is_empty() {
                current_code.push('\n');
            }
            current_code.push_str(line);
        }
    }

    // Handle unclosed fence — treat remaining as code
    if in_block {
        segments.push(ContentSegment::Code {
            language: lang,
            content: current_code,
        });
    } else if !current_text.is_empty() {
        segments.push(ContentSegment::Text(current_text));
    }

    segments
}

/// Render an assistant message with markdown support and styled bash blocks.
pub fn render_assistant_message(content: &str) -> Vec<Line<'static>> {
    render_assistant_message_with_width(content, 80)
}

/// Render an assistant message with a specific width for bash block rendering.
pub fn render_assistant_message_with_width(content: &str, width: usize) -> Vec<Line<'static>> {
    use super::bash_block::render_bash_block;

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "VAC",
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
    )));

    let segments = split_content_segments(content);
    for segment in segments {
        match segment {
            ContentSegment::Text(text) => {
                if !text.trim().is_empty() {
                    match render_markdown_to_lines_safe(&text) {
                        Ok(md_lines) => lines.extend(md_lines),
                        Err(_) => lines.extend(text.lines().map(|l| Line::raw(l.to_string()))),
                    }
                }
            }
            ContentSegment::Code { language, content } => {
                let block = super::bash_block::BashBlock { language, content };
                lines.extend(render_bash_block(&block, width));
            }
        }
    }

    lines
}

/// Render a pending tool call bubble.
pub fn render_tool_call_pending(tool_call: &ToolCall) -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
            Span::styled("⏳ ", Style::default().fg(Color::Yellow)),
            Span::styled(
                tool_call.function.name.clone(),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" [pending approval]", Style::default().fg(Color::DarkGray)),
        ]),
    ]
}

/// Render a tool call result bubble.
pub fn render_tool_result(result: &ToolCallResult) -> Vec<Line<'static>> {
    let (icon, color) = match result.status {
        ToolCallResultStatus::Success => ("✓", Color::Green),
        ToolCallResultStatus::Error => ("✗", Color::Red),
        _ => ("·", Color::Gray),
    };
    vec![
        Line::from(vec![
            Span::styled(format!("{icon} "), Style::default().fg(color)),
            Span::styled(
                result.call.function.name.clone(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(result.result.clone(), Style::default().fg(Color::DarkGray)),
        ]),
    ]
}
