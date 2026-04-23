//! Header rendering module

use crate::app::AppState;
use crate::services::theme::StyleKey;
use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

pub(super) fn render_header(f: &mut Frame, state: &mut AppState, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();

    if std::env::var("VAC_INSIDE_ISOLATION").is_ok() {
        spans.push(Span::styled(
            "[ISOLATED] ",
            state
                .theme
                .style(StyleKey::Warning)
                .add_modifier(Modifier::BOLD),
        ));
    }

    spans.push(Span::styled("VAC", state.theme.style(StyleKey::AppTitle)));
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("session {}", &state.session_id[..8]),
        state.theme.style(StyleKey::Muted),
    ));
    if let Some(title) = &state.session_title {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            title.clone(),
            state.theme.style(StyleKey::Accent),
        ));
    }

    spans.push(Span::raw(" | "));
    spans.push(Span::styled(
        format!("env:{}", state.switchers.active_isolation_mode),
        state.theme.style(StyleKey::Accent),
    ));

    spans.push(Span::raw(" | "));
    spans.push(Span::styled(
        format!("prof:{}", state.switchers.active_profile),
        state
            .theme
            .style(StyleKey::Warning)
            .add_modifier(Modifier::BOLD),
    ));

    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!(
            "model {}",
            state
                .current_model
                .as_ref()
                .map(|m| m.name.as_str())
                .unwrap_or("-")
        ),
        state.theme.style(StyleKey::Muted),
    ));
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        if state.view_flags.auto_approve {
            "perm AUTO"
        } else {
            "perm MANUAL"
        },
        if state.view_flags.auto_approve {
            state
                .theme
                .style(StyleKey::Error)
                .add_modifier(Modifier::BOLD)
        } else {
            state
                .theme
                .style(StyleKey::Success)
                .add_modifier(Modifier::BOLD)
        },
    ));

    // VIL Status Badge
    let score = state.vil.status.validation_score;
    let score_label = if score >= 0.9 {
        "A"
    } else if score >= 0.7 {
        "B"
    } else {
        "C"
    };
    let badge_style = if score >= 0.9 {
        state.theme.style(StyleKey::Success)
    } else if score >= 0.7 {
        state.theme.style(StyleKey::Warning)
    } else {
        state.theme.style(StyleKey::Error)
    };

    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("VIL:{}", score_label),
        badge_style.add_modifier(Modifier::BOLD),
    ));

    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("approvals {}", state.approvals.pending_approvals.len()),
        state.theme.style(StyleKey::Warning),
    ));
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("review {}", state.changeset_store.active_entries().len()),
        state.theme.style(StyleKey::Accent),
    ));

    let widget = Paragraph::new(Line::from(spans));
    f.render_widget(widget, area);
}
