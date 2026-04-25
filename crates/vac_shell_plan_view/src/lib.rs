//! Slice 12 — plan view widget.
//!
//! Renders a `PlanMetadata` as a status header + objective +
//! numbered steps + blocked-on tail. `Option<&PlanMetadata>` so
//! callers render an empty-state message when no plan is loaded.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use vac_shell_contracts::{PlanMetadata, PlanStatus};

pub fn status_label(s: PlanStatus) -> &'static str {
    match s {
        PlanStatus::Draft => "draft",
        PlanStatus::Active => "active",
        PlanStatus::Blocked => "blocked",
        PlanStatus::Done => "done",
        PlanStatus::Cancelled => "cancelled",
    }
}

fn status_color(s: PlanStatus) -> Color {
    match s {
        PlanStatus::Draft => Color::Gray,
        PlanStatus::Active => Color::Cyan,
        PlanStatus::Blocked => Color::Red,
        PlanStatus::Done => Color::Green,
        PlanStatus::Cancelled => Color::DarkGray,
    }
}

pub fn render_plan_view(f: &mut Frame, plan: Option<&PlanMetadata>, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Plan ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let plan = match plan {
        Some(p) => p,
        None => {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "  no plan loaded — write to .vac/session/plan.md to begin",
                    Style::default().fg(Color::DarkGray),
                ))),
                inner,
            );
            return;
        }
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(vec![
        Span::raw(" status "),
        Span::styled(
            status_label(plan.status),
            Style::default()
                .fg(status_color(plan.status))
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    if let Some(obj) = &plan.objective {
        lines.push(Line::from(vec![
            Span::raw(" objective "),
            Span::styled(obj.clone(), Style::default().fg(Color::Gray)),
        ]));
    }
    if !plan.steps.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            " steps",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )));
        for (i, step) in plan.steps.iter().enumerate() {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {}. ", i + 1),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(step.clone(), Style::default().fg(Color::Gray)),
            ]));
        }
    }
    if !plan.blocked_on.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            " blocked on",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
        for blocker in &plan.blocked_on {
            lines.push(Line::from(vec![
                Span::raw("  · "),
                Span::styled(blocker.clone(), Style::default().fg(Color::Gray)),
            ]));
        }
    }

    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        inner,
    );
}
