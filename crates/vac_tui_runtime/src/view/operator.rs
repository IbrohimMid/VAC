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
    if state.session_loading {
        let spinner = match state.view_flags.spinner_frame % 4 {
            0 => "⠋",
            1 => "⠙",
            2 => "⠹",
            _ => "⠸",
        };
        lines.push(Line::from(vec![
            Span::styled(
                spinner,
                state.theme.style(crate::services::theme::StyleKey::Spinner),
            ),
            Span::raw(" "),
            Span::styled(
                "restoring session",
                state.theme.style(crate::services::theme::StyleKey::Spinner),
            ),
        ]));
    } else if state.loading {
        let spinner = match state.view_flags.spinner_frame % 4 {
            0 => "⠋",
            1 => "⠙",
            2 => "⠹",
            _ => "⠸",
        };
        lines.push(Line::from(vec![
            Span::styled(
                spinner,
                state.theme.style(crate::services::theme::StyleKey::Spinner),
            ),
            Span::raw(" "),
            Span::styled(
                "thinking",
                state.theme.style(crate::services::theme::StyleKey::Spinner),
            ),
        ]));
    } else if state.streaming.is_streaming {
        let tok_rate = state
            .streaming.start
            .map(|start| {
                let elapsed = start.elapsed().as_secs_f32();
                if elapsed > 0.1 {
                    state.streaming.tokens as f32 / elapsed
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
                .theme
                .style(crate::services::theme::StyleKey::Streaming),
        ));
    } else {
        lines.push(Line::styled(
            "idle",
            state.theme.style(crate::services::theme::StyleKey::Muted),
        ));
    }

    if let Some(snapshot) = &state.runtime.snapshot {
        let (exec_label, exec_style) = match snapshot.execution_environment {
            vac_core::ExecutionEnvironment::Host => (
                "host",
                state.theme.style(crate::services::theme::StyleKey::Warning),
            ),
            vac_core::ExecutionEnvironment::IsolatedBatch => (
                "isolated-batch",
                state.theme.style(crate::services::theme::StyleKey::Success),
            ),
            vac_core::ExecutionEnvironment::IsolatedInteractive => (
                "isolated-interactive",
                state.theme.style(crate::services::theme::StyleKey::Accent),
            ),
        };
        let env_style = if snapshot.environment_mode.contains("trusted-networked") {
            state.theme.style(crate::services::theme::StyleKey::Error)
        } else {
            state.theme.style(crate::services::theme::StyleKey::Success)
        };
        lines.push(Line::from(vec![
            Span::styled(
                "exec ",
                state.theme.style(crate::services::theme::StyleKey::Muted),
            ),
            Span::styled(exec_label, exec_style),
            Span::raw("  "),
            Span::styled(
                "intent ",
                state.theme.style(crate::services::theme::StyleKey::Muted),
            ),
            Span::styled(
                snapshot.task_intent_mode.to_string(),
                state.theme.style(crate::services::theme::StyleKey::Accent),
            ),
            Span::raw("  "),
            Span::styled(
                "env ",
                state.theme.style(crate::services::theme::StyleKey::Muted),
            ),
            Span::styled(snapshot.environment_mode.clone(), env_style),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled(
            "tools ",
            state.theme.style(crate::services::theme::StyleKey::Muted),
        ),
        Span::styled(
            format!("{}", state.approvals.pending_tool_calls.len()),
            state.theme.style(crate::services::theme::StyleKey::Warning),
        ),
        Span::styled(
            "  approvals ",
            state.theme.style(crate::services::theme::StyleKey::Muted),
        ),
        Span::styled(
            format!("{}", state.approvals.pending_approvals.len()),
            state.theme.style(crate::services::theme::StyleKey::Warning),
        ),
        Span::styled(
            "  modified ",
            state.theme.style(crate::services::theme::StyleKey::Muted),
        ),
        Span::styled(
            format!("{}", state.changeset_store.active_entries().len()),
            state.theme.style(crate::services::theme::StyleKey::Accent),
        ),
    ]));

    if let Some(session) = state.shell.session_store.active() {
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
                    state.theme.style(crate::services::theme::StyleKey::Muted),
                ),
                Span::styled(
                    shell_state,
                    state.theme.style(crate::services::theme::StyleKey::Accent),
                ),
                Span::raw("  "),
                Span::styled(
                    session.label.clone(),
                    state.theme.style(crate::services::theme::StyleKey::Muted),
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
                        state.theme.style(crate::services::theme::StyleKey::Muted),
                    ),
                    Span::raw(l.to_string()),
                ]));
            }
        }
    }

    if !state.mcp_server_states.is_empty() {
        let connected = state
            .mcp_server_states
            .values()
            .filter(|s| s.is_connected())
            .count();
        let total = state.mcp_server_states.len();
        let mcp_style = if connected == total {
            state.theme.style(crate::services::theme::StyleKey::Success)
        } else {
            state.theme.style(crate::services::theme::StyleKey::Warning)
        };
        lines.push(Line::from(vec![
            Span::styled(
                "mcp ",
                state.theme.style(crate::services::theme::StyleKey::Muted),
            ),
            Span::styled(format!("{}/{} connected", connected, total), mcp_style),
        ]));
    }

    // Render budget indicator — only shown when over 16ms
    if state.render_metrics.ema_render_time_us > 16_000 {
        lines.push(Line::from(vec![
            Span::styled(
                "render ",
                state.theme.style(crate::services::theme::StyleKey::Muted),
            ),
            Span::styled(
                format!("{}ms avg ⚠", state.render_metrics.ema_render_time_us / 1000),
                state.theme.style(crate::services::theme::StyleKey::Error),
            ),
        ]));
    }

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(Span::styled(
            "Operator",
            state.theme.style(crate::services::theme::StyleKey::Accent),
        )))
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
    }
}

pub(super) fn render_activity_panel(f: &mut Frame, state: &mut AppState, area: Rect) {
    let height = area.height.saturating_sub(2) as usize;
    let total = state.activity.len();
    let max_visible = height.min(total);
    let start = total.saturating_sub(max_visible + state.scroll.activity);
    let end = (start + max_visible).min(total);

    let mut lines: Vec<Line> = Vec::new();
    for item in &state.activity[start..end] {
        let ts = item.at.format("%H:%M:%S").to_string();
        lines.push(Line::from(vec![
            Span::styled(
                ts,
                state.theme.style(crate::services::theme::StyleKey::Muted),
            ),
            Span::raw(" "),
            Span::styled(
                activity_icon(item.kind),
                state.theme.style(crate::services::theme::StyleKey::Warning),
            ),
            Span::raw(" "),
            Span::raw(item.message.clone()),
        ]));
    }
    if lines.is_empty() {
        lines.push(Line::styled(
            "No activity yet. Events, approvals, and runtime updates will appear here.",
            state.theme.style(crate::services::theme::StyleKey::Muted),
        ));
    }

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(Span::styled(
            "Activity",
            focus_style(state.focus == WorkspaceFocus::Activity, &state.theme),
        )))
        .wrap(Wrap { trim: true });
    f.render_widget(widget, area);
}
