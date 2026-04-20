//! Runtime tab — job queue, autopilot state, MCP servers, task graph.

use super::WorkbenchTabView;
use crate::app::AppState;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

pub struct RuntimeTab;

impl WorkbenchTabView for RuntimeTab {
    fn tab_label(state: &AppState) -> String {
        format!("Runtime ({})", state.runtime.jobs.len())
    }

    fn render(f: &mut Frame, state: &mut AppState, area: Rect) {
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(area);

        let mut queued = 0usize;
        let mut running = 0usize;
        let mut completed = 0usize;
        let mut failed = 0usize;
        let mut cancelled = 0usize;
        for job in &state.runtime.jobs {
            match &job.status {
                vac_runtime::JobStatus::Queued => queued += 1,
                vac_runtime::JobStatus::Running => running += 1,
                vac_runtime::JobStatus::Completed => completed += 1,
                vac_runtime::JobStatus::Failed(_) => failed += 1,
                vac_runtime::JobStatus::Cancelled => cancelled += 1,
            }
        }

        let items: Vec<ListItem> = state
            .runtime
            .jobs
            .iter()
            .enumerate()
            .map(|(idx, job)| {
                let selected = idx == state.runtime.selected_idx;
                let style = if selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let status = match &job.status {
                    vac_runtime::JobStatus::Queued => {
                        Span::styled("Q", Style::default().fg(Color::DarkGray))
                    }
                    vac_runtime::JobStatus::Running => {
                        Span::styled("R", Style::default().fg(Color::Cyan))
                    }
                    vac_runtime::JobStatus::Completed => {
                        Span::styled("C", Style::default().fg(Color::Green))
                    }
                    vac_runtime::JobStatus::Failed(_) => {
                        Span::styled("F", Style::default().fg(Color::Red))
                    }
                    vac_runtime::JobStatus::Cancelled => {
                        Span::styled("X", Style::default().fg(Color::Yellow))
                    }
                };
                ListItem::new(Line::from(vec![
                    status,
                    Span::raw(" "),
                    Span::styled(
                        job.id.to_string().chars().take(8).collect::<String>(),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(" "),
                    Span::styled(job.kind_name(), style),
                ]))
            })
            .collect();

        let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Jobs"));
        f.render_widget(list, body[0]);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(vec![
            Span::styled("Jobs: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(format!("Q {queued}"), Style::default().fg(Color::DarkGray)),
            Span::raw("  "),
            Span::styled(format!("R {running}"), Style::default().fg(Color::Cyan)),
            Span::raw("  "),
            Span::styled(format!("C {completed}"), Style::default().fg(Color::Green)),
            Span::raw("  "),
            Span::styled(format!("F {failed}"), Style::default().fg(Color::Red)),
            Span::raw("  "),
            Span::styled(format!("X {cancelled}"), Style::default().fg(Color::Yellow)),
        ]));
        lines.push(Line::raw(""));

        if let Some(snapshot) = &state.runtime.snapshot {
            lines.push(Line::from(vec![
                Span::styled("Autopilot: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(snapshot.mode.clone()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Intent: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(snapshot.task_intent_mode.clone()),
            ]));
            lines.push(Line::from(vec![
                Span::styled(
                    "Environment: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(snapshot.environment_mode.clone()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Execution: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("{:?}", snapshot.execution_environment)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Queue: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(snapshot.queue_len.to_string()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("State: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("{:?}", snapshot.state)),
            ]));
            if let Some(err) = &snapshot.last_error {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Last error: ",
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(err.clone(), Style::default().fg(Color::Red)),
                ]));
            }
            if let Some(job_id) = snapshot.current_job {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Current job: ",
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(job_id.to_string()),
                ]));
            }
            lines.push(Line::from(vec![
                Span::styled("Updated: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(
                    snapshot
                        .updated_at
                        .format("%Y-%m-%d %H:%M:%S UTC")
                        .to_string(),
                ),
            ]));
            match &snapshot.state {
                vac_runtime::AutopilotState::WaitingApproval { tool_call_id } => {
                    lines.push(Line::from(vec![
                        Span::styled("Approval: ", Style::default().add_modifier(Modifier::BOLD)),
                        Span::styled(tool_call_id.clone(), Style::default().fg(Color::Yellow)),
                    ]));
                }
                vac_runtime::AutopilotState::Backoff { until } => {
                    lines.push(Line::from(vec![
                        Span::styled(
                            "Backoff until: ",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            until.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                            Style::default().fg(Color::Yellow),
                        ),
                    ]));
                }
                _ => {}
            }
            lines.push(Line::raw(""));
        }

        if !state.mcp_server_states.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "MCP Servers:",
                Style::default().add_modifier(Modifier::BOLD),
            )]));
            for (name, conn_state) in &state.mcp_server_states {
                let (status, color) = if conn_state.is_connected() {
                    ("✅ connected", Color::Green)
                } else {
                    ("❌ unreachable", Color::Red)
                };
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(name.clone(), Style::default().fg(Color::Yellow)),
                    Span::raw(" "),
                    Span::styled(status, Style::default().fg(color)),
                ]));
                if let vac_tools::mcp::McpConnectionStatus::Unreachable(reason) = &conn_state.status
                {
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(reason.clone(), Style::default().fg(Color::DarkGray)),
                    ]));
                }
            }
            lines.push(Line::raw(""));
        }

        if let Some(projection) = &state.runtime.task_projection {
            lines.push(Line::from(vec![Span::styled(
                "Task Graph:",
                Style::default().add_modifier(Modifier::BOLD),
            )]));
            lines.push(Line::from(vec![
                Span::styled("  Nodes: ", Style::default().fg(Color::DarkGray)),
                Span::raw(projection.nodes.len().to_string()),
                Span::styled("  Roots: ", Style::default().fg(Color::DarkGray)),
                Span::raw(projection.root_ids.len().to_string()),
            ]));
            for node in projection.nodes.iter().take(5) {
                let status_color = match &node.status {
                    vac_core::engine::TaskNodeStatus::Pending => Color::DarkGray,
                    vac_core::engine::TaskNodeStatus::Running => Color::Cyan,
                    vac_core::engine::TaskNodeStatus::Completed => Color::Green,
                    vac_core::engine::TaskNodeStatus::Failed(_) => Color::Red,
                    vac_core::engine::TaskNodeStatus::Blocked => Color::Yellow,
                };
                let status_label = match &node.status {
                    vac_core::engine::TaskNodeStatus::Pending => "P",
                    vac_core::engine::TaskNodeStatus::Running => "R",
                    vac_core::engine::TaskNodeStatus::Completed => "C",
                    vac_core::engine::TaskNodeStatus::Failed(_) => "F",
                    vac_core::engine::TaskNodeStatus::Blocked => "B",
                };
                let approval = if node.approval_required { "⚠" } else { "" };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("    [{}] ", status_label),
                        Style::default().fg(status_color),
                    ),
                    Span::raw(node.label.chars().take(24).collect::<String>()),
                    Span::styled(approval.to_string(), Style::default().fg(Color::Yellow)),
                ]));
            }
            if projection.nodes.len() > 5 {
                lines.push(Line::styled(
                    format!("    … and {} more", projection.nodes.len() - 5),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            lines.push(Line::raw(""));
        }

        if let Some(job) = state.runtime.jobs.get(state.runtime.selected_idx) {
            lines.push(Line::from(vec![
                Span::styled("Job: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(job.id.to_string()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Kind: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(job.kind_name()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(match &job.status {
                    vac_runtime::JobStatus::Queued => "Queued".to_string(),
                    vac_runtime::JobStatus::Running => "Running".to_string(),
                    vac_runtime::JobStatus::Completed => "Completed".to_string(),
                    vac_runtime::JobStatus::Failed(err) => format!("Failed: {err}"),
                    vac_runtime::JobStatus::Cancelled => "Cancelled".to_string(),
                }),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Retries: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("{}/{}", job.retry_count, job.max_retries)),
            ]));
            if let Some(summary) = &job.result_summary {
                lines.push(Line::raw(""));
                lines.push(Line::styled(
                    "Summary",
                    Style::default().add_modifier(Modifier::BOLD),
                ));
                for line in summary.lines() {
                    lines.push(Line::raw(line.to_string()));
                }
            }
        } else {
            lines.push(Line::styled(
                "No runtime jobs loaded yet. Press r to refresh the queue.",
                Style::default().fg(Color::DarkGray),
            ));
        }

        let detail = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Runtime Detail"),
            )
            .wrap(Wrap { trim: false })
            .scroll((state.runtime.detail_scroll as u16, 0));
        f.render_widget(detail, body[1]);
    }
}
