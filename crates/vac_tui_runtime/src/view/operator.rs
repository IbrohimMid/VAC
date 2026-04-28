//! Operator panel, activity panel, and related status indicators

use crate::app::WorkspaceFocus;
use crate::app::{ActivityKind, AppState};
use crate::ui::style::focus_style;
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub(super) fn render_operator_panel(f: &mut Frame, state: &mut AppState, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    // O4 — Snapshot loading placeholder. Only visible in the brief
    // window between boot and SessionSnapshotLoaded (typically <10ms
    // for cold FS cache, <1ms warm). Higher priority than `thinking`
    // so operators don't see conflicting indicators.
    if state.session.session_meta.loading {
        let spinner = match state.core.view_flags.spinner_frame % 4 {
            0 => "⠋",
            1 => "⠙",
            2 => "⠹",
            _ => "⠸",
        };
        lines.push(Line::from(vec![
            Span::styled(
                spinner,
                state
                    .core
                    .theme
                    .style(crate::services::theme::StyleKey::Spinner),
            ),
            Span::raw(" "),
            Span::styled(
                "restoring session",
                state
                    .core
                    .theme
                    .style(crate::services::theme::StyleKey::Spinner),
            ),
        ]));
    } else if state.core.loading {
        let spinner = match state.core.view_flags.spinner_frame % 4 {
            0 => "⠋",
            1 => "⠙",
            2 => "⠹",
            _ => "⠸",
        };
        lines.push(Line::from(vec![
            Span::styled(
                spinner,
                state
                    .core
                    .theme
                    .style(crate::services::theme::StyleKey::Spinner),
            ),
            Span::raw(" "),
            Span::styled(
                "thinking",
                state
                    .core
                    .theme
                    .style(crate::services::theme::StyleKey::Spinner),
            ),
        ]));
    } else if state.transcript.streaming.is_streaming {
        let tok_rate = state
            .transcript
            .streaming
            .start
            .map(|start| {
                let elapsed = start.elapsed().as_secs_f32();
                if elapsed > 0.1 {
                    state.transcript.streaming.tokens as f32 / elapsed
                } else {
                    0.0
                }
            })
            .unwrap_or(0.0);
        let rate_str = if tok_rate > 0.0 {
            format!("streaming  {tok_rate:.0} tok/s  (Ctrl+C to cancel)")
        } else {
            "streaming…  (Ctrl+C to cancel)".to_string()
        };
        lines.push(Line::styled(
            rate_str,
            state
                .core
                .theme
                .style(crate::services::theme::StyleKey::Streaming),
        ));
    } else {
        // UX audit: static "idle" gave no clue what to do next for
        // first-run operators. Swap to an action hint when the
        // session has no activity yet; keep plain "idle" once
        // there's conversation history so it doesn't feel noisy
        // mid-session.
        let idle_text = if state.transcript.messages.is_empty()
            && state.core.startup.active_model.is_none()
            && state.operator_config.operator.current_model.is_none()
        {
            "idle — pick a model via /model to start"
        } else if state.transcript.messages.is_empty() {
            "idle — type a message or / for commands"
        } else {
            "idle"
        };
        lines.push(Line::styled(
            idle_text,
            state
                .core
                .theme
                .style(crate::services::theme::StyleKey::Muted),
        ));
    }

    if let Some(snapshot) = &state.execution.runtime.snapshot {
        let (exec_label, exec_style) = match snapshot.execution_environment {
            vac_core::ExecutionEnvironment::Host => (
                "host",
                state
                    .core
                    .theme
                    .style(crate::services::theme::StyleKey::Warning),
            ),
            vac_core::ExecutionEnvironment::IsolatedBatch => (
                "isolated-batch",
                state
                    .core
                    .theme
                    .style(crate::services::theme::StyleKey::Success),
            ),
            vac_core::ExecutionEnvironment::IsolatedInteractive => (
                "isolated-interactive",
                state
                    .core
                    .theme
                    .style(crate::services::theme::StyleKey::Accent),
            ),
        };
        let env_style = if snapshot.environment_mode.contains("trusted-networked") {
            state
                .core
                .theme
                .style(crate::services::theme::StyleKey::Error)
        } else {
            state
                .core
                .theme
                .style(crate::services::theme::StyleKey::Success)
        };
        lines.push(Line::from(vec![
            Span::styled(
                "exec ",
                state
                    .core
                    .theme
                    .style(crate::services::theme::StyleKey::Muted),
            ),
            Span::styled(exec_label, exec_style),
            Span::raw("  "),
            Span::styled(
                "intent ",
                state
                    .core
                    .theme
                    .style(crate::services::theme::StyleKey::Muted),
            ),
            Span::styled(
                snapshot.task_intent_mode.to_string(),
                state
                    .core
                    .theme
                    .style(crate::services::theme::StyleKey::Accent),
            ),
            Span::raw("  "),
            Span::styled(
                "env ",
                state
                    .core
                    .theme
                    .style(crate::services::theme::StyleKey::Muted),
            ),
            Span::styled(snapshot.environment_mode.clone(), env_style),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled(
            "tools ",
            state
                .core
                .theme
                .style(crate::services::theme::StyleKey::Muted),
        ),
        Span::styled(
            format!("{}", state.execution.approvals.pending_tool_calls.len()),
            state
                .core
                .theme
                .style(crate::services::theme::StyleKey::Warning),
        ),
        Span::styled(
            "  approvals ",
            state
                .core
                .theme
                .style(crate::services::theme::StyleKey::Muted),
        ),
        Span::styled(
            format!("{}", state.execution.approvals.pending_approvals.len()),
            state
                .core
                .theme
                .style(crate::services::theme::StyleKey::Warning),
        ),
        Span::styled(
            "  modified ",
            state
                .core
                .theme
                .style(crate::services::theme::StyleKey::Muted),
        ),
        Span::styled(
            format!("{}", state.workspace.changeset_store.active_entries().len()),
            state
                .core
                .theme
                .style(crate::services::theme::StyleKey::Accent),
        ),
    ]));

    if let Some(session) = state.execution.shell.session_store.active() {
        if session.command.is_some() || !session.output.trim().is_empty() {
            let shell_state = if session.command.is_some() {
                if session.backgrounded {
                    "background"
                } else {
                    "active"
                }
            } else if let Some(code) = session.exit_code {
                if code == 0 { "completed" } else { "failed" }
            } else {
                "idle"
            };
            lines.push(Line::from(vec![
                Span::styled(
                    "shell ",
                    state
                        .core
                        .theme
                        .style(crate::services::theme::StyleKey::Muted),
                ),
                Span::styled(
                    shell_state,
                    state
                        .core
                        .theme
                        .style(crate::services::theme::StyleKey::Accent),
                ),
                Span::raw("  "),
                Span::styled(
                    session.label.clone(),
                    state
                        .core
                        .theme
                        .style(crate::services::theme::StyleKey::Muted),
                ),
            ]));
        }

        if !session.output.trim().is_empty() {
            let last = session
                .output
                .lines()
                .rev()
                .take(2)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            lines.push(Line::raw(""));
            for l in last.lines() {
                lines.push(Line::from(vec![
                    Span::styled(
                        "shell ",
                        state
                            .core
                            .theme
                            .style(crate::services::theme::StyleKey::Muted),
                    ),
                    Span::raw(l.to_string()),
                ]));
            }
        }
    }

    if !state.execution.mcp_maps.server_states.is_empty() {
        let connected = state
            .execution
            .mcp_maps
            .server_states
            .values()
            .filter(|s| s.state == vac_mcp_core::McpConnectionState::Connected)
            .count();
        let total = state.execution.mcp_maps.server_states.len();
        let mcp_style = if connected == total {
            state
                .core
                .theme
                .style(crate::services::theme::StyleKey::Success)
        } else {
            state
                .core
                .theme
                .style(crate::services::theme::StyleKey::Warning)
        };
        lines.push(Line::from(vec![
            Span::styled(
                "mcp ",
                state
                    .core
                    .theme
                    .style(crate::services::theme::StyleKey::Muted),
            ),
            Span::styled(format!("{}/{} connected", connected, total), mcp_style),
        ]));
    }

    // Render budget indicator — only shown when over 16ms
    if state.core.render_metrics.ema_render_time_us > 16_000 {
        lines.push(Line::from(vec![
            Span::styled(
                "render ",
                state
                    .core
                    .theme
                    .style(crate::services::theme::StyleKey::Muted),
            ),
            Span::styled(
                format!(
                    "{}ms avg ⚠",
                    state.core.render_metrics.ema_render_time_us / 1000
                ),
                state
                    .core
                    .theme
                    .style(crate::services::theme::StyleKey::Error),
            ),
        ]));
    }

    // U3 — append SystemPulse facet summary so the operator panel
    // speaks the same grammar as the statusline. One line per
    // facet: `<glyph> approvals: 2 pending` (for example). The
    // grammar lives in SystemPulse so changes propagate to every
    // surface without drift.
    {
        let pulse = crate::system_pulse::SystemPulse::from_state(state);
        let facets = pulse.facets();
        if !facets.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::styled(
                "System",
                state
                    .core
                    .theme
                    .style(crate::services::theme::StyleKey::Accent),
            ));
            for facet in facets {
                let style_key = match facet.severity {
                    crate::system_pulse::FacetSeverity::Ok => {
                        crate::services::theme::StyleKey::Success
                    }
                    crate::system_pulse::FacetSeverity::Info => {
                        crate::services::theme::StyleKey::Accent
                    }
                    crate::system_pulse::FacetSeverity::Warn => {
                        crate::services::theme::StyleKey::Warning
                    }
                    crate::system_pulse::FacetSeverity::Critical => {
                        crate::services::theme::StyleKey::Error
                    }
                };
                let glyph = facet.severity.glyph().to_string();
                let label = facet.kind.label();
                // Render one compact summary row per facet: the
                // detail_rows already exist on the facet for a
                // future expanded overlay; here we keep it to a
                // single line per facet.
                let summary = facet.detail_rows.first().cloned().unwrap_or_default();
                lines.push(Line::from(vec![
                    Span::styled(glyph, state.core.theme.style(style_key)),
                    Span::raw(" "),
                    Span::styled(label.to_string(), state.core.theme.style(style_key)),
                    Span::raw("  "),
                    Span::raw(summary),
                ]));
            }
        }
    }

    let widget = Paragraph::new(lines)
        .block(
            Block::default().borders(Borders::ALL).title(Span::styled(
                "Operator",
                state
                    .core
                    .theme
                    .style(crate::services::theme::StyleKey::Accent),
            )),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(widget, area);
}

pub(super) fn activity_icon(kind: ActivityKind) -> &'static str {
    match kind {
        ActivityKind::Status => "•",
        ActivityKind::Tool => "🔧",
        ActivityKind::Approval => "⚑",
        ActivityKind::Review => "Δ",
        ActivityKind::Session => "⎇",
        ActivityKind::Error => "!",
        ActivityKind::Mcp => "🔌",
        ActivityKind::Isolation => "🛡",
        ActivityKind::Shell => "⚡",
        ActivityKind::Todo => "☑",
    }
}

/// QW.1 — compose one conversation-lane line for a Todo activity
/// item. Pulled out for unit-testability; the real renderer in
/// `render_activity_panel` calls this when it sees a
/// `ActivityKind::Todo` row (consecutive rows stay consecutive,
/// which groups them visually without a separate widget).
pub(crate) fn render_todo_line(item: &crate::app::types::ActivityItem) -> String {
    let (state, text) = crate::app::types::parse_todo_message(&item.message);
    format!("{}  {}", state.glyph(), text)
}

pub(super) fn render_activity_panel(f: &mut Frame, state: &mut AppState, area: Rect) {
    let height = area.height.saturating_sub(2) as usize;
    let total = state.execution.activity.len();
    let max_visible = height.min(total);
    let start = total.saturating_sub(max_visible + state.layout.scroll.activity);
    let end = (start + max_visible).min(total);

    let mut lines: Vec<Line> = Vec::new();
    for item in &state.execution.activity[start..end] {
        let ts = item.at.format("%H:%M:%S").to_string();
        let body = if item.kind == ActivityKind::Todo {
            render_todo_line(item)
        } else {
            item.message.clone()
        };
        lines.push(Line::from(vec![
            Span::styled(
                ts,
                state
                    .core
                    .theme
                    .style(crate::services::theme::StyleKey::Muted),
            ),
            Span::raw(" "),
            Span::styled(
                activity_icon(item.kind),
                state
                    .core
                    .theme
                    .style(crate::services::theme::StyleKey::Warning),
            ),
            Span::raw(" "),
            Span::raw(body),
        ]));
    }
    if lines.is_empty() {
        lines.push(Line::styled(
            "No activity yet. Events, approvals, and runtime updates will appear here.",
            state
                .core
                .theme
                .style(crate::services::theme::StyleKey::Muted),
        ));
    }

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(Span::styled(
            "Activity",
            focus_style(
                state.layout.focus == WorkspaceFocus::Activity,
                &state.core.theme,
            ),
        )))
        .wrap(Wrap { trim: true });
    f.render_widget(widget, area);
}
