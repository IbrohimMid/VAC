use crate::app::AppState;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

pub fn render_statusline(f: &mut Frame, state: &AppState, area: Rect) {
    let mode_str = match state.focus {
        crate::app::WorkspaceFocus::Input => "INPUT",
        crate::app::WorkspaceFocus::Conversation => "CONVERSATION",
        crate::app::WorkspaceFocus::Activity => "ACTIVITY",
        crate::app::WorkspaceFocus::Workbench => "WORKBENCH",
    };

    let model_str = state
        .current_model
        .as_ref()
        .map(|m| m.name.as_str())
        .unwrap_or("Pending initialization");

    let tokens = state.total_session_usage.total_tokens;

    let mut text = vec![
        Span::styled(
            format!(" {} ", mode_str),
            Style::default()
                .bg(Color::Blue)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::styled(
            format!("Model: {}", model_str),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw(" | "),
        Span::styled(
            format!("Tokens: {}", tokens),
            Style::default().fg(Color::Green),
        ),
        Span::raw(" | "),
        Span::styled(
            if state.auto_approve {
                "AUTO-APPROVE"
            } else {
                "MANUAL"
            },
            if state.auto_approve {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            },
        ),
    ];

    if let Some(score) = state.validation_score {
        text.push(Span::raw(" | "));
        text.push(Span::styled(
            format!("Valid: {:.1}%", score * 100.0),
            if score >= 0.8 {
                Style::default().fg(Color::Green)
            } else if score >= 0.5 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Red)
            },
        ));
    }

    if state.lsp_available {
        text.push(Span::raw(" | "));
        text.push(Span::styled("LSP", Style::default().fg(Color::Blue)));
        if let Some(diag) = &state.lsp_diagnostics {
            let errs = diag.total_errors;
            let warns = diag.total_warnings;
            if errs > 0 || warns > 0 {
                text.push(Span::raw(format!(" E:{} W:{}", errs, warns)));
            } else {
                text.push(Span::raw(" OK"));
            }
        }
    }

    let widget = Paragraph::new(Line::from(text)).alignment(Alignment::Left);
    f.render_widget(widget, area);
}
