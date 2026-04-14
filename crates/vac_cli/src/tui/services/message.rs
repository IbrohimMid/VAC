//! VAC-native message rendering module.
//!
//! Provides formatting and rendering for TUI message bubbles.
//! Uses VAC types directly (no stakpak_shared dependency).

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::tui::types::{ToolCall, ToolCallResult, ToolCallResultStatus};
use crate::tui::services::render_markdown_to_lines_safe;

/// Render a user message as styled lines with cyan prefix bar.
pub fn render_user_message(content: &str, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("▌ ", Style::default().fg(Color::Cyan)),
        Span::styled("You", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ]));
    for line in content.lines() {
        let truncated = if line.len() > width { format!("{}…", &line[..width.saturating_sub(1)]) } else { line.to_string() };
        lines.push(Line::from(vec![
            Span::styled("▌ ", Style::default().fg(Color::Cyan)),
            Span::raw(truncated),
        ]));
    }
    lines
}

/// Render an assistant message with markdown support.
pub fn render_assistant_message(content: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "VAC",
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
    )));
    match render_markdown_to_lines_safe(content) {
        Ok(md_lines) => lines.extend(md_lines),
        Err(_) => lines.extend(content.lines().map(|l| Line::raw(l.to_string()))),
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
