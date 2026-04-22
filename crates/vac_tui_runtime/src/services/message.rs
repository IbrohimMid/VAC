//! VAC-native message rendering module.
//!
//! Provides formatting and rendering for TUI message bubbles.
//! Uses VAC types directly (no stakpak_shared dependency).

use crate::services::theme::StyleKey;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use regex::Regex;
use serde_json::Value;

use crate::services::render_markdown_to_lines_safe;
use crate::types::{ToolCall, ToolCallResult, ToolCallResultStatus};

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

/// Helper to format json values
fn format_json_value(value: &Value) -> String {
    match value {
        Value::Object(obj) => {
            if obj.is_empty() {
                return "{}".to_string();
            }
            let mut values = obj
                .into_iter()
                .map(|(k, v)| (k, format_json_value(v)))
                .collect::<Vec<_>>();
            values.sort_by_key(|(_, val)| val.len());
            values
                .into_iter()
                .map(|(k, v)| format!("{} = {}", k, v))
                .collect::<Vec<_>>()
                .join(", ")
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                "[]".to_string()
            } else {
                format!(
                    "[{}]",
                    arr.iter()
                        .map(format_simple_value)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
        _ => format_simple_value(value),
    }
}

fn format_simple_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        Value::Object(_) => "object".to_string(),
        Value::Array(arr) => format!("[{}]", arr.len()),
    }
}

pub fn extract_full_command_arguments(tool_call: &ToolCall) -> String {
    let args = &tool_call.function.arguments;
    if let Ok(v) = serde_json::from_str::<Value>(args) {
        return format_json_value(&v);
    }
    let patterns = vec![
        r#"["']?(\w+)["']?\s*:\s*["']([^"']+)["']"#,
        r#"(\w+)\s*:\s*([^,}\s]+)"#,
    ];
    for pattern in patterns {
        if let Ok(re) = Regex::new(pattern) {
            let mut results = Vec::new();
            for caps in re.captures_iter(args) {
                if caps.len() >= 3 {
                    // Safe: caps.len() >= 3 guarantees groups 1 and 2 exist.
                    #[allow(clippy::unwrap_used)]
                    results.push(format!(
                        "{} = {}",
                        caps.get(1).unwrap().as_str(),
                        caps.get(2).unwrap().as_str()
                    ));
                }
            }
            if !results.is_empty() {
                return results.join(", ");
            }
        }
    }
    let wrapped = format!("{{{}}}", args);
    if let Ok(v) = serde_json::from_str::<Value>(&wrapped) {
        return format_json_value(&v);
    }
    let trimmed = args.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    String::new()
}

/// Render a user message as styled lines with cyan prefix bar.
pub fn render_user_message(content: &str, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(
            "▌ ",
            crate::services::theme::Theme::default().style(StyleKey::Accent),
        ),
        Span::styled(
            "You",
            crate::services::theme::Theme::default()
                .style(StyleKey::UserMessage)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    for line in content.lines() {
        let truncated = truncate_chars(line, width);
        lines.push(Line::from(vec![
            Span::styled(
                "▌ ",
                crate::services::theme::Theme::default().style(StyleKey::Accent),
            ),
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
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") {
                // Safe: starts_with("```") guard above guarantees strip_prefix succeeds.
                #[allow(clippy::unwrap_used)]
                let fence = trimmed.strip_prefix("```").unwrap();
                if !current_text.is_empty() {
                    segments.push(ContentSegment::Text(std::mem::take(&mut current_text)));
                }
                in_block = true;
                lang = fence.trim().to_lowercase();
                current_code.clear();
                continue;
            }
            if !current_text.is_empty() {
                current_text.push('\n');
            }
            current_text.push_str(line);
        } else if line.trim_start().starts_with("```") {
            segments.push(ContentSegment::Code {
                language: std::mem::take(&mut lang),
                content: std::mem::take(&mut current_code),
            });
            in_block = false;
        } else {
            if !current_code.is_empty() {
                current_code.push('\n');
            }
            current_code.push_str(line);
        }
    }

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
    let theme = crate::services::theme::Theme::default();
    render_assistant_message_with_theme(content, width, &theme)
}

/// Render an assistant message using the provided theme.
pub fn render_assistant_message_with_theme(
    content: &str,
    width: usize,
    theme: &crate::services::theme::Theme,
) -> Vec<Line<'static>> {
    use super::bash_block::render_bash_block;

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "VAC",
        crate::services::theme::Theme::default()
            .style(StyleKey::Success)
            .add_modifier(Modifier::BOLD),
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
                if language.is_empty() || matches!(language.as_str(), "bash" | "sh" | "shell") {
                    let block = super::bash_block::BashBlock {
                        language: "bash".to_string(),
                        content,
                    };
                    lines.extend(render_bash_block(theme, &block, width));
                } else {
                    let reconstructed = format!("```{}\n{}\n```", language, content);
                    match render_markdown_to_lines_safe(&reconstructed) {
                        Ok(md_lines) => lines.extend(md_lines),
                        Err(_) => {
                            lines.extend(reconstructed.lines().map(|l| Line::raw(l.to_string())))
                        }
                    }
                }
            }
        }
    }

    lines
}

/// Render a pending tool call bubble.
pub fn render_tool_call_pending(tool_call: &ToolCall) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled(
            "⏳ ",
            crate::services::theme::Theme::default().style(StyleKey::Warning),
        ),
        Span::styled(
            tool_call.function.name.clone(),
            crate::services::theme::Theme::default()
                .style(StyleKey::Warning)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " [pending approval]",
            crate::services::theme::Theme::default().style(StyleKey::Muted),
        ),
    ])];
    let args = extract_full_command_arguments(tool_call);
    if !args.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                truncate_chars(&args, 100),
                crate::services::theme::Theme::default().style(StyleKey::Muted),
            ),
        ]));
    }
    lines
}

/// Render a tool call result bubble.
pub fn render_tool_result(result: &ToolCallResult) -> Vec<Line<'static>> {
    let (icon, status_key) = match result.status {
        ToolCallResultStatus::Success => ("✓", StyleKey::Success),
        ToolCallResultStatus::Error => ("✗", StyleKey::Error),
        _ => ("·", StyleKey::Muted),
    };
    let base_style = crate::services::theme::Theme::default().style(status_key);
    let mut lines = vec![Line::from(vec![
        Span::styled(format!("{icon} "), base_style),
        Span::styled(
            result.call.function.name.clone(),
            base_style.add_modifier(Modifier::BOLD),
        ),
    ])];

    let args = extract_full_command_arguments(&result.call);
    if !args.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                truncate_chars(&args, 100),
                crate::services::theme::Theme::default().style(StyleKey::Muted),
            ),
        ]));
    }

    let mut result_lines: Vec<&str> = result.result.lines().collect();
    let truncated = if result_lines.len() > 5 {
        result_lines.truncate(5);
        true
    } else {
        false
    };

    for line in result_lines {
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                truncate_chars(line, 100),
                crate::services::theme::Theme::default().style(StyleKey::Muted),
            ),
        ]));
    }
    if truncated {
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                "... (output truncated)",
                crate::services::theme::Theme::default()
                    .style(StyleKey::Muted)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]));
    }

    lines
}
