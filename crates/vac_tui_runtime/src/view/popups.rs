//! Popup windows: shell, footer, @ dropdown, boot skeleton

use crate::app::AppState;
use crate::services::theme::StyleKey;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use super::overlays::centered_rect;

pub(super) fn render_shell_popup(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(80, 55, f.area());
    f.render_widget(Clear, area);

    let active_session = state.execution.shell.session_store.active();
    let title = if let Some(session) = active_session {
        if let Some(shell) = &session.command {
            if session.waiting_for_input {
                format!("Shell [{}] waiting for input", shell.command)
            } else {
                format!("Shell [{}] active", shell.command)
            }
        } else if let Some(code) = session.exit_code {
            format!("Shell {} completed (exit {code})", session.label)
        } else {
            format!("Shell {}", session.label)
        }
    } else {
        "Shell".to_string()
    };

    let mut lines: Vec<Line> = Vec::new();
    let content: Vec<&str> = active_session
        .map(|session| session.output.lines().collect())
        .unwrap_or_default();
    let max_lines = area.height.saturating_sub(4) as usize;
    let start = content.len().saturating_sub(max_lines);
    for line in content.into_iter().skip(start) {
        lines.push(Line::raw(line.to_string()));
    }

    if lines.is_empty() {
        lines.push(Line::styled(
            "Shell session active. Type a command and press Enter to send input.",
            state.core.theme.style(StyleKey::Muted),
        ));
    }

    if let Some(err) = active_session.and_then(|session| session.last_error.as_ref()) {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!("Last error: {err}"),
            state.core.theme.style(StyleKey::Error),
        ));
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "Ctrl+Z backgrounds | /shell-focus restores | /shell-kill terminates",
        state.core.theme.style(StyleKey::Muted),
    ));

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false });
    f.render_widget(widget, area);
}

pub(super) fn render_at_dropdown(f: &mut Frame, state: &mut AppState) {
    let area = f.area();
    let count = state.composer.at_mention.results.len().min(8) as u16;
    if count == 0 {
        return;
    }
    // Position: bottom-left of screen, above footer, width = 50% of screen
    let width = (area.width / 2).max(30).min(area.width.saturating_sub(2));
    let height = count + 2; // border
    let x = area.x + 1;
    let y = area.y + area.height.saturating_sub(height + 2); // above footer

    let rect = Rect {
        x,
        y,
        width,
        height,
    };
    f.render_widget(Clear, rect);

    let items: Vec<ListItem> = state
        .composer
        .at_mention
        .results
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let selected = i
                == state
                    .composer
                    .at_mention
                    .selected_idx
                    .min(state.composer.at_mention.results.len().saturating_sub(1));
            let style = if selected {
                state.core.theme.style(StyleKey::ListSelected)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(path.clone(), style)))
        })
        .collect();

    let query_hint = if state.composer.at_mention.query.is_empty() {
        "@".to_string()
    } else {
        format!("@{}", state.composer.at_mention.query)
    };

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(Span::styled(
        query_hint,
        state.core.theme.style(StyleKey::Accent),
    )));
    f.render_widget(list, rect);
}

pub(super) fn render_footer(f: &mut Frame, state: &mut AppState, area: Rect) {
    // Reject reason prompt takes priority
    if let Some(reason) = &state.execution.approvals.reject_reason_input {
        let hints = vec![
            Span::styled(
                "REJECT REASON ",
                state
                    .core
                    .theme
                    .style(StyleKey::Error)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::raw(reason.as_str()),
            Span::styled("█", state.core.theme.style(StyleKey::Warning)),
            Span::styled(
                "  Enter: confirm  Esc: skip",
                state.core.theme.style(StyleKey::Muted),
            ),
        ];
        let widget = Paragraph::new(Line::from(hints)).wrap(Wrap { trim: true });
        f.render_widget(widget, area);
        return;
    }

    if state.composer.at_mention.trigger_active {
        let hints = vec![
            Span::styled(
                "@ FILE ",
                state
                    .core
                    .theme
                    .style(StyleKey::Accent)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled(
                &state.composer.at_mention.query,
                state.core.theme.style(StyleKey::Normal),
            ),
            Span::styled(
                "  ↑↓: select  Enter: insert  Esc: cancel",
                state.core.theme.style(StyleKey::Muted),
            ),
        ];
        let widget = Paragraph::new(Line::from(hints)).wrap(Wrap { trim: true });
        f.render_widget(widget, area);
        return;
    }

    if state.execution.shell.session_store.popup_visible
        && state
            .execution
            .shell
            .session_store
            .active()
            .and_then(|session| session.command.as_ref())
            .is_some()
    {
        let hints = vec![
            Span::styled(
                "SHELL ",
                state
                    .core
                    .theme
                    .style(StyleKey::Accent)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled(
                "  Ctrl+Z: background  Esc: close  Ctrl+C: kill",
                state.core.theme.style(StyleKey::Muted),
            ),
        ];
        let widget = Paragraph::new(Line::from(hints)).wrap(Wrap { trim: true });
        f.render_widget(widget, area);
        return;
    }

    let ctx = crate::action_registry::ActionContext::from_app_state(state);
    let mut hints = Vec::new();
    for spec in crate::action_registry::footer_specs(ctx) {
        if !(spec.availability)(state) {
            continue;
        }
        if !hints.is_empty() {
            hints.push(Span::raw("  "));
        }
        let key_str = spec.keybindings.join("/");
        hints.push(Span::styled(
            key_str,
            state.core.theme.style(StyleKey::Accent),
        ));
        hints.push(Span::styled(
            format!(": {}  ", spec.title.to_lowercase()),
            state.core.theme.style(StyleKey::Muted),
        ));
    }

    let widget = Paragraph::new(Line::from(hints)).wrap(Wrap { trim: true });
    f.render_widget(widget, area);
}

pub(crate) fn render_boot_skeleton(f: &mut Frame, state: &AppState) {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::widgets::{Block, Borders, Paragraph};

    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    // Header
    let version = &state.core.startup.version;
    let header = Paragraph::new(format!(" VAC v{version} — Starting…")).style(
        state
            .core
            .theme
            .style(StyleKey::Accent)
            .add_modifier(ratatui::style::Modifier::BOLD),
    );
    f.render_widget(header, chunks[0]);

    // Subsystem progress lines
    let provider_line = format!(
        "  Provider    {}",
        if state.core.startup.provider_status == "initializing" {
            "[ loading… ]"
        } else {
            "[ ready    ]"
        }
    );
    let model_line = format!(
        "  Model       {}",
        match state.core.startup.active_model.as_deref() {
            Some(m) => format!("[ {m} ]"),
            None => "[ loading… ]".to_string(),
        }
    );
    let session_line = format!(
        "  Sessions    [ {} loaded ]",
        state.core.startup.session_count
    );
    let vil_line = format!(
        "  VIL engine  {}",
        if state.core.startup.has_vil_engine {
            "[ present  ]"
        } else {
            "[ absent   ]"
        }
    );

    let body = Paragraph::new([provider_line, model_line, session_line, vil_line].join("\n"))
        .block(Block::default().borders(Borders::NONE))
        .style(state.core.theme.style(StyleKey::Muted));
    f.render_widget(body, chunks[1]);

    // Footer hint
    let footer =
        Paragraph::new(" Initializing subsystems…").style(state.core.theme.style(StyleKey::Muted));
    f.render_widget(footer, chunks[2]);
}
