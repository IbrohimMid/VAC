//! Slice 16 — approval detail drawer.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use vac_shell_contracts::{ApprovalDetailView, RiskLevel};

fn risk_label(r: RiskLevel) -> &'static str {
    match r {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
        RiskLevel::Critical => "critical",
    }
}

fn risk_color(r: RiskLevel) -> Color {
    match r {
        RiskLevel::Low => Color::Green,
        RiskLevel::Medium => Color::Yellow,
        RiskLevel::High => Color::Red,
        RiskLevel::Critical => Color::Red,
    }
}

#[derive(Debug, Clone, Default)]
pub struct ApprovalDetailViewState {
    pub visible: bool,
    pub detail: Option<ApprovalDetailView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailKey {
    Approve,
    Reject,
    Escape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetailEvent {
    Approve(String),
    Reject(String),
    Dismissed,
    Ignored,
}

pub fn on_key(state: &mut ApprovalDetailViewState, key: DetailKey) -> DetailEvent {
    if !state.visible {
        return DetailEvent::Ignored;
    }
    let id = state.detail.as_ref().map(|d| d.id.clone());
    match key {
        DetailKey::Approve => match id {
            Some(id) => DetailEvent::Approve(id),
            None => DetailEvent::Ignored,
        },
        DetailKey::Reject => match id {
            Some(id) => DetailEvent::Reject(id),
            None => DetailEvent::Ignored,
        },
        DetailKey::Escape => {
            state.visible = false;
            DetailEvent::Dismissed
        }
    }
}

pub fn render_approval_detail(
    f: &mut Frame,
    state: &ApprovalDetailViewState,
    area: Rect,
) {
    if !state.visible {
        return;
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Approval Detail ")
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let detail = match &state.detail {
        Some(d) => d,
        None => {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "  no approval selected",
                    Style::default().fg(Color::DarkGray),
                ))),
                inner,
            );
            return;
        }
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(vec![
        Span::raw(" tool "),
        Span::styled(
            detail.tool_name.clone(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw("    "),
        Span::raw("risk "),
        Span::styled(
            risk_label(detail.risk_level),
            Style::default()
                .fg(risk_color(detail.risk_level))
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::raw(" reason "),
        Span::styled(detail.reason.clone(), Style::default().fg(Color::Gray)),
    ]));
    if let Some(cmd) = &detail.command_preview {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            " command",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            format!("  $ {cmd}"),
            Style::default().fg(Color::Gray),
        )));
    }
    if let Some(file) = &detail.file_preview {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            " file",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )));
        for line in file.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {line}"),
                Style::default().fg(Color::Gray),
            )));
        }
    }
    if let Some(policy) = &detail.policy_source {
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::raw(" policy "),
            Span::styled(policy.clone(), Style::default().fg(Color::Magenta)),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(
            "  [y]",
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" approve   ", Style::default().fg(Color::Gray)),
        Span::styled(
            "[n]",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" reject   ", Style::default().fg(Color::Gray)),
        Span::styled(
            "[esc]",
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" defer", Style::default().fg(Color::Gray)),
    ]));

    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        inner,
    );
}
