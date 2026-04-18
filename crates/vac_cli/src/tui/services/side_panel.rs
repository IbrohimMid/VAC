use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::tui::app::{AppState, SidePanelSection};

pub fn render_side_panel(f: &mut Frame, state: &mut AppState, area: Rect) {
    f.render_widget(ratatui::widgets::Clear, area);

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner_area = block.inner(area);
    f.render_widget(block, area);

    let padded_area = Rect {
        x: inner_area.x,
        y: inner_area.y.saturating_add(1),
        width: inner_area.width,
        height: inner_area.height.saturating_sub(2),
    };

    let context_collapsed = state
        .side_panel_section_collapsed
        .contains(&SidePanelSection::Context);
    let runtime_collapsed = state
        .side_panel_section_collapsed
        .contains(&SidePanelSection::Runtime);
    let changeset_collapsed = state
        .side_panel_section_collapsed
        .contains(&SidePanelSection::Changeset);
    let mcp_collapsed = state
        .side_panel_section_collapsed
        .contains(&SidePanelSection::Mcp);
    let sessions_collapsed = state
        .side_panel_section_collapsed
        .contains(&SidePanelSection::Sessions);
    let todos_collapsed = state
        .side_panel_section_collapsed
        .contains(&SidePanelSection::Todos);
    let usage_collapsed = state
        .side_panel_section_collapsed
        .contains(&SidePanelSection::Usage);

    let collapsed_height = 1;
    let context_extra = (!state.pinned_files.is_empty()) as u16
        + (!state.pinned_diffs.is_empty()) as u16
        + (!state.pinned_diagnostics.is_empty()) as u16;
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
    let changeset_height = if changeset_collapsed {
        collapsed_height
    } else {
        10
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
    // Todos: hide section entirely when empty; otherwise scale to 1 header + item lines (cap 8).
    let todos_visible = !state.todos.is_empty();
    let todos_height = if !todos_visible {
        0
    } else if todos_collapsed {
        collapsed_height
    } else {
        (state.todos.len().min(8) as u16) + 1
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

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(context_height),
            Constraint::Length(usage_height),
            Constraint::Length(sessions_height),
            Constraint::Length(runtime_height),
            Constraint::Length(todos_height),
            Constraint::Length(changeset_height),
            Constraint::Length(mcp_height),
            Constraint::Min(0),
        ])
        .split(padded_area);

    state.side_panel_header_areas.clear();
    state.side_panel_row_areas.clear();
    let mut sections: Vec<(SidePanelSection, Rect)> = vec![(SidePanelSection::Context, chunks[0])];
    if usage_visible {
        sections.push((SidePanelSection::Usage, chunks[1]));
    }
    sections.push((SidePanelSection::Sessions, chunks[2]));
    sections.push((SidePanelSection::Runtime, chunks[3]));
    if todos_visible {
        sections.push((SidePanelSection::Todos, chunks[4]));
    }
    sections.push((SidePanelSection::Changeset, chunks[5]));
    sections.push((SidePanelSection::Mcp, chunks[6]));
    for (sec, mut rect) in sections {
        rect.height = 1;
        state.side_panel_header_areas.insert(sec, rect);
    }

    render_context_section(f, state, chunks[0], context_collapsed);
    if usage_visible {
        render_usage_section(f, state, chunks[1], usage_collapsed);
    }
    render_sessions_section(f, state, chunks[2], sessions_collapsed);
    render_runtime_section(f, state, chunks[3], runtime_collapsed);
    if todos_visible {
        render_todos_section(f, state, chunks[4], todos_collapsed);
    }
    render_changeset_section(f, state, chunks[5], changeset_collapsed);
    render_mcp_section(f, state, chunks[6], mcp_collapsed);
}

fn render_todos_section(f: &mut Frame, state: &AppState, area: Rect, collapsed: bool) {
    use crate::tui::services::changeset::TodoStatus;
    let collapse_indicator = if collapsed { "▸" } else { "▾" };
    let pending = state
        .todos
        .iter()
        .filter(|t| t.status != TodoStatus::Done)
        .count();
    let done = state.todos.len().saturating_sub(pending);
    let header = Line::from(vec![
        Span::styled(
            format!("  {} Todos ", collapse_indicator),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({}/{})", done, state.todos.len()),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    if collapsed {
        f.render_widget(Paragraph::new(vec![header]), area);
        return;
    }

    let mut lines = vec![header];
    for t in state.todos.iter().take(8) {
        let (marker, color) = match t.status {
            TodoStatus::Done => ("[✓]", Color::Green),
            TodoStatus::InProgress => ("[/]", Color::Yellow),
            TodoStatus::Pending => ("[ ]", Color::DarkGray),
        };
        let style = if t.status == TodoStatus::Done {
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::CROSSED_OUT)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(marker, Style::default().fg(color)),
            Span::raw(" "),
            Span::styled(t.text.clone(), style),
        ]));
    }
    f.render_widget(Paragraph::new(lines), area);
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
    let pct_color = if pct >= 80.0 {
        Color::Red
    } else if pct >= 50.0 {
        Color::Yellow
    } else {
        Color::Green
    };
    let lines = vec![
        header,
        Line::from(vec![
            Span::styled("    Turn: ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!(
                "{} in / {} out",
                state.current_message_usage.input_tokens, state.current_message_usage.output_tokens
            )),
        ]),
        Line::from(vec![
            Span::styled("    Session: ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{} tokens", state.total_session_usage.total_tokens)),
        ]),
        Line::from(vec![
            Span::styled("    Context: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:.0}%", pct), Style::default().fg(pct_color)),
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
        .current_model
        .as_ref()
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "-".to_string());
    lines.push(Line::from(vec![
        Span::styled("    Model: ", Style::default().fg(Color::DarkGray)),
        Span::raw(model_name),
    ]));

    let session = state.session_id.chars().take(8).collect::<String>();
    lines.push(Line::from(vec![
        Span::styled("    Session: ", Style::default().fg(Color::DarkGray)),
        Span::raw(session),
    ]));

    let auto = if state.auto_approve {
        "Enabled"
    } else {
        "Disabled"
    };
    let auto_color = if state.auto_approve {
        Color::Red
    } else {
        Color::Green
    };
    lines.push(Line::from(vec![
        Span::styled("    Auto-Approve: ", Style::default().fg(Color::DarkGray)),
        Span::styled(auto, Style::default().fg(auto_color)),
    ]));

    if let Some(ident) = state
        .auth_display_info
        .0
        .as_ref()
        .or(state.auth_display_info.1.as_ref())
    {
        lines.push(Line::from(vec![
            Span::styled("    Auth: ", Style::default().fg(Color::DarkGray)),
            Span::raw(ident.clone()),
        ]));
    }

    if !state.pinned_files.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("    Files: ", Style::default().fg(Color::DarkGray)),
            Span::raw(compact_items(&state.pinned_files, 2)),
        ]));
    }
    if !state.pinned_diffs.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("    Diffs: ", Style::default().fg(Color::DarkGray)),
            Span::raw(compact_items(&state.pinned_diffs, 2)),
        ]));
    }
    if !state.pinned_diagnostics.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("    Diagnostics: ", Style::default().fg(Color::DarkGray)),
            Span::raw(compact_items(&state.pinned_diagnostics, 2)),
        ]));
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
            "    No sessions",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ));
    } else {
        for (i, session) in state.sessions.iter().take(5).enumerate() {
            let is_active = session.id == state.session_id;
            let color = if is_active {
                Color::Yellow
            } else {
                Color::DarkGray
            };
            let title = if session.title.is_empty() {
                "Untitled"
            } else {
                &session.title
            };
            let prefix = if is_active { "    * " } else { "      " };
            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(color)),
                Span::styled(
                    title.chars().take(20).collect::<String>(),
                    Style::default().fg(color),
                ),
            ]));
            // Track row rect for click handling (header is row 0, sessions start at row 1)
            let row_y = area.y + 1 + i as u16;
            if row_y < area.y + area.height {
                state.side_panel_row_areas.push((
                    crate::tui::app::SidePanelRowAction::SwitchSession(session.id.clone()),
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
            "    No MCP servers",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ));
    } else {
        let mut row_offset = 1u16; // header is row 0
        for (name, conn_state) in &state.mcp_server_states {
            // Track row for click
            let row_y = area.y + row_offset;
            if row_y < area.y + area.height {
                state.side_panel_row_areas.push((
                    crate::tui::app::SidePanelRowAction::ShowMcpDetail(name.clone()),
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
            let (status, color) = if conn_state.is_connected() {
                ("✅", Color::Green)
            } else {
                ("❌", Color::Red)
            };

            let mut line_spans = vec![
                Span::raw("    "),
                Span::styled(status, Style::default().fg(color)),
                Span::raw(" "),
                Span::styled(name.clone(), Style::default().fg(Color::Yellow)),
            ];

            if let Some(trust) = &conn_state.trust_class {
                let (trust_badge, trust_color) = match trust {
                    vac_tools::mcp::McpTrustClass::LocalTrusted => ("[Local]", Color::Green),
                    vac_tools::mcp::McpTrustClass::RemoteVerified => ("[Verified]", Color::Yellow),
                    vac_tools::mcp::McpTrustClass::RemoteUntrusted => ("[Untrusted]", Color::Red),
                };
                line_spans.push(Span::raw(" "));
                line_spans.push(Span::styled(trust_badge, Style::default().fg(trust_color)));
            }

            let mut active_mode = state.active_isolation_mode.clone();
            if active_mode.starts_with("isolated") {
                active_mode = "isolated".to_string(); // Map isolated variants
            }
            if !conn_state.allowed_in_modes.is_empty()
                && !conn_state.allowed_in_modes.contains(&active_mode)
            {
                line_spans.push(Span::raw(" "));
                line_spans.push(Span::styled(
                    "⚠️ Mode Mismatch",
                    Style::default()
                        .fg(Color::LightRed)
                        .add_modifier(Modifier::BOLD),
                ));
            }

            lines.push(Line::from(line_spans));
            if let vac_tools::mcp::McpConnectionStatus::Unreachable(reason) = &conn_state.status {
                lines.push(Line::from(vec![
                    Span::raw("      "),
                    Span::styled(reason.clone(), Style::default().fg(Color::DarkGray)),
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
        Span::styled("    Queued: ", Style::default().fg(Color::DarkGray)),
        Span::raw(queued.to_string()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("    Running: ", Style::default().fg(Color::DarkGray)),
        Span::raw(running.to_string()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("    Completed: ", Style::default().fg(Color::DarkGray)),
        Span::raw(completed.to_string()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("    Failed: ", Style::default().fg(Color::DarkGray)),
        Span::raw(failed.to_string()),
    ]));

    f.render_widget(Paragraph::new(lines), area);
}

fn render_changeset_section(f: &mut Frame, state: &AppState, area: Rect, collapsed: bool) {
    let collapse_indicator = if collapsed { "▸" } else { "▾" };
    let count = state.changeset_store.active_entries().len();
    let header = Line::from(Span::styled(
        format!("  {} Changeset ({})", collapse_indicator, count),
        Style::default().add_modifier(Modifier::BOLD),
    ));

    if collapsed {
        f.render_widget(Paragraph::new(vec![header]), area);
        return;
    }

    let mut lines = vec![header];

    if count == 0 {
        lines.push(Line::styled(
            "    No changes",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ));
    } else {
        for entry in state.changeset_store.active_entries().iter().take(8) {
            let indicator = match entry.state {
                crate::tui::services::FileState::Created => "[+]",
                crate::tui::services::FileState::Modified => "[~]",
                crate::tui::services::FileState::Removed => "[-]",
                crate::tui::services::FileState::Reverted => "[✓]",
                crate::tui::services::FileState::FailedRestore => "[✗]",
            };
            let color = match entry.state {
                crate::tui::services::FileState::Created => Color::Green,
                crate::tui::services::FileState::Modified => Color::Yellow,
                crate::tui::services::FileState::Removed => Color::Red,
                crate::tui::services::FileState::Reverted => Color::Cyan,
                crate::tui::services::FileState::FailedRestore => Color::Red,
            };
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(indicator, Style::default().fg(color)),
                Span::raw(" "),
                Span::raw(entry.path.clone()),
            ]));
        }
    }

    f.render_widget(Paragraph::new(lines), area);
}
