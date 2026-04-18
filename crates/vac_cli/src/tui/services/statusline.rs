use crate::tui::app::AppState;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub fn render_statusline(f: &mut Frame, state: &AppState, area: Rect) {
    let mode_str = match state.focus {
        crate::tui::app::WorkspaceFocus::Input => "INPUT",
        crate::tui::app::WorkspaceFocus::Conversation => "CONVERSATION",
        crate::tui::app::WorkspaceFocus::Activity => "ACTIVITY",
        crate::tui::app::WorkspaceFocus::Workbench => "WORKBENCH",
    };

    let model_str = state
        .current_model
        .as_ref()
        .map(|m| m.name.as_str())
        .unwrap_or("none");

    let tokens = state.total_session_usage.total_tokens;

    let text = vec![
        Span::styled(
            format!(" {} ", mode_str),
            Style::default()
                .bg(Color::Blue)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::styled(format!("Model: {}", model_str), Style::default().fg(Color::Cyan)),
        Span::raw(" | "),
        Span::styled(format!("Tokens: {}", tokens), Style::default().fg(Color::Green)),
        Span::raw(" | "),
        Span::styled(
            if state.auto_approve { "AUTO-APPROVE" } else { "MANUAL" },
            if state.auto_approve {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            },
        ),
    ];

    let widget = Paragraph::new(Line::from(text)).alignment(Alignment::Left);
    f.render_widget(widget, area);
}
