//! Modal overlays: model switcher, file search, changeset, toast, command palette, shortcuts

use crate::app::AppState;
use crate::services::ToastStyle;
use crate::services::theme::StyleKey;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

pub(super) fn render_model_switcher(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(70, 60, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let input = Paragraph::new(Line::from(vec![
        Span::styled("Filter ", state.core.theme.style(StyleKey::Muted)),
        Span::raw(&state.layout.switchers.model_filter),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Model Switcher"),
    );
    f.render_widget(input, chunks[0]);

    let models = state.model_switcher_filtered();
    let items: Vec<ListItem> = models
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let style = if i == state.layout.switchers.model_selected {
                state.core.theme.style(StyleKey::ListSelected)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{}  ", m.provider),
                    state.core.theme.style(StyleKey::Muted),
                ),
                Span::styled(m.name.clone(), style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Models"))
        .highlight_style(state.core.theme.style(StyleKey::ListSelected));
    f.render_widget(list, chunks[1]);
}

pub(super) fn render_file_search(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(80, 70, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let input = Paragraph::new(Line::from(vec![
        Span::styled("Query ", state.core.theme.style(StyleKey::Muted)),
        Span::raw(&state.workspace.file_index.search_query),
    ]))
    .block(Block::default().borders(Borders::ALL).title("File Search"));
    f.render_widget(input, chunks[0]);

    let items: Vec<ListItem> = state
        .workspace.file_index.search_results
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let style = if i == state.workspace.file_index.search_selected_idx {
                state.core.theme.style(StyleKey::ListSelected)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(path.clone(), style)))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Files"))
        .highlight_style(state.core.theme.style(StyleKey::ListSelected));
    f.render_widget(list, chunks[1]);
}

pub(super) fn render_changeset(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(90, 80, f.area());
    f.render_widget(Clear, area);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    let entries = state.workspace.changeset_store.entries();
    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let style = if i == state.workspace.changeset_ui.selected_idx {
                state.core.theme.style(StyleKey::ListSelected)
            } else {
                Style::default()
            };
            let indicator = match entry.state {
                vac_changeset::FileState::Created => "[+]",
                vac_changeset::FileState::Modified => "[~]",
                vac_changeset::FileState::Removed => "[-]",
                vac_changeset::FileState::Reverted => "[✓]",
                vac_changeset::FileState::FailedRestore => "[✗]",
            };
            let indicator_style = match entry.state {
                vac_changeset::FileState::Created => state.core.theme.style(StyleKey::Success),
                vac_changeset::FileState::Modified => state.core.theme.style(StyleKey::Warning),
                vac_changeset::FileState::Removed => state.core.theme.style(StyleKey::Error),
                vac_changeset::FileState::Reverted => state.core.theme.style(StyleKey::Accent),
                vac_changeset::FileState::FailedRestore => state.core.theme.style(StyleKey::Error),
            };
            ListItem::new(Line::from(vec![
                Span::styled(indicator, indicator_style),
                Span::raw(" "),
                Span::styled(entry.path.clone(), style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Changeset"))
        .highlight_style(state.core.theme.style(StyleKey::ListSelected));
    f.render_widget(list, body[0]);

    let width = body[1].width.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();
    if let Some(diff) = &state.workspace.changeset_ui.diff {
        if let Some(err) = &diff.last_error {
            lines.push(Line::styled(
                err.clone(),
                state
                    .core.theme
                    .style(StyleKey::Error)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ));
        } else if let (Some(old), Some(new)) = (&diff.old_content, &diff.new_content) {
            lines.extend(crate::services::preview_file_diff(
                &state.core.theme,
                &diff.path,
                old,
                new,
                width,
            ));
        } else {
            lines.push(Line::styled(
                "No diff available",
                state.core.theme.style(StyleKey::Muted),
            ));
        }
    } else {
        lines.push(Line::styled(
            "Select a file to preview diff",
            state.core.theme.style(StyleKey::Muted),
        ));
    }

    let detail = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Preview"))
        .wrap(Wrap { trim: false })
        .scroll((state.workspace.changeset_ui.diff_scroll as u16, 0));
    f.render_widget(detail, body[1]);
}

pub(super) fn render_toast(f: &mut Frame, state: &mut AppState) {
    let Some(toast) = state.layout.toasts.last() else {
        return;
    };

    let area = f.area();
    let max_width = area.width.saturating_sub(2).min(60);
    let text_width = toast.message.chars().count() as u16;
    let width = (text_width + 4).min(max_width).max(10);
    let height = 3u16.min(area.height.saturating_sub(1)).max(1);
    let x = area.x + area.width.saturating_sub(width + 1);
    let y = area.y + 1;

    let toast_style = match toast.style {
        ToastStyle::Success => state.core.theme.style(StyleKey::ToastSuccess),
        ToastStyle::Error => state.core.theme.style(StyleKey::ToastError),
        ToastStyle::Warning => state.core.theme.style(StyleKey::ToastWarning),
        ToastStyle::Info => state.core.theme.style(StyleKey::ToastInfo),
    };

    let rect = Rect {
        x,
        y,
        width,
        height,
    };
    f.render_widget(Clear, rect);
    let widget = Paragraph::new(Line::from(Span::raw(toast.message.clone())))
        .style(toast_style)
        .block(Block::default().borders(Borders::ALL).style(toast_style))
        .wrap(Wrap { trim: true });
    f.render_widget(widget, rect);
}

pub(super) fn render_command_palette(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(60, 40, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    // Input
    let input = Paragraph::new(Line::from(vec![
        Span::styled("/", state.core.theme.style(StyleKey::Warning)),
        Span::raw(&state.layout.command_palette.input),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Command"));
    f.render_widget(input, chunks[0]);

    // Commands list
    let filtered = state.filtered_commands();
    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .map(|(i, cmd)| {
            let style = if i == state.layout.command_palette.selected {
                state.core.theme.style(StyleKey::ListSelected)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(&cmd.command, style),
                Span::raw(" - "),
                Span::styled(&cmd.description, state.core.theme.style(StyleKey::Muted)),
            ]))
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Commands"));
    f.render_widget(list, chunks[1]);
}

pub(super) fn render_shortcuts(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(70, 60, f.area());
    f.render_widget(Clear, area);

    // U0 + post-audit — shortcuts are derived from ACTION_SPECS and
    // grouped by scope so operators see "where does this chord
    // work?" without reading docs. View-internal chords (Tab / Ctrl+
    // Tab / PageUp-Down / arrows / Enter / Esc) are appended
    // separately since they aren't registry-backed actions.
    let mut grouped: std::collections::BTreeMap<&'static str, Vec<String>> =
        std::collections::BTreeMap::new();
    for spec in crate::action_registry::ACTION_SPECS.iter() {
        if spec.keybindings.is_empty() {
            continue;
        }
        if !(spec.availability)(state) {
            continue;
        }
        let scope_label: &'static str = match spec.scope {
            crate::action_ids::ActionContext::Global => "Global",
            crate::action_ids::ActionContext::InputFocus => "Input focus",
            crate::action_ids::ActionContext::ConversationFocus => "Conversation focus",
            crate::action_ids::ActionContext::ActivityFocus => "Activity focus",
            crate::action_ids::ActionContext::WorkbenchApprovals => "Approvals tab",
            crate::action_ids::ActionContext::WorkbenchReview => "Review tab",
            crate::action_ids::ActionContext::WorkbenchSessions => "Sessions tab",
            crate::action_ids::ActionContext::WorkbenchAgents => "Agents tab",
            crate::action_ids::ActionContext::WorkbenchRuntime => "Runtime tab",
            crate::action_ids::ActionContext::WorkbenchPlan => "Plan tab",
            crate::action_ids::ActionContext::WorkbenchVil => "VIL tab",
            crate::action_ids::ActionContext::WorkbenchVwfd => "VWFD tab",
            crate::action_ids::ActionContext::WorkbenchAny => "Any workbench tab",
            crate::action_ids::ActionContext::OverlayActive => "Overlay",
        };
        let chords = spec.keybindings.join(" / ");
        grouped
            .entry(scope_label)
            .or_default()
            .push(format!("  {chords:<18}  {} — {}", spec.title, spec.description));
    }

    let mut rows: Vec<String> = Vec::new();
    // "Global" first, then the rest in BTreeMap order. This keeps
    // the most-used chords near the top of the popup.
    if let Some(global) = grouped.remove("Global") {
        rows.push("[Global]".into());
        rows.extend(global);
        rows.push(String::new());
    }
    for (scope, lines) in &grouped {
        rows.push(format!("[{scope}]"));
        rows.extend(lines.clone());
        rows.push(String::new());
    }

    rows.push("[View-internal navigation]".into());
    rows.push("  Tab                 Cycle focus pane".into());
    rows.push("  Ctrl+Tab            Cycle workbench tabs".into());
    rows.push("  PageUp / PageDown   Scroll pane content".into());
    rows.push("  Up / Down           List / history navigation".into());
    rows.push("  Enter               Submit / confirm".into());
    rows.push("  Esc                 Cancel / close overlay".into());

    let items: Vec<ListItem> = rows
        .iter()
        .map(|s| ListItem::new(Line::raw(s.clone())))
        .collect();
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Shortcuts (Esc to close)"),
    );
    f.render_widget(list, area);
}

pub(super) fn render_context_inspector(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(60, 50, f.area());
    f.render_widget(Clear, area);

    let b = &state.operator_config.billing;
    let input = b.total_session.input_tokens;
    let output = b.total_session.output_tokens;
    let total = input.saturating_add(output).max(1);

    let bar = |n: u64, label: &str, width: u16| -> Line<'static> {
        let fraction = (n as f64) / (total as f64);
        let filled = ((fraction * width as f64).round() as u16).min(width);
        let empty = width.saturating_sub(filled);
        Line::from(vec![
            Span::raw(format!("{label:<10}")),
            Span::raw("│"),
            Span::raw("█".repeat(filled as usize)),
            Span::raw(" ".repeat(empty as usize)),
            Span::raw("│ "),
            Span::raw(format!("{n:>8} ({:.1}%)", fraction * 100.0)),
        ])
    };

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::raw(format!(
        "Context usage: {:.1}%",
        b.context_usage_percent,
    )));
    lines.push(Line::raw(""));
    lines.push(bar(input, "input", 30));
    lines.push(bar(output, "output", 30));
    lines.push(Line::raw(""));
    lines.push(Line::raw(format!(
        "Total session:   {}",
        b.total_session.total_tokens,
    )));
    lines.push(Line::raw(format!(
        "Current message: {}",
        b.current_message.total_tokens,
    )));
    lines.push(Line::raw(""));
    lines.push(Line::raw("[Esc] close"));

    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Context inspector (/context)"),
        );
    f.render_widget(para, area);
}

pub(super) fn render_elicitation(f: &mut Frame, state: &mut AppState) {
    let Some(prompt) = state.layout.elicitation.as_ref() else {
        return;
    };
    let area = centered_rect(60, 30, f.area());
    f.render_widget(Clear, area);

    let mut lines: Vec<Line> = Vec::new();
    if let Some(p) = prompt.prompt.as_deref() {
        lines.push(Line::raw(p.to_string()));
        lines.push(Line::raw(""));
    }
    lines.push(Line::from(vec![
        Span::raw("URL: "),
        Span::styled(
            prompt.url.clone(),
            Style::default(),
        ),
    ]));
    lines.push(Line::raw(""));
    lines.push(Line::raw("[Enter] open in browser   [Esc] cancel"));

    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("MCP elicitation"),
        );
    f.render_widget(para, area);
}

pub(crate) fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
