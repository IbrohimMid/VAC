//! Slice 14 — activity stream widget.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use vac_shell_contracts::{Severity, ShellActivityEntry, ShellActivityKind};

#[derive(Debug, Clone, Default)]
pub struct ActivityView {
    pub entries: Vec<ShellActivityEntry>,
    pub scroll: usize,
}

fn kind_label(k: ShellActivityKind) -> &'static str {
    match k {
        ShellActivityKind::UserInput => "user",
        ShellActivityKind::AgentThoughtSummary => "think",
        ShellActivityKind::ToolCall => "tool",
        ShellActivityKind::ToolResult => "tool·ok",
        ShellActivityKind::Diagnostic => "diag",
        ShellActivityKind::Status => "status",
        ShellActivityKind::FileEdit => "edit",
        ShellActivityKind::ShellCommand => "shell",
        ShellActivityKind::ApprovalRequested => "approve?",
        ShellActivityKind::ApprovalResolved => "approve",
        ShellActivityKind::ModelChanged => "model",
        ShellActivityKind::Error => "error",
    }
}

fn severity_color(s: Severity) -> Color {
    match s {
        Severity::Info => Color::Gray,
        Severity::Ok => Color::Green,
        Severity::Warn => Color::Yellow,
        Severity::Error => Color::Red,
    }
}

pub fn render_activity(f: &mut Frame, view: &ActivityView, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Activity ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if view.entries.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  no activity yet",
                Style::default().fg(Color::DarkGray),
            ))),
            inner,
        );
        return;
    }

    let height = inner.height as usize;
    let total = view.entries.len();
    let max_scroll = total.saturating_sub(height);
    let scroll = view.scroll.min(max_scroll);

    let mut lines: Vec<Line<'static>> = Vec::new();
    for entry in view.entries.iter().skip(scroll).take(height) {
        let severity = severity_color(entry.severity);
        let kind = kind_label(entry.kind);
        let mut spans = vec![
            Span::styled(
                format!(" {:<8}", kind),
                Style::default().fg(severity).add_modifier(Modifier::BOLD),
            ),
            Span::styled(entry.title.clone(), Style::default().fg(Color::Gray)),
        ];
        if let Some(d) = &entry.detail {
            spans.push(Span::styled(
                format!("   {d}"),
                Style::default().fg(Color::DarkGray),
            ));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        inner,
    );
}
