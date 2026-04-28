//! Slice 14 — activity stream widget.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use vac_shell_contracts::{Severity, ShellActivityEntry, ShellActivityFilter, ShellActivityKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogsBrowserKey {
    ScrollUp,
    ScrollDown,
    FilterAll,
    FilterErrors,
    FilterWarnings,
    FilterStatus,
    FilterDiagnostics,
    FilterTools,
    FilterApprovals,
    Search,
    Char(char),
    Backspace,
    Escape,
}

pub fn on_logs_browser_key(view: &mut ActivityLogBrowserView, key: LogsBrowserKey) {
    match key {
        LogsBrowserKey::ScrollUp => {
            view.scroll = view.scroll.saturating_sub(1);
        }
        LogsBrowserKey::ScrollDown => {
            view.scroll = view.scroll.saturating_add(1);
        }
        LogsBrowserKey::FilterAll => {
            view.filter = ShellActivityFilter::All;
            view.scroll = 0;
        }
        LogsBrowserKey::FilterErrors => {
            view.filter = ShellActivityFilter::Errors;
            view.scroll = 0;
        }
        LogsBrowserKey::FilterWarnings => {
            view.filter = ShellActivityFilter::Warnings;
            view.scroll = 0;
        }
        LogsBrowserKey::FilterStatus => {
            view.filter = ShellActivityFilter::Status;
            view.scroll = 0;
        }
        LogsBrowserKey::FilterDiagnostics => {
            view.filter = ShellActivityFilter::Diagnostics;
            view.scroll = 0;
        }
        LogsBrowserKey::FilterTools => {
            view.filter = ShellActivityFilter::Tools;
            view.scroll = 0;
        }
        LogsBrowserKey::FilterApprovals => {
            view.filter = ShellActivityFilter::Approvals;
            view.scroll = 0;
        }
        LogsBrowserKey::Search => {
            view.search_mode = true;
        }
        LogsBrowserKey::Char(c) => {
            if view.search_mode {
                view.search_query.push(c);
            }
        }
        LogsBrowserKey::Backspace => {
            if view.search_mode {
                view.search_query.pop();
            }
        }
        LogsBrowserKey::Escape => {
            if view.search_mode {
                view.search_mode = false;
                view.search_query.clear();
            }
        }
    }
}

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

fn severity_label(s: Severity) -> &'static str {
    match s {
        Severity::Info => "info",
        Severity::Ok => "ok",
        Severity::Warn => "warn",
        Severity::Error => "error",
    }
}

fn entry_matches_search(entry: &ShellActivityEntry, query: &str) -> bool {
    let q = query.to_lowercase();
    if entry.title.to_lowercase().contains(&q) {
        return true;
    }
    if entry
        .detail
        .as_ref()
        .is_some_and(|d| d.to_lowercase().contains(&q))
    {
        return true;
    }
    if kind_label(entry.kind).contains(&q) {
        return true;
    }
    if severity_label(entry.severity).contains(&q) {
        return true;
    }
    false
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
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

#[derive(Debug, Clone, Default)]
pub struct ActivityLogBrowserView {
    pub visible: bool,
    pub entries: Vec<ShellActivityEntry>,
    pub filter: ShellActivityFilter,
    pub search_query: String,
    pub search_mode: bool,
    pub scroll: usize,
}

pub fn render_logs_browser(f: &mut Frame, view: &ActivityLogBrowserView, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Logs Browser ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let filter_label = match view.filter {
        ShellActivityFilter::All => "[All] Errors Warnings Status Diag Tools Approvals",
        ShellActivityFilter::Errors => "All [Errors] Warnings Status Diag Tools Approvals",
        ShellActivityFilter::Warnings => "All Errors [Warnings] Status Diag Tools Approvals",
        ShellActivityFilter::Status => "All Errors Warnings [Status] Diag Tools Approvals",
        ShellActivityFilter::Diagnostics => "All Errors Warnings Status [Diag] Tools Approvals",
        ShellActivityFilter::Tools => "All Errors Warnings Status Diag [Tools] Approvals",
        ShellActivityFilter::Approvals => "All Errors Warnings Status Diag Tools [Approvals]",
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            filter_label,
            Style::default().fg(Color::DarkGray),
        ))),
        chunks[0],
    );

    let filtered: Vec<&ShellActivityEntry> = view
        .entries
        .iter()
        .filter(|e| match view.filter {
            ShellActivityFilter::All => true,
            ShellActivityFilter::Errors => e.severity == Severity::Error,
            ShellActivityFilter::Warnings => e.severity == Severity::Warn,
            ShellActivityFilter::Status => matches!(
                e.kind,
                ShellActivityKind::Status | ShellActivityKind::ModelChanged
            ),
            ShellActivityFilter::Diagnostics => matches!(e.kind, ShellActivityKind::Diagnostic),
            ShellActivityFilter::Tools => matches!(
                e.kind,
                ShellActivityKind::ToolCall
                    | ShellActivityKind::ToolResult
                    | ShellActivityKind::ShellCommand
            ),
            ShellActivityFilter::Approvals => matches!(
                e.kind,
                ShellActivityKind::ApprovalRequested | ShellActivityKind::ApprovalResolved
            ),
        })
        .filter(|e| {
            if view.search_query.is_empty() {
                true
            } else {
                entry_matches_search(e, &view.search_query)
            }
        })
        .collect();

    if filtered.is_empty() {
        let empty_message = if view.entries.is_empty() {
            "  no log entries"
        } else {
            "  no matching logs"
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                empty_message,
                Style::default().fg(Color::DarkGray),
            ))),
            chunks[1],
        );
    } else {
        let height = chunks[1].height as usize;
        let total = filtered.len();
        let max_scroll = total.saturating_sub(height);
        let scroll = view.scroll.min(max_scroll);

        let mut lines: Vec<Line<'static>> = Vec::new();
        for entry in filtered.iter().skip(scroll).take(height) {
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

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        f.render_widget(paragraph, chunks[1]);
    }

    let total = filtered.len();
    let entries_total = view.entries.len();

    let status_text = if view.search_mode {
        format!(
            "  search: {} | Backspace edit | Esc close",
            view.search_query
        )
    } else {
        format!(
            "  {}/{} entries | ↑↓ scroll | 1-7 filter | / search | Esc close",
            total, entries_total
        )
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            status_text,
            Style::default().fg(Color::DarkGray),
        ))),
        chunks[2],
    );
}
