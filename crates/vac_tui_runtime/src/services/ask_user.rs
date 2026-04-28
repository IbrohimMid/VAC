//! Ask-User popup
//!
//! Structured Q&A with the user when an assistant requests clarification via
//! the `ask_user` tool. Parsed from the tool call's JSON arguments:
//!
//! ```json
//! { "question": "…", "options": [{"id":"a","label":"…","description":"…"}],
//!   "kind": "single_select", "metadata": {"context": "..."} }
//! ```

use crate::app::AppState;
use crate::services::theme::StyleKey;
use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AskUserQuestionKind {
    /// One option must be selected (radio buttons).
    #[default]
    SingleSelect,
    /// Multiple options can be toggled on/off (checkboxes).
    MultiSelect,
    /// No options; only the free-text field is shown.
    FreeText,
    /// Options + free-text combined.
    Mixed,
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
    let kind = state.layout.ask_user.question_kind;

    match kind {
        AskUserQuestionKind::FreeText => {
            serde_json::json!({
                "kind": "free_text",
                "text": state.layout.ask_user.input,
                "metadata": state.layout.ask_user.metadata,
            })
        }
        AskUserQuestionKind::SingleSelect => {
            let sel = state
                .layout
                .ask_user
                .options
                .get(state.layout.ask_user.selected);
            serde_json::json!({
                "kind": "single_select",
                "selected": sel.map(|o| &o.id),
                "label": sel.map(|o| o.label.as_str()),
                "metadata": sel.map(|o| &o.metadata).unwrap_or(&HashMap::new()),
                "question_metadata": state.layout.ask_user.metadata,
            })
        }
        AskUserQuestionKind::MultiSelect => {
            let selected: Vec<&AskUserOption> = multi_selected
                .iter()
                .filter_map(|&i| state.layout.ask_user.options.get(i))
                .collect();
            serde_json::json!({
                "kind": "multi_select",
                "selected": selected.iter().map(|o| &o.id).collect::<Vec<_>>(),
                "labels": selected.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(),
                "metadata": selected.iter().map(|o| &o.metadata).collect::<Vec<_>>(),
                "question_metadata": state.layout.ask_user.metadata,
            })
        }
        AskUserQuestionKind::Mixed => {
            let sel = state
                .layout
                .ask_user
                .options
                .get(state.layout.ask_user.selected);
            serde_json::json!({
                "kind": "mixed",
                "selected": sel.map(|o| &o.id),
                "text": state.layout.ask_user.input,
                "metadata": sel.map(|o| &o.metadata).unwrap_or(&HashMap::new()),
                "question_metadata": state.layout.ask_user.metadata,
            })
        }
    }
}

pub fn filtered_option_indices(query: &str, options: &[AskUserOption]) -> Vec<usize> {
    let q = query.trim();
    if q.is_empty() {
        return (0..options.len()).collect();
    }

    let pattern = Pattern::new(
        q,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut buf: Vec<char> = Vec::new();

    let mut matches: Vec<(u32, usize)> = Vec::new();
    for (idx, opt) in options.iter().enumerate() {
        let mut text = opt.label.clone();
        if let Some(desc) = &opt.description {
            text.push(' ');
            text.push_str(desc);
        }
        let haystack: Utf32Str<'_> = Utf32Str::new(&text, &mut buf);
        if let Some(score) = pattern.score(haystack, &mut matcher) {
            matches.push((score, idx));
        }
    }

    matches.sort_by(
        |(a_score, a_idx), (b_score, b_idx)| match b_score.cmp(a_score) {
            std::cmp::Ordering::Equal => options[*a_idx]
                .label
                .to_lowercase()
                .cmp(&options[*b_idx].label.to_lowercase()),
            other => other,
        },
    );

    matches.into_iter().map(|(_, idx)| idx).collect()
}

pub fn answer_summary(state: &AppState, filtered: &[usize]) -> String {
    match state.layout.ask_user.question_kind {
        AskUserQuestionKind::FreeText => state.layout.ask_user.input.trim().to_string(),
        AskUserQuestionKind::SingleSelect => state
            .layout
            .ask_user
            .options
            .get(state.layout.ask_user.selected)
            .map(|o| o.label.clone())
            .unwrap_or_default(),
        AskUserQuestionKind::MultiSelect => {
            let mut labels = state
                .layout
                .ask_user
                .multi_selected
                .iter()
                .filter_map(|&i| {
                    state
                        .layout
                        .ask_user
                        .options
                        .get(i)
                        .map(|o| o.label.clone())
                })
                .collect::<Vec<_>>();
            labels.sort();
            if labels.is_empty() {
                if let Some(&idx) = filtered.first() {
                    return state
                        .layout
                        .ask_user
                        .options
                        .get(idx)
                        .map(|o| o.label.clone())
                        .unwrap_or_default();
                }
            }
            labels.join(", ")
        }
        AskUserQuestionKind::Mixed => {
            let t = state.layout.ask_user.input.trim();
            if !t.is_empty() {
                return t.to_string();
            }
            state
                .layout
                .ask_user
                .options
                .get(state.layout.ask_user.selected)
                .map(|o| o.label.clone())
                .unwrap_or_default()
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
    let option_count = state.layout.ask_user.options.len() as u16;
    let show_search = !state.layout.ask_user.options.is_empty();
    let list_cap = option_count.clamp(1, 10);
    let height = (if show_search { 13 } else { 10 } + list_cap).min(terminal.height);
    let x = (terminal.width.saturating_sub(width)) / 2;
    let y = (terminal.height.saturating_sub(height)) / 2;
    let area = Rect::new(x, y, width, height);

    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(state.core.theme.style(StyleKey::Accent))
        .title(Span::styled(
            " Assistant is asking… ",
            state
                .core
                .theme
                .style(StyleKey::Warning)
                .add_modifier(Modifier::BOLD),
        ));
    f.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width - 2,
        height: area.height - 2,
    };

    let mut constraints = Vec::new();
    constraints.push(Constraint::Length(3));
    if show_search {
        constraints.push(Constraint::Length(3));
    }
    constraints.push(Constraint::Min(1));
    constraints.push(Constraint::Length(3));
    constraints.push(Constraint::Length(1));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);
    let option_chunk_idx = if show_search { 2 } else { 1 };
    let input_chunk_idx = if show_search { 3 } else { 2 };
    let footer_chunk_idx = if show_search { 4 } else { 3 };

    // Question
    let question = state.layout.ask_user.question.clone().unwrap_or_default();
    let q_para = Paragraph::new(Line::from(Span::styled(
        question,
        state.core.theme.style(StyleKey::Text),
    )))
    .wrap(Wrap { trim: false });
    f.render_widget(q_para, chunks[0]);

    if show_search {
        let prompt = ">";
        let cursor = "|";
        let placeholder = "Type to filter options";
        let active = state.layout.ask_user.search_active;

        let spans = if state.layout.ask_user.filter.is_empty() {
            vec![
                Span::raw(" "),
                Span::styled(prompt, state.core.theme.style(StyleKey::AppTitle)),
                Span::raw(" "),
                Span::styled(
                    cursor,
                    if active {
                        state.core.theme.style(StyleKey::Accent)
                    } else {
                        state.core.theme.style(StyleKey::Muted)
                    },
                ),
                Span::styled(placeholder, state.core.theme.style(StyleKey::Muted)),
                Span::raw(" "),
            ]
        } else {
            vec![
                Span::raw(" "),
                Span::styled(prompt, state.core.theme.style(StyleKey::AppTitle)),
                Span::raw(" "),
                Span::styled(
                    &state.layout.ask_user.filter,
                    state
                        .core
                        .theme
                        .style(StyleKey::Text)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    cursor,
                    if active {
                        state.core.theme.style(StyleKey::Accent)
                    } else {
                        state.core.theme.style(StyleKey::Muted)
                    },
                ),
                Span::raw(" "),
            ]
        };
        let search_text =
            ratatui::text::Text::from(vec![Line::from(""), Line::from(spans), Line::from("")]);
        f.render_widget(Paragraph::new(search_text), chunks[1]);
    }

    // Options list — render depends on question kind
    let kind = state.layout.ask_user.question_kind;
    let is_multi = kind == AskUserQuestionKind::MultiSelect;
    let mut opt_lines: Vec<Line> = Vec::new();
    let filtered = filtered_option_indices(
        &state.layout.ask_user.filter,
        &state.layout.ask_user.options,
    );
    if filtered.is_empty() && !state.layout.ask_user.options.is_empty() {
        opt_lines.push(Line::from(Span::styled(
            "  (no options match your filter)",
            state.core.theme.style(StyleKey::Muted),
        )));
    } else if state.layout.ask_user.options.is_empty() {
        opt_lines.push(Line::from(Span::styled(
            "  (no options — type your answer below)",
            state.core.theme.style(StyleKey::Muted),
        )));
    } else {
        let visible = chunks[option_chunk_idx].height as usize;
        let sel_pos = filtered
            .iter()
            .position(|&i| i == state.layout.ask_user.selected)
            .unwrap_or(0);

        let mut scroll = state.layout.ask_user.scroll.min(sel_pos);
        if sel_pos >= scroll.saturating_add(visible) {
            scroll = sel_pos + 1 - visible;
        }
        scroll = scroll.min(filtered.len().saturating_sub(1));

        let mut available = visible;
        let has_above = scroll > 0;
        let has_below = scroll.saturating_add(visible) < filtered.len();
        if has_above && available > 0 {
            opt_lines.push(Line::from(Span::styled(
                " ▲",
                state.core.theme.style(StyleKey::Muted),
            )));
            available = available.saturating_sub(1);
        }
        if has_below && available > 0 {
            available = available.saturating_sub(1);
        }

        for i in 0..available {
            let pos = scroll + i;
            if pos >= filtered.len() {
                break;
            }
            let opt_idx = filtered[pos];
            let opt = &state.layout.ask_user.options[opt_idx];
            let is_cursor = opt_idx == state.layout.ask_user.selected;
            let is_checked = state.layout.ask_user.multi_selected.contains(&opt_idx);
            let style = if is_cursor {
                state
                    .core
                    .theme
                    .style(StyleKey::HighlightFg)
                    .patch(state.core.theme.style(StyleKey::HighlightBg))
            } else {
                Style::default()
            };
            let prefix = if is_cursor { ">" } else { " " };
            let mut label = if is_multi {
                let box_mark = if is_checked { "[x]" } else { "[ ]" };
                format!("  {} {} {}. {}", prefix, box_mark, pos + 1, opt.label)
            } else {
                format!("  {} {}. {}", prefix, pos + 1, opt.label)
            };
            if let Some(desc) = &opt.description {
                label.push_str(&format!("  — {}", desc));
            }
            opt_lines.push(Line::from(Span::styled(label, style)));
        }
        if has_below {
            opt_lines.push(Line::from(Span::styled(
                " ▼",
                state.core.theme.style(StyleKey::Muted),
            )));
        }
    }
    f.render_widget(Paragraph::new(opt_lines), chunks[option_chunk_idx]);

    // Free text input row — title + styling reflect whether the caller
    // permits a free-text answer for this question.
    let (title, body_line) = if state.layout.ask_user.allow_free_text {
        (
            " Free text (Enter to submit) ".to_string(),
            Line::from(Span::raw(state.layout.ask_user.input.clone())),
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
    f.render_widget(input_para, chunks[input_chunk_idx]);

    // Footer — varies by question kind
    let accent = state.core.theme.style(StyleKey::Accent);
    let muted = state.core.theme.style(StyleKey::Muted);
    let mut hints = vec![
        Span::raw(" "),
        Span::styled("↑/↓", accent),
        Span::styled(": Option  ", muted),
    ];
    if show_search {
        hints.push(Span::styled("Tab", accent));
        hints.push(Span::styled(": Search  ", muted));
    }
    if is_multi {
        hints.push(Span::styled("Space", accent));
        hints.push(Span::styled(": Toggle  ", muted));
        hints.push(Span::styled("Ctrl+A", accent));
        hints.push(Span::styled(": All  ", muted));
        hints.push(Span::styled("Ctrl+U", accent));
        hints.push(Span::styled(": None  ", muted));
    }
    hints.push(Span::styled("Enter", accent));
    hints.push(Span::styled(": Confirm  ", muted));
    hints.push(Span::styled("Esc", accent));
    hints.push(Span::styled(": Cancel", muted));
    let footer = Line::from(hints);
    f.render_widget(Paragraph::new(footer), chunks[footer_chunk_idx]);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
