//! Plan Review Rendering
//!
//! Compact review overlay for the plan: shows metadata header, body with
//! line numbers, status badge, comment count, and keyboard footer.

use crate::app::AppState;
use crate::services::plan::PlanStatus;
use crate::services::theme::StyleKey;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

pub fn render_plan_review(f: &mut Frame, state: &AppState) {
    let terminal = f.area();
    let width = (terminal.width * 80 / 100).max(60).min(terminal.width);
    let height = (terminal.height * 80 / 100).max(16).min(terminal.height);
    let x = (terminal.width.saturating_sub(width)) / 2;
    let y = (terminal.height.saturating_sub(height)) / 2;
    let area = Rect::new(x, y, width, height);

    f.render_widget(Clear, area);
    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(state.theme.style(StyleKey::Accent)),
        area,
    );

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width - 2,
        height: area.height - 2,
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Title + meta
            Constraint::Min(3),    // Body
            Constraint::Length(1), // Footer
        ])
        .split(inner);

    render_title(f, state, chunks[0]);
    render_body(f, state, chunks[1]);
    render_footer(f, state, chunks[2]);
}

fn render_title(f: &mut Frame, state: &AppState, area: Rect) {
    let (title, status, version) = match &state.plan.metadata {
        Some(m) => (m.title.clone(), m.status, m.version),
        None => ("Untitled Plan".to_string(), PlanStatus::Drafting, 1),
    };

    let (status_label, status_key) = match status {
        PlanStatus::Drafting => ("DRAFTING", StyleKey::Warning),
        PlanStatus::PendingReview => ("PENDING REVIEW", StyleKey::Accent),
        PlanStatus::Approved => ("APPROVED", StyleKey::Success),
    };

    let comment_count = state.plan.comments.len();
    let line1 = Line::from(vec![
        Span::styled("  Plan: ", state.theme.style(StyleKey::Muted)),
        Span::styled(
            title,
            state
                .theme
                .style(StyleKey::Warning)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(format!("v{}", version), state.theme.style(StyleKey::Muted)),
        Span::raw("  "),
        Span::styled(
            status_label,
            state.theme.style(status_key).add_modifier(Modifier::BOLD),
        ),
    ]);

    let line2 = Line::from(vec![
        Span::styled(
            format!("  {} lines  ", count_body_lines(state)),
            state.theme.style(StyleKey::Muted),
        ),
        Span::styled(
            format!("{} comments", comment_count),
            state.theme.style(StyleKey::Muted),
        ),
    ]);

    f.render_widget(Paragraph::new(vec![line1, line2]), area);
}

fn render_body(f: &mut Frame, state: &AppState, area: Rect) {
    let body = crate::services::plan::extract_plan_body(&state.plan.draft);
    let lines_iter = body.lines();
    let mut lines: Vec<Line> = Vec::new();
    for (i, line_str) in lines_iter.enumerate() {
        let is_selected = i == state.plan.review_selected;
        let num_style = if is_selected {
            state
                .theme
                .style(StyleKey::Warning)
                .add_modifier(Modifier::BOLD)
        } else {
            state.theme.style(StyleKey::Muted)
        };
        let body_style = if is_selected {
            state
                .theme
                .style(StyleKey::HighlightBg)
                .patch(state.theme.style(StyleKey::HighlightFg))
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{:>4} │ ", i + 1), num_style),
            Span::styled(line_str.to_string(), body_style),
        ]));
    }

    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((state.plan.review_scroll as u16, 0));
    f.render_widget(para, area);
}

fn render_footer(f: &mut Frame, state: &AppState, area: Rect) {
    let accent = state.theme.style(StyleKey::Accent);
    let muted = state.theme.style(StyleKey::Muted);
    let footer = Line::from(vec![
        Span::raw(" "),
        Span::styled("↑/↓", accent),
        Span::styled(": Line  ", muted),
        Span::styled("PgUp/PgDn", accent),
        Span::styled(": Scroll  ", muted),
        Span::styled("a", accent),
        Span::styled(": Approve  ", muted),
        Span::styled("r", accent),
        Span::styled(": Request changes  ", muted),
        Span::styled("Esc", accent),
        Span::styled(": Close", muted),
    ]);
    f.render_widget(Paragraph::new(footer), area);
}

fn count_body_lines(state: &AppState) -> usize {
    crate::services::plan::extract_plan_body(&state.plan.draft)
        .lines()
        .count()
}
