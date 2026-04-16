//! Plan Review Rendering
//!
//! Compact review overlay for the plan: shows metadata header, body with
//! line numbers, status badge, comment count, and keyboard footer.

use crate::tui::app::AppState;
use crate::tui::services::detect_term::ThemeColors;
use crate::tui::services::plan::PlanStatus;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
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
            .border_style(Style::default().fg(ThemeColors::cyan())),
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
    render_footer(f, chunks[2]);
}

fn render_title(f: &mut Frame, state: &AppState, area: Rect) {
    let (title, status, version) = match &state.plan_metadata {
        Some(m) => (m.title.clone(), m.status, m.version),
        None => ("Untitled Plan".to_string(), PlanStatus::Drafting, 1),
    };

    let (status_label, status_color) = match status {
        PlanStatus::Drafting => ("DRAFTING", Color::Yellow),
        PlanStatus::PendingReview => ("PENDING REVIEW", Color::Cyan),
        PlanStatus::Approved => ("APPROVED", Color::Green),
    };

    let comment_count = state.plan_comments.len();
    let line1 = Line::from(vec![
        Span::styled(
            "  Plan: ",
            Style::default().fg(ThemeColors::dark_gray()),
        ),
        Span::styled(
            title,
            Style::default()
                .fg(ThemeColors::yellow())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("v{}", version),
            Style::default().fg(ThemeColors::dark_gray()),
        ),
        Span::raw("  "),
        Span::styled(
            status_label,
            Style::default().fg(status_color).add_modifier(Modifier::BOLD),
        ),
    ]);

    let line2 = Line::from(vec![
        Span::styled(
            format!("  {} lines  ", count_body_lines(state)),
            Style::default().fg(ThemeColors::dark_gray()),
        ),
        Span::styled(
            format!("{} comments", comment_count),
            Style::default().fg(ThemeColors::dark_gray()),
        ),
    ]);

    f.render_widget(Paragraph::new(vec![line1, line2]), area);
}

fn render_body(f: &mut Frame, state: &AppState, area: Rect) {
    let body = crate::tui::services::plan::extract_plan_body(&state.plan_draft);
    let lines_iter = body.lines();
    let mut lines: Vec<Line> = Vec::new();
    for (i, line_str) in lines_iter.enumerate() {
        let is_selected = i == state.plan_review_selected;
        let num_style = if is_selected {
            Style::default()
                .fg(ThemeColors::yellow())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(ThemeColors::dark_gray())
        };
        let body_style = if is_selected {
            Style::default()
                .bg(ThemeColors::highlight_bg())
                .fg(ThemeColors::highlight_fg())
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
        .scroll((state.plan_review_scroll as u16, 0));
    f.render_widget(para, area);
}

fn render_footer(f: &mut Frame, area: Rect) {
    let footer = Line::from(vec![
        Span::raw(" "),
        Span::styled("↑/↓", Style::default().fg(ThemeColors::cyan())),
        Span::styled(": Line  ", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled("PgUp/PgDn", Style::default().fg(ThemeColors::cyan())),
        Span::styled(": Scroll  ", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled("a", Style::default().fg(ThemeColors::cyan())),
        Span::styled(": Approve  ", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled("r", Style::default().fg(ThemeColors::cyan())),
        Span::styled(": Request changes  ", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled("Esc", Style::default().fg(ThemeColors::cyan())),
        Span::styled(": Close", Style::default().fg(ThemeColors::dark_gray())),
    ]);
    f.render_widget(Paragraph::new(footer), area);
}

fn count_body_lines(state: &AppState) -> usize {
    crate::tui::services::plan::extract_plan_body(&state.plan_draft)
        .lines()
        .count()
}
