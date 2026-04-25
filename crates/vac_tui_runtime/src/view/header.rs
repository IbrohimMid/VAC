//! Header rendering module

use crate::app::AppState;
use crate::services::theme::StyleKey;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

pub(super) fn render_header(f: &mut Frame, state: &mut AppState, area: Rect) {
    // Wave 3 #02 — split the header into two lines:
    //   line 1: the existing chip strip (session/env/profile/model/perm/VIL/…)
    //   line 2: surface tabs + readiness indicator
    // Tabs are visual-only for now (chat is the only live surface);
    // the runtime/review/workbench/mcp surfaces land in later patches.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

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
    f.render_widget(widget, rows[0]);

    render_tab_strip(f, state, rows[1]);
}

fn render_tab_strip(f: &mut Frame, state: &mut AppState, area: Rect) {
    let theme = &state.core.theme;
    let active_idx = match state.layout.surface {
        crate::app::types::Surface::Chat => 0,
        crate::app::types::Surface::Runtime => 1,
    };

    // Only chat + runtime are live surfaces today; review/workbench/mcp
    // remain workbench-internal tabs until their own surface ships.
    let tabs = [
        ("ctrl+1", "chat"),
        ("ctrl+2", "runtime"),
        ("—", "review"),
        ("—", "workbench"),
        ("—", "mcp"),
    ];

    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::raw(" "));
    for (i, (key, label)) in tabs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", theme.style(StyleKey::Muted)));
        }
        let is_active = i == active_idx;
        let key_style = if is_active {
            theme.style(StyleKey::Accent).add_modifier(Modifier::BOLD)
        } else {
            theme.style(StyleKey::Muted)
        };
        let label_style = if is_active {
            theme
                .style(StyleKey::OverlaySelected)
                .add_modifier(Modifier::BOLD)
        } else {
            theme.style(StyleKey::Muted)
        };
        spans.push(Span::styled(format!("[{}]", key), key_style));
        spans.push(Span::raw(" "));
        spans.push(Span::styled((*label).to_string(), label_style));
    }

    // Right-aligned readiness chunk: VIL-native ● ready · engine connected · rulebook X enforcing
    let ready_dot = match state.core.startup.provider_status.as_str() {
        s if s.starts_with("ready") => ("●", StyleKey::Success),
        "loading..." | "initializing" => ("●", StyleKey::Warning),
        _ => ("●", StyleKey::Error),
    };
    let rulebook = state
        .core
        .startup
        .active_rulebook
        .clone()
        .unwrap_or_else(|| "none".to_string());
    let engine_label = if state.core.startup.has_vil_engine {
        "engine connected"
    } else {
        "engine offline"
    };

    let right_spans = vec![
        Span::styled("VIL-native ", theme.style(StyleKey::Accent)),
        Span::styled(ready_dot.0, theme.style(ready_dot.1)),
        Span::styled(" ready", theme.style(StyleKey::Success)),
        Span::styled(" · ", theme.style(StyleKey::Muted)),
        Span::styled(engine_label, theme.style(StyleKey::Muted)),
        Span::styled(" · ", theme.style(StyleKey::Muted)),
        Span::styled("rulebook ", theme.style(StyleKey::Muted)),
        Span::styled(rulebook, theme.style(StyleKey::Accent)),
        Span::styled(" enforcing ", theme.style(StyleKey::Muted)),
    ];

    // Compute widths to lay tabs left, ready right.
    let left_w: u16 = spans
        .iter()
        .map(|s| s.content.chars().count() as u16)
        .sum();
    let right_w: u16 = right_spans
        .iter()
        .map(|s| s.content.chars().count() as u16)
        .sum();

    let avail = area.width;
    let pad = avail.saturating_sub(left_w).saturating_sub(right_w);
    if pad > 0 {
        spans.push(Span::raw(" ".repeat(pad as usize)));
    } else {
        spans.push(Span::raw(" "));
    }
    spans.extend(right_spans);

    let widget = Paragraph::new(Line::from(spans));
    f.render_widget(widget, area);
}
