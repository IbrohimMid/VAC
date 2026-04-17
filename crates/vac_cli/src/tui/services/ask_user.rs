//! Ask-User popup
//!
//! Structured Q&A with the user when an assistant requests clarification via
//! the `ask_user` tool. Parsed from the tool call's JSON arguments:
//!
//! ```json
//! { "question": "…", "options": [{"id":"a","label":"…","description":"…"}],
//!   "kind": "single_select", "metadata": {"context": "..."} }
//! ```

use crate::tui::app::AppState;
use crate::tui::services::detect_term::ThemeColors;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const ASK_USER_TOOL_NAME: &str = "ask_user";

// ========== Unit 7 (Wave 3.5) — Ask-user structured UX ==========

/// Discriminated question kind; the tool caller can request a specific UX.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskUserQuestionKind {
    /// One option must be selected (radio buttons).
    SingleSelect,
    /// Multiple options can be toggled on/off (checkboxes).
    MultiSelect,
    /// No options; only the free-text field is shown.
    FreeText,
    /// Options + free-text combined.
    Mixed,
}

impl Default for AskUserQuestionKind {
    fn default() -> Self {
        Self::SingleSelect
    }
}

/// Mirror of the donor's `AskUserOption`. Kept local to avoid a stakpak_shared
/// dependency. `description` is optional to support simple yes/no prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserOption {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Arbitrary key-value pairs passed back verbatim in the tool result.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserArgs {
    pub question: String,
    #[serde(default)]
    pub options: Vec<AskUserOption>,
    #[serde(default)]
    pub allow_free_text: bool,
    /// Question UX kind (default: SingleSelect when options present, FreeText otherwise).
    #[serde(default)]
    pub kind: Option<AskUserQuestionKind>,
    /// Arbitrary metadata passed back verbatim in the tool result.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl AskUserArgs {
    /// Resolve the effective question kind (caller-explicit > inferred).
    pub fn effective_kind(&self) -> AskUserQuestionKind {
        if let Some(k) = self.kind {
            return k;
        }
        if self.options.is_empty() {
            AskUserQuestionKind::FreeText
        } else if self.allow_free_text {
            AskUserQuestionKind::Mixed
        } else {
            AskUserQuestionKind::SingleSelect
        }
    }
}

pub fn parse_args(json: &str) -> Option<AskUserArgs> {
    serde_json::from_str(json).ok()
}

/// Build the transcript annotation appended when the ask-user resolves.
///
/// Format: `[ask-user:Q] <question> → <answer>`
pub fn transcript_annotation(question: &str, answer: &str) -> String {
    let q = if question.len() > 80 {
        format!("{}…", &question[..77])
    } else {
        question.to_string()
    };
    format!("[ask-user:Q] {} → {}", q, answer)
}

/// Build the answer payload for the tool result, including metadata round-trip.
pub fn build_answer(state: &AppState, multi_selected: &HashSet<usize>) -> serde_json::Value {
    let kind = state.ask_user_question_kind;

    match kind {
        AskUserQuestionKind::FreeText => {
            serde_json::json!({
                "kind": "free_text",
                "text": state.ask_user_input,
                "metadata": state.ask_user_metadata,
            })
        }
        AskUserQuestionKind::SingleSelect => {
            let sel = state.ask_user_options.get(state.ask_user_selected);
            serde_json::json!({
                "kind": "single_select",
                "selected": sel.map(|o| &o.id),
                "label": sel.map(|o| o.label.as_str()),
                "metadata": sel.map(|o| &o.metadata).unwrap_or(&HashMap::new()),
                "question_metadata": state.ask_user_metadata,
            })
        }
        AskUserQuestionKind::MultiSelect => {
            let selected: Vec<&AskUserOption> = multi_selected
                .iter()
                .filter_map(|&i| state.ask_user_options.get(i))
                .collect();
            serde_json::json!({
                "kind": "multi_select",
                "selected": selected.iter().map(|o| &o.id).collect::<Vec<_>>(),
                "labels": selected.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(),
                "metadata": selected.iter().map(|o| &o.metadata).collect::<Vec<_>>(),
                "question_metadata": state.ask_user_metadata,
            })
        }
        AskUserQuestionKind::Mixed => {
            let sel = state.ask_user_options.get(state.ask_user_selected);
            serde_json::json!({
                "kind": "mixed",
                "selected": sel.map(|o| &o.id),
                "text": state.ask_user_input,
                "metadata": sel.map(|o| &o.metadata).unwrap_or(&HashMap::new()),
                "question_metadata": state.ask_user_metadata,
            })
        }
    }
}

pub fn render_ask_user_popup(f: &mut Frame, state: &AppState) {
    let terminal = f.area();
    // Fall back to a terse notice when the terminal is too small to show the
    // full popup. Keeps the assistant question visible without clipping UI.
    if terminal.height < 12 || terminal.width < 40 {
        let notice = Paragraph::new(Line::from(Span::styled(
            " Terminal too small to show ask_user popup — resize to continue. ",
            Style::default().add_modifier(Modifier::REVERSED),
        )));
        f.render_widget(Clear, terminal);
        f.render_widget(notice, terminal);
        return;
    }
    let width = (terminal.width * 70 / 100).max(60).min(terminal.width);
    let option_count = state.ask_user_options.len() as u16;
    let height = (7 + option_count.min(10) + 3).min(terminal.height);
    let x = (terminal.width.saturating_sub(width)) / 2;
    let y = (terminal.height.saturating_sub(height)) / 2;
    let area = Rect::new(x, y, width, height);

    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ThemeColors::cyan()))
        .title(Span::styled(
            " Assistant is asking… ",
            Style::default()
                .fg(ThemeColors::yellow())
                .add_modifier(Modifier::BOLD),
        ));
    f.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width - 2,
        height: area.height - 2,
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(inner);

    // Question
    let question = state.ask_user_question.clone().unwrap_or_default();
    let q_para = Paragraph::new(Line::from(Span::styled(
        question,
        Style::default().fg(ThemeColors::text()),
    )))
    .wrap(Wrap { trim: false });
    f.render_widget(q_para, chunks[0]);

    // Options list — render depends on question kind
    let kind = state.ask_user_question_kind;
    let is_multi = kind == AskUserQuestionKind::MultiSelect;
    let mut opt_lines: Vec<Line> = Vec::new();
    for (i, opt) in state.ask_user_options.iter().enumerate() {
        let is_cursor = i == state.ask_user_selected;
        let is_checked = state.ask_user_multi_selected.contains(&i);
        let marker = if is_multi {
            if is_checked { "[x]" } else { "[ ]" }
        } else if is_cursor {
            ">"
        } else {
            " "
        };
        let style = if is_cursor {
            Style::default()
                .fg(ThemeColors::highlight_fg())
                .bg(ThemeColors::highlight_bg())
        } else {
            Style::default()
        };
        let mut label = format!("  {} {}. {}", marker, i + 1, opt.label);
        if let Some(desc) = &opt.description {
            label.push_str(&format!("  — {}", desc));
        }
        opt_lines.push(Line::from(Span::styled(label, style)));
    }
    if opt_lines.is_empty() {
        opt_lines.push(Line::from(Span::styled(
            "  (no options — type your answer below)",
            Style::default().fg(ThemeColors::dark_gray()),
        )));
    }
    f.render_widget(Paragraph::new(opt_lines), chunks[1]);

    // Free text input row — title + styling reflect whether the caller
    // permits a free-text answer for this question.
    let (title, body_line) = if state.ask_user_allow_free_text {
        (
            " Free text (Enter to submit) ".to_string(),
            Line::from(Span::raw(state.ask_user_input.clone())),
        )
    } else {
        (
            " Free text disabled — pick an option ".to_string(),
            Line::from(Span::styled(
                "(select above and press Enter)",
                Style::default().add_modifier(Modifier::DIM),
            )),
        )
    };
    let input_block = Block::default().borders(Borders::ALL).title(title);
    let input_para = Paragraph::new(body_line).block(input_block);
    f.render_widget(input_para, chunks[2]);

    // Footer — varies by question kind
    let mut hints = vec![
        Span::raw(" "),
        Span::styled("↑/↓", Style::default().fg(ThemeColors::cyan())),
        Span::styled(": Option  ", Style::default().fg(ThemeColors::dark_gray())),
    ];
    if is_multi {
        hints.push(Span::styled(
            "Space",
            Style::default().fg(ThemeColors::cyan()),
        ));
        hints.push(Span::styled(
            ": Toggle  ",
            Style::default().fg(ThemeColors::dark_gray()),
        ));
    }
    hints.push(Span::styled(
        "Enter",
        Style::default().fg(ThemeColors::cyan()),
    ));
    hints.push(Span::styled(
        ": Submit  ",
        Style::default().fg(ThemeColors::dark_gray()),
    ));
    hints.push(Span::styled(
        "Esc",
        Style::default().fg(ThemeColors::cyan()),
    ));
    hints.push(Span::styled(
        ": Cancel",
        Style::default().fg(ThemeColors::dark_gray()),
    ));
    let footer = Line::from(hints);
    f.render_widget(Paragraph::new(footer), chunks[3]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_with_kind_and_metadata() {
        let json = r#"{
            "question": "Pick a framework",
            "options": [
                {"id": "a", "label": "Actix", "metadata": {"tier": "1"}},
                {"id": "b", "label": "Axum", "description": "Tower-based"}
            ],
            "kind": "multi_select",
            "metadata": {"source": "planner"},
            "allow_free_text": false
        }"#;
        let args = parse_args(json).unwrap();
        assert_eq!(args.effective_kind(), AskUserQuestionKind::MultiSelect);
        assert_eq!(args.metadata.get("source").unwrap(), "planner");
        assert_eq!(args.options[0].metadata.get("tier").unwrap(), "1");
        assert!(args.options[1].metadata.is_empty());
    }

    #[test]
    fn effective_kind_infers_from_options_and_free_text() {
        // No options → FreeText
        let args = AskUserArgs {
            question: "why?".into(),
            options: vec![],
            allow_free_text: false,
            kind: None,
            metadata: HashMap::new(),
        };
        assert_eq!(args.effective_kind(), AskUserQuestionKind::FreeText);

        // Options + free_text → Mixed
        let args = AskUserArgs {
            question: "why?".into(),
            options: vec![AskUserOption {
                id: "a".into(),
                label: "A".into(),
                description: None,
                metadata: HashMap::new(),
            }],
            allow_free_text: true,
            kind: None,
            metadata: HashMap::new(),
        };
        assert_eq!(args.effective_kind(), AskUserQuestionKind::Mixed);

        // Options only → SingleSelect
        let args = AskUserArgs {
            question: "why?".into(),
            options: vec![AskUserOption {
                id: "a".into(),
                label: "A".into(),
                description: None,
                metadata: HashMap::new(),
            }],
            allow_free_text: false,
            kind: None,
            metadata: HashMap::new(),
        };
        assert_eq!(args.effective_kind(), AskUserQuestionKind::SingleSelect);
    }

    #[test]
    fn transcript_annotation_format() {
        let ann = transcript_annotation("Which DB?", "postgres");
        assert_eq!(ann, "[ask-user:Q] Which DB? → postgres");
    }

    #[test]
    fn transcript_annotation_truncates_long_question() {
        let q = "x".repeat(200);
        let ann = transcript_annotation(&q, "yes");
        assert!(ann.starts_with("[ask-user:Q] "));
        assert!(ann.contains('…'));
        assert!(ann.len() < 200);
    }

    #[test]
    fn metadata_round_trip_through_json() {
        let opt = AskUserOption {
            id: "test".into(),
            label: "Test".into(),
            description: Some("desc".into()),
            metadata: HashMap::from([("key".into(), "val".into())]),
        };
        let json = serde_json::to_string(&opt).unwrap();
        let parsed: AskUserOption = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.metadata.get("key").unwrap(), "val");
    }
}
