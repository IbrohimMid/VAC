use crate::services::theme::StyleKey;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{AppState, SidePanelSection};

pub fn render_side_panel(f: &mut Frame, state: &mut AppState, area: Rect) {
    f.render_widget(ratatui::widgets::Clear, area);

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(state.theme.style(StyleKey::Muted));
    let inner_area = block.inner(area);
    f.render_widget(block, area);

    let padded_area = Rect {
        x: inner_area.x,
        y: inner_area.y.saturating_add(1),
        width: inner_area.width,
        height: inner_area.height.saturating_sub(2),
    };

    let context_collapsed = state
        .side_panel.section_collapsed
        .contains(&SidePanelSection::Context);
    let runtime_collapsed = state
        .side_panel.section_collapsed
        .contains(&SidePanelSection::Runtime);
    let mcp_collapsed = state
        .side_panel.section_collapsed
        .contains(&SidePanelSection::Mcp);
    let sessions_collapsed = state
        .side_panel.section_collapsed
        .contains(&SidePanelSection::Sessions);
    let usage_collapsed = state
        .side_panel.section_collapsed
        .contains(&SidePanelSection::Usage);

    let collapsed_height = 1;
    let context_extra = (!state.pins.files.is_empty()) as u16
        + (!state.pins.diffs.is_empty()) as u16
        + (!state.pins.diagnostics.is_empty()) as u16;
    let context_height = if context_collapsed {
        collapsed_height
    } else {
        5 + context_extra
    };
    let runtime_height = if runtime_collapsed {
        collapsed_height
    } else {
        6
    };
    let mcp_lines = state
        .mcp_server_states
        .values()
        .map(|s| {
            if matches!(
                s.status,
                vac_tools::mcp::McpConnectionStatus::Unreachable(_)
            ) {
                2
            } else {
                1
            }
        })
        .sum::<u16>();
    let mcp_height = if mcp_collapsed {
        collapsed_height
    } else {
        (mcp_lines + 2).max(3)
    };
    let sessions_height = if sessions_collapsed {
        collapsed_height
    } else {
        8
    };
    let usage_visible = state.current_message_usage.total_tokens > 0
        || state.total_session_usage.total_tokens > 0
        || state.context_usage_percent > 0.0;
    let usage_height = if !usage_visible {
        0
    } else if usage_collapsed {
        collapsed_height
    } else {
        4
    };
    // Changeset and Todos are summary-only (1 line each); full detail is in workbench tabs.
    let changeset_count = state.changeset_store.active_entries().len();
    let changeset_height: u16 = if changeset_count > 0 { 1 } else { 0 };
    let todos_visible = !state.todos.is_empty();
    let todos_height: u16 = if todos_visible { 1 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(context_height),
            Constraint::Length(usage_height),
            Constraint::Length(sessions_height),
            Constraint::Length(runtime_height),
            Constraint::Length(mcp_height),
            Constraint::Length(changeset_height),
            Constraint::Length(todos_height),
            Constraint::Min(0),
        ])
        .split(padded_area);

    state.side_panel.header_areas.clear();
    state.side_panel.row_areas.clear();
    let mut sections: Vec<(SidePanelSection, Rect)> = vec![(SidePanelSection::Context, chunks[0])];
    if usage_visible {
        sections.push((SidePanelSection::Usage, chunks[1]));
    }
    sections.push((SidePanelSection::Sessions, chunks[2]));
    sections.push((SidePanelSection::Runtime, chunks[3]));
    sections.push((SidePanelSection::Mcp, chunks[4]));
    if changeset_count > 0 {
        sections.push((SidePanelSection::Changeset, chunks[5]));
    }
    if todos_visible {
        sections.push((SidePanelSection::Todos, chunks[6]));
    }
    for (sec, mut rect) in sections {
        rect.height = 1;
        state.side_panel.header_areas.insert(sec, rect);
    }

    render_context_section(f, state, chunks[0], context_collapsed);
    if usage_visible {
        render_usage_section(f, state, chunks[1], usage_collapsed);
    }
    render_sessions_section(f, state, chunks[2], sessions_collapsed);
    render_runtime_section(f, state, chunks[3], runtime_collapsed);
    render_mcp_section(f, state, chunks[4], mcp_collapsed);
    if changeset_count > 0 {
        render_changeset_summary(f, state, chunks[5]);
    }
    if todos_visible {
        render_todos_summary(f, state, chunks[6]);
    }
}

fn render_todos_summary(f: &mut Frame, state: &AppState, area: Rect) {
    use vac_changeset::TodoStatus;
    let pending = state
        .todos
        .iter()
        .filter(|t| t.status != TodoStatus::Done)
        .count();
    let done = state.todos.len().saturating_sub(pending);
    let line = Line::from(vec![
        Span::styled(
            format!("  ▸ Todos ({}/{}) ", done, state.todos.len()),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled("— Workbench", state.theme.style(StyleKey::Muted)),
    ]);
    f.render_widget(Paragraph::new(vec![line]), area);
}

fn render_usage_section(f: &mut Frame, state: &AppState, area: Rect, collapsed: bool) {
    let collapse_indicator = if collapsed { "▸" } else { "▾" };
    let header = Line::from(Span::styled(
        format!("  {} Usage", collapse_indicator),
        Style::default().add_modifier(Modifier::BOLD),
    ));

    if collapsed {
        f.render_widget(Paragraph::new(vec![header]), area);
        return;
    }

    let pct = state.context_usage_percent.clamp(0.0, 100.0);
    let pct_style = if pct >= 80.0 {
        state.theme.style(StyleKey::Error)
    } else if pct >= 50.0 {
        state.theme.style(StyleKey::Warning)
    } else {
        state.theme.style(StyleKey::Success)
    };
    let lines = vec![
        header,
        Line::from(vec![
            Span::styled("    Turn: ", state.theme.style(StyleKey::Muted)),
            Span::raw(format!(
                "{} in / {} out",
                state.current_message_usage.input_tokens, state.current_message_usage.output_tokens
            )),
        ]),
        Line::from(vec![
            Span::styled("    Session: ", state.theme.style(StyleKey::Muted)),
            Span::raw(format!("{} tokens", state.total_session_usage.total_tokens)),
        ]),
        Line::from(vec![
            Span::styled("    Context: ", state.theme.style(StyleKey::Muted)),
            Span::styled(format!("{:.0}%", pct), pct_style),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn render_context_section(f: &mut Frame, state: &AppState, area: Rect, collapsed: bool) {
    let collapse_indicator = if collapsed { "▸" } else { "▾" };
    let header = Line::from(Span::styled(
        format!("  {} Context", collapse_indicator),
        Style::default().add_modifier(Modifier::BOLD),
    ));

    if collapsed {
        f.render_widget(Paragraph::new(vec![header]), area);
        return;
    }

    let mut lines = vec![header];

    let model_name = state
        .operator.current_model
        .as_ref()
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "no active model selected".to_string());
    lines.push(Line::from(vec![
        Span::styled("    Model: ", state.theme.style(StyleKey::Muted)),
        Span::raw(model_name),
    ]));

    let session = state.session_id.chars().take(8).collect::<String>();
    lines.push(Line::from(vec![
        Span::styled("    Session: ", state.theme.style(StyleKey::Muted)),
        Span::raw(session),
    ]));

    let auto = if state.view_flags.auto_approve {
        "Enabled"
    } else {
        "Disabled"
    };
    let auto_style = if state.view_flags.auto_approve {
        state.theme.style(StyleKey::Error)
    } else {
        state.theme.style(StyleKey::Success)
    };
    lines.push(Line::from(vec![
        Span::styled("    Auto-Approve: ", state.theme.style(StyleKey::Muted)),
        Span::styled(auto, auto_style),
    ]));

    if let Some(ident) = state
        .auth_display_info
        .0
        .as_ref()
        .or(state.auth_display_info.1.as_ref())
    {
        lines.push(Line::from(vec![
            Span::styled("    Auth: ", state.theme.style(StyleKey::Muted)),
            Span::raw(ident.clone()),
        ]));
    }

    if !state.pins.files.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("    Files: ", state.theme.style(StyleKey::Muted)),
            Span::raw(compact_items(&state.pins.files, 2)),
        ]));
    }
    if !state.pins.diffs.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("    Diffs: ", state.theme.style(StyleKey::Muted)),
            Span::raw(compact_items(&state.pins.diffs, 2)),
        ]));
    }
    if !state.pins.diagnostics.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("    Diagnostics: ", state.theme.style(StyleKey::Muted)),
            Span::raw(compact_items(&state.pins.diagnostics, 2)),
        ]));
    } else if state.pins.files.is_empty() && state.pins.diffs.is_empty() {
        lines.push(Line::styled(
            "    No pinned context yet. Use /context pin <file> to add one.",
            state
                .theme
                .style(StyleKey::Muted)
                .add_modifier(Modifier::ITALIC),
        ));
    }

    f.render_widget(Paragraph::new(lines), area);
}

fn compact_items(items: &[String], max: usize) -> String {
    let shown = items.iter().take(max).cloned().collect::<Vec<_>>();
    if items.len() > max {
        format!("{}, +{}", shown.join(", "), items.len() - max)
    } else {
        shown.join(", ")
    }
}

fn render_sessions_section(f: &mut Frame, state: &mut AppState, area: Rect, collapsed: bool) {
    let collapse_indicator = if collapsed { "▸" } else { "▾" };
    let header = Line::from(Span::styled(
        format!("  {} Recent Sessions", collapse_indicator),
        Style::default().add_modifier(Modifier::BOLD),
    ));

    if collapsed {
        f.render_widget(Paragraph::new(vec![header]), area);
        return;
    }

    let mut lines = vec![header];

    if state.sessions.is_empty() {
        lines.push(Line::styled(
            "    No sessions loaded yet. Run /sessions to open saved sessions.",
            state
                .theme
                .style(StyleKey::Muted)
                .add_modifier(Modifier::ITALIC),
        ));
    } else {
        for (i, session) in state.sessions.iter().take(5).enumerate() {
            let is_active = session.id == state.session_id;
            let row_style = if is_active {
                state.theme.style(StyleKey::Warning)
            } else {
                state.theme.style(StyleKey::Muted)
            };
            let title = if session.title.is_empty() {
                "Untitled"
            } else {
                &session.title
            };
            let prefix = if is_active { "    * " } else { "      " };
            lines.push(Line::from(vec![
                Span::styled(prefix, row_style),
                Span::styled(title.chars().take(20).collect::<String>(), row_style),
            ]));
            // Track row rect for click handling (header is row 0, sessions start at row 1)
            let row_y = area.y + 1 + i as u16;
            if row_y < area.y + area.height {
                state.side_panel.row_areas.push((
                    crate::app::SidePanelRowAction::SwitchSession(session.id.clone()),
                    Rect::new(area.x, row_y, area.width, 1),
                ));
            }
        }
    }

    f.render_widget(Paragraph::new(lines), area);
}

fn render_mcp_section(f: &mut Frame, state: &mut AppState, area: Rect, collapsed: bool) {
    let collapse_indicator = if collapsed { "▸" } else { "▾" };
    let connected = state
        .mcp_server_states
        .values()
        .filter(|s| s.is_connected())
        .count();
    let total = state.mcp_server_states.len();
    let header = Line::from(Span::styled(
        format!(
            "  {} MCP Servers ({}/{})",
            collapse_indicator, connected, total
        ),
        Style::default().add_modifier(Modifier::BOLD),
    ));

    if collapsed {
        f.render_widget(Paragraph::new(vec![header]), area);
        return;
    }

    let mut lines = vec![header];

    if total == 0 {
        lines.push(Line::styled(
            "    No MCP servers configured.",
            state
                .theme
                .style(StyleKey::Muted)
                .add_modifier(Modifier::ITALIC),
        ));
    } else {
        let mut row_offset = 1u16; // header is row 0
        for (name, conn_state) in &state.mcp_server_states {
            // Track row for click
            let row_y = area.y + row_offset;
            if row_y < area.y + area.height {
                state.side_panel.row_areas.push((
                    crate::app::SidePanelRowAction::ShowMcpDetail(name.clone()),
                    Rect::new(area.x, row_y, area.width, 1),
                ));
            }
            row_offset += if matches!(
                conn_state.status,
                vac_tools::mcp::McpConnectionStatus::Unreachable(_)
            ) {
                2
            } else {
                1
            };
            let (status, status_key) = if conn_state.is_connected() {
                ("✅", StyleKey::Success)
            } else {
                ("❌", StyleKey::Error)
            };

            let mut line_spans = vec![
                Span::raw("    "),
                Span::styled(status, state.theme.style(status_key)),
                Span::raw(" "),
                Span::styled(name.clone(), state.theme.style(StyleKey::Warning)),
            ];

            if let Some(trust) = &conn_state.trust_class {
                let (trust_badge, trust_key) = match trust {
                    vac_tools::mcp::McpTrustClass::LocalTrusted => ("[Local]", StyleKey::Success),
                    vac_tools::mcp::McpTrustClass::RemoteVerified => {
                        ("[Verified]", StyleKey::Warning)
                    }
                    vac_tools::mcp::McpTrustClass::RemoteUntrusted => {
                        ("[Untrusted]", StyleKey::Error)
                    }
                };
                line_spans.push(Span::raw(" "));
                line_spans.push(Span::styled(trust_badge, state.theme.style(trust_key)));
            }

            let mut active_mode = state.switchers.active_isolation_mode.clone();
            if active_mode.starts_with("isolated") {
                active_mode = "isolated".to_string(); // Map isolated variants
            }
            if !conn_state.allowed_in_modes.is_empty()
                && !conn_state.allowed_in_modes.contains(&active_mode)
            {
                line_spans.push(Span::raw(" "));
                line_spans.push(Span::styled(
                    "⚠️ Mode Mismatch",
                    state
                        .theme
                        .style(StyleKey::Error)
                        .add_modifier(Modifier::BOLD),
                ));
            }

            lines.push(Line::from(line_spans));
            if let vac_tools::mcp::McpConnectionStatus::Unreachable(reason) = &conn_state.status {
                lines.push(Line::from(vec![
                    Span::raw("      "),
                    Span::styled(reason.clone(), state.theme.style(StyleKey::Muted)),
                ]));
            }
        }
    }

    f.render_widget(Paragraph::new(lines), area);
}

fn render_runtime_section(f: &mut Frame, state: &AppState, area: Rect, collapsed: bool) {
    let collapse_indicator = if collapsed { "▸" } else { "▾" };
    let header = Line::from(Span::styled(
        format!("  {} Runtime", collapse_indicator),
        Style::default().add_modifier(Modifier::BOLD),
    ));

    if collapsed {
        f.render_widget(Paragraph::new(vec![header]), area);
        return;
    }

    let mut lines = vec![header];

    let mut queued = 0;
    let mut running = 0;
    let mut completed = 0;
    let mut failed = 0;
    for job in &state.runtime.jobs {
        match &job.status {
            vac_runtime::JobStatus::Queued => queued += 1,
            vac_runtime::JobStatus::Running => running += 1,
            vac_runtime::JobStatus::Completed => completed += 1,
            vac_runtime::JobStatus::Failed(_) => failed += 1,
            _ => {}
        }
    }

    lines.push(Line::from(vec![
        Span::styled("    Queued: ", state.theme.style(StyleKey::Muted)),
        Span::raw(queued.to_string()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("    Running: ", state.theme.style(StyleKey::Muted)),
        Span::raw(running.to_string()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("    Completed: ", state.theme.style(StyleKey::Muted)),
        Span::raw(completed.to_string()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("    Failed: ", state.theme.style(StyleKey::Muted)),
        Span::raw(failed.to_string()),
    ]));

    f.render_widget(Paragraph::new(lines), area);
}

fn render_changeset_summary(f: &mut Frame, state: &AppState, area: Rect) {
    let count = state.changeset_store.active_entries().len();
    let line = Line::from(vec![
        Span::styled(
            format!("  ▸ Changeset ({}) ", count),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled("— Workbench", state.theme.style(StyleKey::Muted)),
    ]);
    f.render_widget(Paragraph::new(vec![line]), area);
}
