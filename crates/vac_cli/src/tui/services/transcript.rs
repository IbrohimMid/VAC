//! Transcript rendering with wrapping and JSON collapse.
//! Prewrap pattern adapted from stakpak/tui/src/services/wrapping.rs (Apache-2.0).

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::tui::app::TranscriptEntry;

/// Render transcript entries into wrapped lines for display.
/// Returns (all_lines, total_line_count).
pub fn render_transcript_lines(
    entries: &[TranscriptEntry],
    max_width: usize,
) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();

    for entry in entries {
        // Label
        out.push(Line::from(Span::styled(
            entry.label.clone(),
            Style::default().fg(entry.color).add_modifier(Modifier::BOLD),
        )));

        // Body: collapse JSON, wrap lines
        let body = collapse_json(&entry.body);
        for raw in body.lines() {
            if raw.is_empty() {
                out.push(Line::from(""));
                continue;
            }
            // Word-wrap
            let chars: Vec<char> = raw.chars().collect();
            let mut start = 0;
            while start < chars.len() {
                let end = (start + max_width).min(chars.len());
                let seg: String = chars[start..end].iter().collect();
                out.push(Line::from(Span::raw(seg)));
                start = end;
            }
        }
        out.push(Line::from("")); // separator
    }

    out
}

/// Collapse raw JSON blobs to compact summaries.
pub fn collapse_json(text: &str) -> String {
    let trimmed = text.trim();
    // Pure JSON object/array that's long
    if trimmed.len() > 200 && (trimmed.starts_with('{') || trimmed.starts_with('[')) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            return match &v {
                serde_json::Value::Object(m) => {
                    let keys: Vec<&str> = m.keys().map(|k| k.as_str()).take(4).collect();
                    format!("{{{}{}}}",
                        keys.join(", "),
                        if m.len() > 4 { format!(", +{}", m.len() - 4) } else { String::new() }
                    )
                }
                serde_json::Value::Array(a) => format!("[{} items]", a.len()),
                _ => truncate_str(trimmed, 200),
            };
        }
    }
    // Mixed text: collapse embedded JSON blocks > 80 chars
    if text.len() > 300 {
        let mut result = String::with_capacity(text.len());
        let mut depth = 0i32;
        let mut block_start: Option<usize> = None;
        for ch in text.chars() {
            match ch {
                '{' | '[' => {
                    if depth == 0 { block_start = Some(result.len()); }
                    depth += 1;
                    result.push(ch);
                }
                '}' | ']' => {
                    depth -= 1;
                    result.push(ch);
                    if depth == 0 {
                        if let Some(start) = block_start {
                            if result.len() - start > 80 {
                                result.truncate(start);
                                result.push_str("[…]");
                            }
                        }
                        block_start = None;
                    }
                }
                c => result.push(c),
            }
        }
        return result;
    }
    text.to_string()
}

fn truncate_str(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n { out.push_str("…"); }
    out
}
