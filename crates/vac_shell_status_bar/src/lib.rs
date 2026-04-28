//! Slice 19 — global status bar widget.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use vac_shell_contracts::ShellStatusView;

pub fn render_status_bar(f: &mut Frame, view: &ShellStatusView, area: Rect) {
    let muted = Color::DarkGray;
    let accent = Color::Cyan;
    let warn = Color::Yellow;
    let err = Color::Red;
    let ok = Color::Green;

    let mut spans: Vec<Span<'static>> = Vec::new();

    // surface
    spans.push(Span::styled(
        format!(
            " {} ",
            view.surface.as_deref().unwrap_or("chat").to_uppercase()
        ),
        Style::default()
            .fg(Color::Black)
            .bg(accent)
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::raw("  "));

    // model
    spans.push(Span::styled("model ", Style::default().fg(muted)));
    spans.push(Span::styled(
        view.model_label.clone().unwrap_or_else(|| "—".into()),
        Style::default().fg(accent),
    ));
    spans.push(Span::raw("  "));

    // cwd
    spans.push(Span::styled("cwd ", Style::default().fg(muted)));
    spans.push(Span::styled(
        view.cwd.clone(),
        Style::default().fg(Color::Gray),
    ));
    spans.push(Span::raw("  "));

    // git branch (optional)
    if let Some(branch) = &view.git_branch {
        spans.push(Span::styled("git ", Style::default().fg(muted)));
        spans.push(Span::styled(
            branch.clone(),
            Style::default().fg(Color::Magenta),
        ));
        spans.push(Span::raw("  "));
    }

    // approvals
    spans.push(Span::styled("approvals ", Style::default().fg(muted)));
    if view.pending_approvals > 0 {
        spans.push(Span::styled(
            view.pending_approvals.to_string(),
            Style::default().fg(warn).add_modifier(Modifier::BOLD),
        ));
    } else {
        spans.push(Span::styled("0", Style::default().fg(ok)));
    }
    spans.push(Span::raw("  "));

    // running tasks
    spans.push(Span::styled("tasks ", Style::default().fg(muted)));
    spans.push(Span::styled(
        view.running_tasks.to_string(),
        if view.running_tasks > 0 {
            Style::default().fg(accent)
        } else {
            Style::default().fg(muted)
        },
    ));

    // last error
    if let Some(e) = &view.last_error {
        spans.push(Span::raw("   "));
        spans.push(Span::styled(
            "error ",
            Style::default().fg(err).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(e.clone(), Style::default().fg(Color::Gray)));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
