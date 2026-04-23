//! Runtime tab — job queue, autopilot state, MCP servers, task graph.

use super::WorkbenchTabView;
use crate::app::AppState;
use crate::services::theme::StyleKey;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
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
                    state
                        .theme
                        .style(StyleKey::Warning)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let status = match &job.status {
                    vac_runtime::JobStatus::Queued => {
                        Span::styled("Q", state.theme.style(StyleKey::Muted))
                    }
                    vac_runtime::JobStatus::Running => {
                        Span::styled("R", state.theme.style(StyleKey::Accent))
                    }
                    vac_runtime::JobStatus::Completed => {
                        Span::styled("C", state.theme.style(StyleKey::Success))
                    }
                    vac_runtime::JobStatus::Failed(_) => {
                        Span::styled("F", state.theme.style(StyleKey::Error))
                    }
                    vac_runtime::JobStatus::Cancelled => {
                        Span::styled("X", state.theme.style(StyleKey::Warning))
                    }
                };
                ListItem::new(Line::from(vec![
                    status,
                    Span::raw(" "),
                    Span::styled(
                        job.id.to_string().chars().take(8).collect::<String>(),
                        state.theme.style(StyleKey::Muted),
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
            Span::styled(format!("Q {queued}"), state.theme.style(StyleKey::Muted)),
            Span::raw("  "),
            Span::styled(format!("R {running}"), state.theme.style(StyleKey::Accent)),
            Span::raw("  "),
            Span::styled(
                format!("C {completed}"),
                state.theme.style(StyleKey::Success),
            ),
            Span::raw("  "),
            Span::styled(format!("F {failed}"), state.theme.style(StyleKey::Error)),
            Span::raw("  "),
            Span::styled(
                format!("X {cancelled}"),
                state.theme.style(StyleKey::Warning),
            ),
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
                    Span::styled(err.clone(), state.theme.style(StyleKey::Error)),
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
                        Span::styled(tool_call_id.clone(), state.theme.style(StyleKey::Warning)),
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
                            state.theme.style(StyleKey::Warning),
                        ),
                    ]));
                }
                _ => {}
            }
            lines.push(Line::raw(""));
        }

        if !state.mcp_maps.server_states.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "MCP Servers:",
                Style::default().add_modifier(Modifier::BOLD),
            )]));
            for (name, conn_state) in &state.mcp_maps.server_states {
                let (status, status_key) = if conn_state.is_connected() {
                    ("✅ connected", StyleKey::Success)
                } else {
                    ("❌ unreachable", StyleKey::Error)
                };
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(name.clone(), state.theme.style(StyleKey::Warning)),
                    Span::raw(" "),
                    Span::styled(status, state.theme.style(status_key)),
                ]));
                if let vac_tools::mcp::McpConnectionStatus::Unreachable(reason) = &conn_state.status
                {
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(reason.clone(), state.theme.style(StyleKey::Muted)),
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
                Span::styled("  Nodes: ", state.theme.style(StyleKey::Muted)),
                Span::raw(projection.nodes.len().to_string()),
                Span::styled("  Roots: ", state.theme.style(StyleKey::Muted)),
                Span::raw(projection.root_ids.len().to_string()),
            ]));
            for node in projection.nodes.iter().take(5) {
                let status_style = match &node.status {
                    vac_core::engine::TaskNodeStatus::Pending => state.theme.style(StyleKey::Muted),
                    vac_core::engine::TaskNodeStatus::Running => {
                        state.theme.style(StyleKey::TaskRunning)
                    }
                    vac_core::engine::TaskNodeStatus::Completed => {
                        state.theme.style(StyleKey::TaskCompleted)
                    }
                    vac_core::engine::TaskNodeStatus::Failed(_) => {
                        state.theme.style(StyleKey::TaskFailed)
                    }
                    vac_core::engine::TaskNodeStatus::Blocked => {
                        state.theme.style(StyleKey::Warning)
                    }
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
                    Span::styled(format!("    [{}] ", status_label), status_style),
                    Span::raw(node.label.chars().take(24).collect::<String>()),
                    Span::styled(approval.to_string(), state.theme.style(StyleKey::Warning)),
                ]));
            }
            if projection.nodes.len() > 5 {
                lines.push(Line::styled(
                    format!("    … and {} more", projection.nodes.len() - 5),
                    state.theme.style(StyleKey::Muted),
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
                state.theme.style(StyleKey::Muted),
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
