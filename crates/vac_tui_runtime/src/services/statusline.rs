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

    // Tightened chip set per Wave 3 design:
    //   INPUT | model {name} | {tokens} tok | manual/auto | valid {%}
    //         | lsp E{n} W{n} | profile {name} | rulebook {name}
    // Provider/MCP/SystemPulse facets moved off the statusline — they
    // belong on the operator pane and hydration snapshot, not on the
    // permanent footer where they crowd out per-task signal.
    let theme = &state.core.theme;
    let sep = || Span::raw(" · ");

    let mut text = vec![
        Span::styled(
            format!(" {} ", mode_str),
            theme
                .style(StyleKey::OverlaySelected)
                .add_modifier(Modifier::BOLD),
        ),
        sep(),
        Span::styled("model ", theme.style(StyleKey::Muted)),
        Span::styled(model_str, theme.style(StyleKey::Accent)),
        sep(),
        Span::styled(format!("{} tok", tokens), theme.style(StyleKey::Success)),
        sep(),
        Span::styled(
            if state.core.view_flags.auto_approve {
                "auto"
            } else {
                "manual"
            },
            if state.core.view_flags.auto_approve {
                theme.style(StyleKey::Warning)
            } else {
                theme.style(StyleKey::Success)
            },
        ),
    ];

    // valid % — show even when unknown so the slot is stable
    text.push(sep());
    text.push(Span::styled("valid ", theme.style(StyleKey::Muted)));
    match state.layout.lsp_ui.validation_score {
        Some(score) => {
            let pct = score * 100.0;
            let style = if score >= 0.8 {
                theme.style(StyleKey::Success)
            } else if score >= 0.5 {
                theme.style(StyleKey::Warning)
            } else {
                theme.style(StyleKey::Error)
            };
            text.push(Span::styled(format!("{:.0}%", pct), style));
        }
        None => text.push(Span::styled("—", theme.style(StyleKey::Muted))),
    }

    // lsp E{n} W{n} — always rendered with zero counts when no diagnostics
    text.push(sep());
    text.push(Span::styled("lsp ", theme.style(StyleKey::Muted)));
    let (errs, warns) = state
        .layout
        .lsp_ui
        .lsp_diagnostics
        .as_ref()
        .map(|d| (d.total_errors, d.total_warnings))
        .unwrap_or((0, 0));
    let lsp_style = if errs > 0 {
        theme.style(StyleKey::Error)
    } else if warns > 0 {
        theme.style(StyleKey::Warning)
    } else {
        theme.style(StyleKey::Success)
    };
    text.push(Span::styled(format!("E{} W{}", errs, warns), lsp_style));

    // profile
    text.push(sep());
    text.push(Span::styled("profile ", theme.style(StyleKey::Muted)));
    let profile = state
        .core
        .startup
        .active_profile
        .clone()
        .unwrap_or_else(|| "default".to_string());
    text.push(Span::styled(profile, theme.style(StyleKey::Accent)));

    // rulebook
    text.push(sep());
    text.push(Span::styled("rulebook ", theme.style(StyleKey::Muted)));
    let rulebook = state
        .core
        .startup
        .active_rulebook
        .clone()
        .unwrap_or_else(|| "none".to_string());
    text.push(Span::styled(rulebook, theme.style(StyleKey::Accent)));

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
