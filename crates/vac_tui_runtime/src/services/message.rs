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

/// Pick the most informative single argument from a tool call for the
/// timeline row's summary column (path > command > pattern > query > url).
/// Falls back to the compact "k = v" projection when no known key is
/// present.
fn tool_summary(tool_call: &ToolCall) -> String {
    let raw = tool_call.function.arguments.trim();
    if let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(raw) {
        for key in [
            "path",
            "file_path",
            "file",
            "command",
            "cmd",
            "pattern",
            "query",
            "url",
            "name",
            "description",
        ] {
            if let Some(Value::String(s)) = obj.get(key) {
                return truncate_chars(s, 60);
            }
        }
    }
    truncate_chars(&extract_full_command_arguments(tool_call), 60)
}

/// Single-row tool timeline entry in the Wave 3 #03 format:
///   `{dot} {name:<14} {summary}  {tail}`
/// `tail` is a right-aligned hint like "queued", "12 matches", "823 B".
fn render_tool_row(
    dot: &str,
    dot_key: StyleKey,
    name: &str,
    summary: &str,
    tail: &str,
    tail_key: StyleKey,
) -> Line<'static> {
    let theme = crate::services::theme::Theme::default();
    let name_padded = format!("{:<14}", truncate_chars(name, 13));
    Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{dot} "), theme.style(dot_key)),
        Span::styled(name_padded, theme.style(StyleKey::Accent)),
        Span::raw(" "),
        Span::styled(summary.to_string(), theme.style(StyleKey::Muted)),
        Span::raw("  "),
        Span::styled(tail.to_string(), theme.style(tail_key)),
    ])
}

/// Render a pending / queued tool call as a timeline row.
pub fn render_tool_call_pending(tool_call: &ToolCall) -> Vec<Line<'static>> {
    vec![render_tool_row(
        "○",
        StyleKey::Muted,
        &tool_call.function.name,
        &tool_summary(tool_call),
        "queued",
        StyleKey::Muted,
    )]
}

/// Render a completed tool call result as a timeline row + optional
/// 1-line summary tail.
pub fn render_tool_result(result: &ToolCallResult) -> Vec<Line<'static>> {
    let (dot, dot_key, tail_key) = match result.status {
        ToolCallResultStatus::Success => ("●", StyleKey::Success, StyleKey::Success),
        ToolCallResultStatus::Error => ("●", StyleKey::Error, StyleKey::Error),
        _ => ("●", StyleKey::Warning, StyleKey::Warning),
    };

    // Tail: prefer envelope duration/size if present, otherwise summarize
    // by output-line count so the operator can see whether the call
    // returned anything substantive without expanding the row.
    let tail = if let Some(envelope) = &result.envelope {
        if envelope.duration_ms > 0 {
            format_duration(envelope.duration_ms)
        } else if !envelope.summary.is_empty() {
            let n = envelope.summary.lines().count();
            format!("{} line{}", n, if n == 1 { "" } else { "s" })
        } else {
            match result.status {
                ToolCallResultStatus::Success => "done".to_string(),
                ToolCallResultStatus::Error => "error".to_string(),
                _ => "…".to_string(),
            }
        }
    } else {
        let bytes = result.result.len();
        if bytes == 0 {
            match result.status {
                ToolCallResultStatus::Success => "done".to_string(),
                ToolCallResultStatus::Error => "error".to_string(),
                _ => "…".to_string(),
            }
        } else if bytes < 1024 {
            format!("{} B", bytes)
        } else {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        }
    };

    let mut lines = vec![render_tool_row(
        dot,
        dot_key,
        &result.call.function.name,
        &tool_summary(&result.call),
        &tail,
        tail_key,
    )];

    // When the result is an error, surface the first line of the
    // output under the row so the operator sees the cause without
    // expanding an overlay.
    if matches!(result.status, ToolCallResultStatus::Error) {
        let first = result.result.lines().next().unwrap_or("").trim();
        if !first.is_empty() {
            lines.push(Line::from(vec![
                Span::raw("     "),
                Span::styled(
                    truncate_chars(first, 100),
                    crate::services::theme::Theme::default().style(StyleKey::Error),
                ),
            ]));
        }
    }

    lines
}

/// Wave 3 #04 — inline approval card.
///
/// Renders the currently-active pending approval as a bordered
/// information block that sits in the conversation stream rather than
/// off in a side panel. Width is the conversation pane width; the card
/// adapts so its borders never clip.
pub fn render_approval_card(
    tool_call: &ToolCall,
    idx: usize,
    total: usize,
    width: usize,
) -> Vec<Line<'static>> {
    let theme = crate::services::theme::Theme::default();
    let muted = theme.style(StyleKey::Muted);
    let accent = theme.style(StyleKey::Accent);
    let warn = theme.style(StyleKey::Warning);
    let err = theme.style(StyleKey::Error);
    let ok = theme.style(StyleKey::Success);

    // Inner width (2 for borders, 2 for leading indent).
    let inner_w = width.saturating_sub(4).max(40);

    let tool_name = tool_call.function.name.as_str();
    let tool_badge = tool_name.to_ascii_uppercase();
    let tool_tag = tool_name.to_ascii_lowercase();

    // Pick a single-line command/path preview for the "$ ..." line.
    let preview = tool_summary(tool_call);
    let preview = if preview.is_empty() {
        "(no arguments)".to_string()
    } else {
        preview
    };

    // Risk heuristic — we don't yet carry a policy clause on ToolCall,
    // so classify by tool name. Shell/edit/write = destructive; reads
    // and queries = elevated; everything else = standard.
    let (risk_label, risk_style, risk_reason) = match tool_name {
        "shell" | "bash" | "run" | "execute" => (
            "DESTRUCTIVE",
            err,
            "shell command · effects outside sandbox",
        ),
        "file_write" | "file_edit" | "write" | "edit" | "apply_patch" => {
            ("WRITES", warn, "modifies files in the working tree")
        }
        "file_read" | "read" | "grep" | "glob" | "search" => {
            ("READ", ok, "read-only · no side effects")
        }
        _ => ("ELEVATED", warn, "requires operator confirmation"),
    };

    let top = format!(
        "┌ approval required {}┐",
        "─".repeat(inner_w.saturating_sub(18))
    );
    let bottom_label = if total > 1 {
        format!("─ batch {}/{} ", idx + 1, total)
    } else {
        String::new()
    };
    let bottom = format!(
        "└{}{}┘",
        bottom_label,
        "─".repeat(inner_w.saturating_sub(bottom_label.chars().count()))
    );

    let pad = |content: &str| -> String {
        let n = content.chars().count();
        if n >= inner_w {
            content.chars().take(inner_w).collect::<String>()
        } else {
            format!("{}{}", content, " ".repeat(inner_w - n))
        }
    };

    let lead = |s: Line<'static>| -> Line<'static> {
        // Indent card under conversation body, matching tool timeline rows.
        let mut spans = vec![Span::raw("  ")];
        spans.extend(s.spans);
        Line::from(spans)
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(lead(Line::from(Span::styled(top, warn))));

    // Header row: BASH badge + human sentence.
    let header_content = format!(
        " {:<6}  the agent wants to run this {}",
        tool_badge, tool_tag
    );
    lines.push(lead(Line::from(vec![
        Span::styled("│", warn),
        Span::styled(pad(&header_content), accent),
        Span::styled("│", warn),
    ])));

    // Blank spacer.
    lines.push(lead(Line::from(vec![
        Span::styled("│", warn),
        Span::styled(pad(""), muted),
        Span::styled("│", warn),
    ])));

    // $ command preview.
    let cmd_line = format!(" $ {}", truncate_chars(&preview, inner_w.saturating_sub(4)));
    lines.push(lead(Line::from(vec![
        Span::styled("│", warn),
        Span::styled(pad(&cmd_line), accent),
        Span::styled("│", warn),
    ])));

    // runtime envelope row.
    let env_line = " cwd  (project) · runtime host · network inherit · writes allowed";
    lines.push(lead(Line::from(vec![
        Span::styled("│", warn),
        Span::styled(pad(env_line), muted),
        Span::styled("│", warn),
    ])));

    // risk row.
    let risk_line = format!(" risk  {}  {}", risk_label, risk_reason);
    lines.push(lead(Line::from(vec![
        Span::styled("│", warn),
        Span::styled(pad(&risk_line), risk_style),
        Span::styled("│", warn),
    ])));

    // policy row — placeholder until ToolCall carries a clause.
    let policy_line = format!(" policy vil.core · {} requires explicit approval", tool_tag);
    lines.push(lead(Line::from(vec![
        Span::styled("│", warn),
        Span::styled(pad(&policy_line), muted),
        Span::styled("│", warn),
    ])));

    // actions row.
    let actions_line = " [enter] approve  [shift+a] approve all  [x] reject  [esc] defer";
    lines.push(lead(Line::from(vec![
        Span::styled("│", warn),
        Span::styled(pad(actions_line), accent),
        Span::styled("│", warn),
    ])));

    lines.push(lead(Line::from(Span::styled(bottom, warn))));
    lines
}

fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{} ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}m {}s", ms / 60_000, (ms % 60_000) / 1000)
    }
}
