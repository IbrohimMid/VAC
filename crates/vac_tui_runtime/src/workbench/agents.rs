//! Agents tab — monitor swarm agent tasks and worker pool.

use crate::app::AppState;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};
use super::WorkbenchTabView;

pub struct AgentsTab;

impl WorkbenchTabView for AgentsTab {
    fn tab_label(state: &AppState) -> String {
        format!("Agents ({})", state.runtime.agent_tasks.len())
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
        for task in &state.runtime.agent_tasks {
            match &task.status {
                vac_runtime::AgentTaskStatus::Queued => queued += 1,
                vac_runtime::AgentTaskStatus::Running => running += 1,
                vac_runtime::AgentTaskStatus::Completed => completed += 1,
                vac_runtime::AgentTaskStatus::Failed(_) => failed += 1,
                vac_runtime::AgentTaskStatus::Cancelled => cancelled += 1,
            }
        }

        let items: Vec<ListItem> = state
            .runtime
            .agent_tasks
            .iter()
            .enumerate()
            .map(|(idx, task)| {
                let selected = idx == state.runtime.agent_selected;
                let style = if selected {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let status = match &task.status {
                    vac_runtime::AgentTaskStatus::Queued => Span::styled("Q", Style::default().fg(Color::DarkGray)),
                    vac_runtime::AgentTaskStatus::Running => Span::styled("R", Style::default().fg(Color::Cyan)),
                    vac_runtime::AgentTaskStatus::Completed => Span::styled("C", Style::default().fg(Color::Green)),
                    vac_runtime::AgentTaskStatus::Failed(_) => Span::styled("F", Style::default().fg(Color::Red)),
                    vac_runtime::AgentTaskStatus::Cancelled => Span::styled("X", Style::default().fg(Color::Yellow)),
                };
                let role = Span::styled(task.role.label(), Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD));
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
                    Span::styled(short_id, Style::default().fg(Color::DarkGray)),
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

        if let Some(snapshot) = &state.runtime.agent_snapshot {
            lines.push(Line::styled("Workers:", Style::default().add_modifier(Modifier::BOLD)));
            for w in &snapshot.workers {
                let role = Span::styled(w.role.label(), Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD));
                let status = match &w.status {
                    vac_runtime::AgentWorkerStatus::Idle => Span::styled("idle", Style::default().fg(Color::DarkGray)),
                    vac_runtime::AgentWorkerStatus::Running { task_id, .. } => Span::styled(
                        format!("running {}", task_id.to_string().chars().take(8).collect::<String>()),
                        Style::default().fg(Color::Cyan),
                    ),
                };
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    role,
                    Span::raw(" "),
                    Span::styled(w.worker_id.clone(), Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    status,
                ]));
                if let Some(out) = &w.last_output {
                    let mut o = out.clone();
                    if o.chars().count() > 72 {
                        o = o.chars().take(69).collect::<String>() + "...";
                    }
                    lines.push(Line::from(vec![Span::raw("    "), Span::styled(o, Style::default().fg(Color::DarkGray))]));
                }
            }
            lines.push(Line::raw(""));
            lines.push(Line::from(vec![
                Span::styled("Updated: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(snapshot.updated_at.format("%Y-%m-%d %H:%M:%S UTC").to_string()),
            ]));
            lines.push(Line::raw(""));
        } else {
            lines.push(Line::styled("No agent scheduler state found.", Style::default().fg(Color::DarkGray)));
            lines.push(Line::raw(""));
        }

        if let Some(task) = state.runtime.agent_tasks.get(state.runtime.agent_selected) {
            lines.push(Line::styled("Selected:", Style::default().add_modifier(Modifier::BOLD)));
            lines.push(Line::from(vec![
                Span::styled("ID: ", Style::default().fg(Color::DarkGray)),
                Span::raw(task.id.to_string()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Role: ", Style::default().fg(Color::DarkGray)),
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
                Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
                Span::raw(status),
            ]));
            if let Some(out) = &task.output_summary {
                lines.push(Line::from(vec![
                    Span::styled("Output: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(out.clone()),
                ]));
            }
            lines.push(Line::raw(""));
            lines.push(Line::styled("j/k: navigate  r: refresh", Style::default().fg(Color::DarkGray)));
        } else {
            lines.push(Line::styled("No tasks enqueued.", Style::default().fg(Color::DarkGray)));
        }

        let detail = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Agents"))
            .wrap(Wrap { trim: true })
            .scroll((state.runtime.agent_detail_scroll as u16, 0));
        f.render_widget(detail, body[1]);
    }
}
