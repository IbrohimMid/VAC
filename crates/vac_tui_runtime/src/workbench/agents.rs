//! Agents tab — monitor swarm agent tasks and worker pool.

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

pub struct AgentsTab;

impl WorkbenchTabView for AgentsTab {
    fn tab_label(state: &AppState) -> String {
        format!("Agents ({})", state.execution.runtime.agent_tasks.len())
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
        for task in &state.execution.runtime.agent_tasks {
            match &task.status {
                vac_runtime::AgentTaskStatus::Queued => queued += 1,
                vac_runtime::AgentTaskStatus::Running => running += 1,
                vac_runtime::AgentTaskStatus::Completed => completed += 1,
                vac_runtime::AgentTaskStatus::Failed(_) => failed += 1,
                vac_runtime::AgentTaskStatus::Cancelled => cancelled += 1,
            }
        }

        let items: Vec<ListItem> = state
            .execution
            .runtime
            .agent_tasks
            .iter()
            .enumerate()
            .map(|(idx, task)| {
                let selected = idx == state.execution.runtime.agent_selected;
                let style = if selected {
                    state
                        .core
                        .theme
                        .style(StyleKey::Warning)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let status = match &task.status {
                    vac_runtime::AgentTaskStatus::Queued => {
                        Span::styled("Q", state.core.theme.style(StyleKey::Muted))
                    }
                    vac_runtime::AgentTaskStatus::Running => {
                        Span::styled("R", state.core.theme.style(StyleKey::Accent))
                    }
                    vac_runtime::AgentTaskStatus::Completed => {
                        Span::styled("C", state.core.theme.style(StyleKey::Success))
                    }
                    vac_runtime::AgentTaskStatus::Failed(_) => {
                        Span::styled("F", state.core.theme.style(StyleKey::Error))
                    }
                    vac_runtime::AgentTaskStatus::Cancelled => {
                        Span::styled("X", state.core.theme.style(StyleKey::Warning))
                    }
                };
                let role = Span::styled(
                    task.role.label(),
                    state
                        .core
                        .theme
                        .style(StyleKey::AppTitle)
                        .add_modifier(Modifier::BOLD),
                );
                let short_id = task.id.to_string().chars().take(8).collect::<String>();
                let mut desc = task.description.clone();
                if desc.chars().count() > 48 {
                    desc = desc.chars().take(45).collect::<String>() + "...";
                }
                ListItem::new(Line::from(vec![
                    status,
                    Span::raw(" "),
                    role,
                    Span::raw(" "),
                    Span::styled(short_id, state.core.theme.style(StyleKey::Muted)),
                    Span::raw(" "),
                    Span::styled(desc, style),
                ]))
            })
            .collect();

        let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Queue"));
        f.render_widget(list, body[0]);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(vec![
            Span::styled("Tasks: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("Q {queued}"),
                state.core.theme.style(StyleKey::Muted),
            ),
            Span::raw("  "),
            Span::styled(
                format!("R {running}"),
                state.core.theme.style(StyleKey::Accent),
            ),
            Span::raw("  "),
            Span::styled(
                format!("C {completed}"),
                state.core.theme.style(StyleKey::Success),
            ),
            Span::raw("  "),
            Span::styled(
                format!("F {failed}"),
                state.core.theme.style(StyleKey::Error),
            ),
            Span::raw("  "),
            Span::styled(
                format!("X {cancelled}"),
                state.core.theme.style(StyleKey::Warning),
            ),
        ]));
        lines.push(Line::raw(""));

        if let Some(snapshot) = &state.execution.runtime.agent_snapshot {
            lines.push(Line::styled(
                "Workers:",
                Style::default().add_modifier(Modifier::BOLD),
            ));
            for w in &snapshot.workers {
                let role = Span::styled(
                    w.role.label(),
                    state
                        .core
                        .theme
                        .style(StyleKey::AppTitle)
                        .add_modifier(Modifier::BOLD),
                );
                let status = match &w.status {
                    vac_runtime::AgentWorkerStatus::Idle => {
                        Span::styled("idle", state.core.theme.style(StyleKey::Muted))
                    }
                    vac_runtime::AgentWorkerStatus::Running { task_id, .. } => Span::styled(
                        format!(
                            "running {}",
                            task_id.to_string().chars().take(8).collect::<String>()
                        ),
                        state.core.theme.style(StyleKey::Accent),
                    ),
                };
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    role,
                    Span::raw(" "),
                    Span::styled(w.worker_id.clone(), state.core.theme.style(StyleKey::Muted)),
                    Span::raw(" "),
                    status,
                ]));
                if let Some(out) = &w.last_output {
                    let mut o = out.clone();
                    if o.chars().count() > 72 {
                        o = o.chars().take(69).collect::<String>() + "...";
                    }
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(o, state.core.theme.style(StyleKey::Muted)),
                    ]));
                }
            }
            lines.push(Line::raw(""));
            lines.push(Line::from(vec![
                Span::styled("Updated: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(
                    snapshot
                        .updated_at
                        .format("%Y-%m-%d %H:%M:%S UTC")
                        .to_string(),
                ),
            ]));
            lines.push(Line::raw(""));
        } else {
            lines.push(Line::styled(
                "No agent scheduler state found.",
                state.core.theme.style(StyleKey::Muted),
            ));
            lines.push(Line::raw(""));
        }

        if let Some(task) = state
            .execution
            .runtime
            .agent_tasks
            .get(state.execution.runtime.agent_selected)
        {
            lines.push(Line::styled(
                "Selected:",
                Style::default().add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::from(vec![
                Span::styled("ID: ", state.core.theme.style(StyleKey::Muted)),
                Span::raw(task.id.to_string()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Role: ", state.core.theme.style(StyleKey::Muted)),
                Span::raw(task.role.label()),
            ]));
            let status = match &task.status {
                vac_runtime::AgentTaskStatus::Queued => "Queued".to_string(),
                vac_runtime::AgentTaskStatus::Running => "Running".to_string(),
                vac_runtime::AgentTaskStatus::Completed => "Completed".to_string(),
                vac_runtime::AgentTaskStatus::Failed(e) => format!("Failed: {e}"),
                vac_runtime::AgentTaskStatus::Cancelled => "Cancelled".to_string(),
            };
            lines.push(Line::from(vec![
                Span::styled("Status: ", state.core.theme.style(StyleKey::Muted)),
                Span::raw(status),
            ]));
            if let Some(out) = &task.output_summary {
                lines.push(Line::from(vec![
                    Span::styled("Output: ", state.core.theme.style(StyleKey::Muted)),
                    Span::raw(out.clone()),
                ]));
            }
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "j/k: navigate  r: refresh",
                state.core.theme.style(StyleKey::Muted),
            ));
        } else {
            lines.push(Line::styled(
                "No tasks enqueued.",
                state.core.theme.style(StyleKey::Muted),
            ));
        }

        let detail = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Agents"))
            .wrap(Wrap { trim: true })
            .scroll((state.execution.runtime.agent_detail_scroll as u16, 0));
        f.render_widget(detail, body[1]);
    }
}
