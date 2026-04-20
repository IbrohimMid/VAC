//! Sessions tab — browse and restore saved sessions.

use super::WorkbenchTabView;
use crate::app::AppState;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

pub struct SessionsTab;

impl WorkbenchTabView for SessionsTab {
    fn tab_label(state: &AppState) -> String {
        format!("Sessions ({})", state.sessions.len())
    }

    fn render(f: &mut Frame, state: &mut AppState, area: Rect) {
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(area);

        let items: Vec<ListItem> = state
            .sessions
            .iter()
            .enumerate()
            .map(|(idx, s)| {
                let sel = idx == state.sessions_selected_idx;
                let style = if sel {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let checkpoint_icon = if s.has_checkpoint { "●" } else { "○" };
                let snapshot_icon = if s.snapshot_stale {
                    "!"
                } else if s.snapshot_present {
                    "◆"
                } else {
                    "○"
                };
                let snapshot_color = if s.snapshot_stale {
                    Color::Yellow
                } else if s.snapshot_present {
                    Color::Cyan
                } else {
                    Color::DarkGray
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        checkpoint_icon,
                        Style::default().fg(if s.has_checkpoint {
                            Color::Green
                        } else {
                            Color::DarkGray
                        }),
                    ),
                    Span::raw(" "),
                    Span::styled(snapshot_icon, Style::default().fg(snapshot_color)),
                    Span::raw(" "),
                    Span::styled(&s.last_activity, Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(&s.title, style),
                    Span::styled(
                        format!(" ({}t)", s.task_count),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]))
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Sessions ({})", state.sessions.len())),
        );
        f.render_widget(list, body[0]);

        let mut lines: Vec<Line> = Vec::new();
        if let Some(sel) = state.sessions.get(state.sessions_selected_idx) {
            lines.push(Line::from(vec![
                Span::styled("Title: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(sel.title.clone()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("ID: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(sel.id.chars().take(16).collect::<String>()),
            ]));
            lines.push(Line::from(vec![
                Span::styled(
                    "Last active: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(sel.last_activity.clone()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Tasks: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(sel.task_count.to_string()),
            ]));
            lines.push(Line::from(vec![
                Span::styled(
                    "Checkpoint: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                if sel.has_checkpoint {
                    Span::styled("available ●", Style::default().fg(Color::Green))
                } else {
                    Span::styled(
                        "no checkpoint available ○",
                        Style::default().fg(Color::DarkGray),
                    )
                },
            ]));
            lines.push(Line::from(vec![
                Span::styled("Snapshot: ", Style::default().add_modifier(Modifier::BOLD)),
                if sel.snapshot_stale {
                    Span::styled("stale !", Style::default().fg(Color::Yellow))
                } else if sel.snapshot_present {
                    Span::styled("available ◆", Style::default().fg(Color::Cyan))
                } else {
                    Span::styled("not saved ○", Style::default().fg(Color::DarkGray))
                },
            ]));
            if !sel.checkpoints.is_empty() {
                lines.push(Line::raw(""));
                lines.push(Line::styled(
                    "Checkpoints:",
                    Style::default().add_modifier(Modifier::BOLD),
                ));
                for cp in sel.checkpoints.iter().take(4) {
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::raw(cp.clone()),
                    ]));
                }
            }
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "Enter: restore  r: resume checkpoint  d: cleanup artifacts",
                Style::default().fg(Color::DarkGray),
            ));
        } else {
            lines.push(Line::styled(
                "No sessions loaded yet. Run /sessions to open saved sessions.",
                Style::default().fg(Color::DarkGray),
            ));
        }

        let detail = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Detail"))
            .wrap(Wrap { trim: true });
        f.render_widget(detail, body[1]);
    }
}
