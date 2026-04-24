use crate::app::AppState;
use crate::services::theme::StyleKey;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

pub(crate) fn model_label(state: &AppState) -> String {
    match state.operator_config.operator.current_model.as_ref() {
        Some(model) => model.name.clone(),
        None => match state.core.startup.default_model.as_ref() {
            Some(default) => format!("no active model selected (default: {default})"),
            None => "no active model selected".to_string(),
        },
    }
}

pub fn render_statusline(f: &mut Frame, state: &AppState, area: Rect) {
    let mode_str = match state.layout.focus {
        crate::app::WorkspaceFocus::Input => "INPUT",
        crate::app::WorkspaceFocus::Conversation => "CONVERSATION",
        crate::app::WorkspaceFocus::Activity => "ACTIVITY",
        crate::app::WorkspaceFocus::Workbench => "WORKBENCH",
    };

    let model_str = model_label(state);

    let tokens = state.operator_config.billing.total_session.total_tokens;

    let mut text = vec![
        Span::styled(
            format!(" {} ", mode_str),
            state
                .core.theme
                .style(StyleKey::OverlaySelected)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::styled(
            format!("Model: {}", model_str),
            state.core.theme.style(StyleKey::Accent),
        ),
        Span::raw(" | "),
        Span::styled(
            format!("Tokens: {}", tokens),
            state.core.theme.style(StyleKey::Success),
        ),
        Span::raw(" | "),
        Span::styled(
            if state.core.view_flags.auto_approve {
                "AUTO-APPROVE"
            } else {
                "MANUAL"
            },
            if state.core.view_flags.auto_approve {
                state.core.theme.style(StyleKey::Error)
            } else {
                state.core.theme.style(StyleKey::Success)
            },
        ),
    ];

    if let Some(score) = state.layout.lsp_ui.validation_score {
        text.push(Span::raw(" | "));
        text.push(Span::styled(
            format!("Valid: {:.1}%", score * 100.0),
            if score >= 0.8 {
                state.core.theme.style(StyleKey::Success)
            } else if score >= 0.5 {
                state.core.theme.style(StyleKey::Warning)
            } else {
                state.core.theme.style(StyleKey::Error)
            },
        ));
    }

    // Phase 3: Show provider/auth status from startup snapshot
    {
        let provider_status = &state.core.startup.provider_status;
        let (provider_label, provider_key) = if provider_status.starts_with("ready") {
            (provider_status.as_str(), StyleKey::Success)
        } else if provider_status == "initializing" || provider_status == "loading..." {
            (provider_status.as_str(), StyleKey::Warning)
        } else {
            (provider_status.as_str(), StyleKey::Error)
        };
        text.push(Span::raw(" | "));
        text.push(Span::styled(
            format!("Provider: {}", provider_label),
            state.core.theme.style(provider_key),
        ));
    }
    if state.core.startup.mcp_server_count > 0 {
        text.push(Span::raw(" | "));
        text.push(Span::styled(
            format!("MCP: {}", state.core.startup.mcp_server_count),
            state.core.theme.style(StyleKey::Accent),
        ));
    }

    if state.layout.lsp_ui.lsp_available {
        text.push(Span::raw(" | "));
        text.push(Span::styled("LSP", state.core.theme.style(StyleKey::Accent)));
        if let Some(diag) = &state.layout.lsp_ui.lsp_diagnostics {
            let errs = diag.total_errors;
            let warns = diag.total_warnings;
            if errs > 0 || warns > 0 {
                text.push(Span::raw(format!(" E:{} W:{}", errs, warns)));
            } else {
                text.push(Span::raw(" OK"));
            }
        }
    }

    // U2 — append SystemPulse tokens. Each facet gets its own
    // severity-coloured span so the statusline reflects
    // approvals / runtime / MCP health at a glance, using the same
    // grammar the operator panel + future overlays consume.
    let pulse = crate::system_pulse::SystemPulse::from_state(state);
    for facet in pulse.facets() {
        text.push(Span::raw(" · "));
        let style_key = match facet.severity {
            crate::system_pulse::FacetSeverity::Ok => StyleKey::Success,
            crate::system_pulse::FacetSeverity::Info => StyleKey::Accent,
            crate::system_pulse::FacetSeverity::Warn => StyleKey::Warning,
            crate::system_pulse::FacetSeverity::Critical => StyleKey::Error,
        };
        text.push(Span::styled(
            facet.compact_token.as_ref().to_string(),
            state.core.theme.style(style_key),
        ));
    }

    let widget = Paragraph::new(Line::from(text)).alignment(Alignment::Left);
    f.render_widget(widget, area);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::app::{AppState, AppStateOptions};

    #[test]
    fn model_label_prefers_active_model() {
        let state = AppState::new(AppStateOptions {
            model: Some(crate::types::Model {
                id: "m1".to_string(),
                name: "Primary".to_string(),
                provider: "anthropic".to_string(),
                supports_reasoning: false,
                ..Default::default()
            }),
            session_id: None,
            checkpoint_path: None,
            project_root: std::env::current_dir().unwrap(),
        });

        assert_eq!(model_label(&state), "Primary");
    }

    #[test]
    fn model_label_reports_missing_active_model_with_default_hint() {
        let mut state = AppState::default();
        state.core.startup.default_model = Some("claude-4".to_string());

        assert_eq!(
            model_label(&state),
            "no active model selected (default: claude-4)"
        );
    }
}
