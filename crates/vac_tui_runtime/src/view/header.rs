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
                .core.theme
                .style(StyleKey::Warning)
                .add_modifier(Modifier::BOLD),
        ));
    }

    spans.push(Span::styled("VAC", state.core.theme.style(StyleKey::AppTitle)));
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("session {}", &state.session.session_id[..8]),
        state.core.theme.style(StyleKey::Muted),
    ));
    if let Some(title) = &state.session.session_meta.title {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            title.clone(),
            state.core.theme.style(StyleKey::Accent),
        ));
    }

    spans.push(Span::raw(" | "));
    spans.push(Span::styled(
        format!("env:{}", state.layout.switchers.active_isolation_mode),
        state.core.theme.style(StyleKey::Accent),
    ));

    spans.push(Span::raw(" | "));
    spans.push(Span::styled(
        format!("prof:{}", state.layout.switchers.active_profile),
        state
            .core.theme
            .style(StyleKey::Warning)
            .add_modifier(Modifier::BOLD),
    ));

    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!(
            "model {}",
            state
                .operator_config.operator.current_model
                .as_ref()
                .map(|m| m.name.as_str())
                .unwrap_or("-")
        ),
        state.core.theme.style(StyleKey::Muted),
    ));
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        if state.core.view_flags.auto_approve {
            "perm AUTO"
        } else {
            "perm MANUAL"
        },
        if state.core.view_flags.auto_approve {
            state
                .core.theme
                .style(StyleKey::Error)
                .add_modifier(Modifier::BOLD)
        } else {
            state
                .core.theme
                .style(StyleKey::Success)
                .add_modifier(Modifier::BOLD)
        },
    ));

    // VIL Status Badge
    let score = state.vil_domain.vil.status.validation_score;
    let score_label = if score >= 0.9 {
        "A"
    } else if score >= 0.7 {
        "B"
    } else {
        "C"
    };
    let badge_style = if score >= 0.9 {
        state.core.theme.style(StyleKey::Success)
    } else if score >= 0.7 {
        state.core.theme.style(StyleKey::Warning)
    } else {
        state.core.theme.style(StyleKey::Error)
    };

    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("VIL:{}", score_label),
        badge_style.add_modifier(Modifier::BOLD),
    ));

    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("approvals {}", state.execution.approvals.pending_approvals.len()),
        state.core.theme.style(StyleKey::Warning),
    ));
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("review {}", state.workspace.changeset_store.active_entries().len()),
        state.core.theme.style(StyleKey::Accent),
    ));

    let widget = Paragraph::new(Line::from(spans));
    f.render_widget(widget, area);
}
