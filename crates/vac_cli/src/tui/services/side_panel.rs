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

    let context_collapsed = state.side_panel_section_collapsed.contains(&SidePanelSection::Context);
    let runtime_collapsed = state.side_panel_section_collapsed.contains(&SidePanelSection::Runtime);
    let changeset_collapsed = state.side_panel_section_collapsed.contains(&SidePanelSection::Changeset);
    let vil_status_collapsed = state.side_panel_section_collapsed.contains(&SidePanelSection::VilStatus);
    let mcp_collapsed = state.side_panel_section_collapsed.contains(&SidePanelSection::Mcp);

    let collapsed_height = 1;
    let context_height = if context_collapsed { collapsed_height } else { 5 };
    let runtime_height = if runtime_collapsed { collapsed_height } else { 6 };
    let changeset_height = if changeset_collapsed { collapsed_height } else { 10 };
    let vil_issues_count = state.vil_status.validation_issues.len().min(3);
    let vil_status_lines = if state.vil_status.profile.is_some() { 4 } else { 3 };
    let vil_status_height = if vil_status_collapsed { collapsed_height } else { (vil_status_lines + vil_issues_count + if state.vil_status.validation_issues.len() > 3 { 1 } else { 0 }) as u16 };
    let mcp_lines = state.mcp_server_states.iter().map(|(_, s)| {
        if matches!(s, vac_tools::mcp::McpConnectionState::Unreachable(_)) { 2 } else { 1 }
    }).sum::<u16>();
    let mcp_height = if mcp_collapsed { collapsed_height } else { (mcp_lines + 2).max(3) };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(context_height),
            Constraint::Length(runtime_height),
            Constraint::Length(changeset_height),
            Constraint::Length(vil_status_height),
            Constraint::Length(mcp_height),
            Constraint::Min(0),
        ])
        .split(padded_area);

    state.side_panel_header_areas.clear();
    let sections = [
        (SidePanelSection::Context, chunks[0]),
        (SidePanelSection::Runtime, chunks[1]),
        (SidePanelSection::Changeset, chunks[2]),
        (SidePanelSection::VilStatus, chunks[3]),
        (SidePanelSection::Mcp, chunks[4]),
    ];
    for (sec, mut rect) in sections {
        rect.height = 1;
        state.side_panel_header_areas.insert(sec, rect);
    }

    render_context_section(f, state, chunks[0], context_collapsed);
    render_runtime_section(f, state, chunks[1], runtime_collapsed);
    render_changeset_section(f, state, chunks[2], changeset_collapsed);
    render_vil_status_section(f, state, chunks[3], vil_status_collapsed);
    render_mcp_section(f, state, chunks[4], mcp_collapsed);
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

    let model_name = state.current_model.as_ref().map(|m| m.name.clone()).unwrap_or_else(|| "-".to_string());
    lines.push(Line::from(vec![
        Span::styled("    Model: ", Style::default().fg(Color::DarkGray)),
        Span::raw(model_name),
    ]));

    let session = state.session_id.chars().take(8).collect::<String>();
    lines.push(Line::from(vec![
        Span::styled("    Session: ", Style::default().fg(Color::DarkGray)),
        Span::raw(session),
    ]));

    let auto = if state.auto_approve { "Enabled" } else { "Disabled" };
    let auto_color = if state.auto_approve { Color::Red } else { Color::Green };
    lines.push(Line::from(vec![
        Span::styled("    Auto-Approve: ", Style::default().fg(Color::DarkGray)),
        Span::styled(auto, Style::default().fg(auto_color)),
    ]));

    f.render_widget(Paragraph::new(lines), area);
}

fn render_mcp_section(f: &mut Frame, state: &AppState, area: Rect, collapsed: bool) {
    let collapse_indicator = if collapsed { "▸" } else { "▾" };
    let connected = state.mcp_server_states.values().filter(|s| s.is_connected()).count();
    let total = state.mcp_server_states.len();
    let header = Line::from(Span::styled(
        format!("  {} MCP Servers ({}/{})", collapse_indicator, connected, total),
        Style::default().add_modifier(Modifier::BOLD),
    ));

    if collapsed {
        f.render_widget(Paragraph::new(vec![header]), area);
        return;
    }

    let mut lines = vec![header];

    if total == 0 {
        lines.push(Line::styled("    No MCP servers", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)));
    } else {
        for (name, conn_state) in &state.mcp_server_states {
            let (status, color) = if conn_state.is_connected() {
                ("✅", Color::Green)
            } else {
                ("❌", Color::Red)
            };
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(status, Style::default().fg(color)),
                Span::raw(" "),
                Span::styled(name.clone(), Style::default().fg(Color::Yellow)),
            ]));
            if let vac_tools::mcp::McpConnectionState::Unreachable(reason) = conn_state {
                lines.push(Line::from(vec![
                    Span::raw("      "),
                    Span::styled(reason.clone(), Style::default().fg(Color::DarkGray)),
                ]));
            }
        }
    }

    f.render_widget(Paragraph::new(lines), area);
}

fn render_vil_status_section(f: &mut Frame, state: &AppState, area: Rect, collapsed: bool) {
    let collapse_indicator = if collapsed { "▸" } else { "▾" };
    let score = state.vil_status.validation_score;
    let score_label = if score >= 0.9 {
        "A"
    } else if score >= 0.7 {
        "B"
    } else {
        "C"
    };
    
    let header = Line::from(Span::styled(
        format!("  {} VIL Status [{}]", collapse_indicator, score_label),
        Style::default().add_modifier(Modifier::BOLD),
    ));

    if collapsed {
        f.render_widget(Paragraph::new(vec![header]), area);
        return;
    }

    let mut lines = vec![header];

    if let Some(profile) = &state.vil_status.profile {
        let archetype = format!("{}", profile.archetype);
        lines.push(Line::from(vec![
            Span::styled("    Archetype: ", Style::default().fg(Color::DarkGray)),
            Span::styled(archetype, Style::default().fg(Color::Cyan)),
        ]));
        
        let deps_count = profile.vil_deps.len();
        lines.push(Line::from(vec![
            Span::styled("    VIL Deps: ", Style::default().fg(Color::DarkGray)),
            Span::raw(deps_count.to_string()),
        ]));
    } else {
        lines.push(Line::styled("    Scanning...", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)));
    }

    let issues_count = state.vil_status.validation_issues.len();
    let issues_color = if issues_count > 0 { Color::Yellow } else { Color::Green };
    lines.push(Line::from(vec![
        Span::styled("    Issues: ", Style::default().fg(Color::DarkGray)),
        Span::styled(issues_count.to_string(), Style::default().fg(issues_color)),
    ]));

    if issues_count > 0 {
        let max_issues = 3;
        for issue in state.vil_status.validation_issues.iter().take(max_issues) {
            let mut text = issue.clone();
            if text.len() > 30 {
                text.truncate(27);
                text.push_str("...");
            }
            lines.push(Line::from(vec![
                Span::styled("      • ", Style::default().fg(Color::DarkGray)),
                Span::styled(text, Style::default().fg(Color::Yellow)),
            ]));
        }
        if issues_count > max_issues {
            lines.push(Line::from(vec![
                Span::styled(format!("      ... and {} more", issues_count - max_issues), Style::default().fg(Color::DarkGray)),
            ]));
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
    for job in &state.runtime_jobs {
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
        lines.push(Line::styled("    No changes", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)));
    } else {
        for (_, entry) in state.changeset_store.active_entries().iter().take(8).enumerate() {
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