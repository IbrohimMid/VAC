//! Ask-User popup
//!
//! Structured Q&A with the user when an assistant requests clarification via
//! the `ask_user` tool. Parsed from the tool call's JSON arguments:
//!
//! ```json
//! { "question": "…", "options": [{"id":"a","label":"…","description":"…"}] }
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

pub const ASK_USER_TOOL_NAME: &str = "ask_user";

/// Mirror of the donor's `AskUserOption`. Kept local to avoid a stakpak_shared
/// dependency. `description` is optional to support simple yes/no prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserOption {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserArgs {
    pub question: String,
    #[serde(default)]
    pub options: Vec<AskUserOption>,
    #[serde(default)]
    pub allow_free_text: bool,
}

pub fn parse_args(json: &str) -> Option<AskUserArgs> {
    serde_json::from_str(json).ok()
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

    // Options list
    let mut opt_lines: Vec<Line> = Vec::new();
    for (i, opt) in state.ask_user_options.iter().enumerate() {
        let is_selected = i == state.ask_user_selected;
        let marker = if is_selected { ">" } else { " " };
        let style = if is_selected {
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

    // Footer
    let footer = Line::from(vec![
        Span::raw(" "),
        Span::styled("↑/↓", Style::default().fg(ThemeColors::cyan())),
        Span::styled(": Option  ", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled("Enter", Style::default().fg(ThemeColors::cyan())),
        Span::styled(": Submit  ", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled("Esc", Style::default().fg(ThemeColors::cyan())),
        Span::styled(": Cancel", Style::default().fg(ThemeColors::dark_gray())),
    ]);
    f.render_widget(Paragraph::new(footer), chunks[3]);
}
