//! Approvals tab — review and approve/reject pending tool calls.

use crate::app::AppState;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};
use super::WorkbenchTabView;

pub struct ApprovalsTab;

impl WorkbenchTabView for ApprovalsTab {
    fn tab_label(state: &AppState) -> String {
        format!("Approvals ({})", state.pending_approvals.len())
    }

    fn render(f: &mut Frame, state: &mut AppState, area: Rect) {
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(area);

        let items: Vec<ListItem> = state
            .pending_approvals
            .iter()
            .enumerate()
            .map(|(idx, tc)| {
                let selected = idx == state.approval_selected_idx;
                let style = if selected {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let id_short = tc.id.chars().take(8).collect::<String>();
                ListItem::new(Line::from(vec![
                    Span::styled(id_short, Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(tc.function.name.clone(), style),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Pending"));
        f.render_widget(list, body[0]);

        let mut lines: Vec<Line> = Vec::new();
        if let Some(tc) = state.pending_approvals.get(state.approval_selected_idx) {
            lines.push(Line::from(vec![
                Span::styled("Tool: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(tc.function.name.clone(), Style::default().fg(Color::Yellow)),
            ]));
            lines.push(Line::raw(""));

            if let Some(expl) = state
                .approval_explanations
                .get(&tc.id)
                .and_then(|v| v.clone())
            {
                lines.push(Line::styled("Explanation", Style::default().add_modifier(Modifier::BOLD)));
                for l in expl.lines() {
                    lines.push(Line::styled(l.to_string(), Style::default().fg(Color::DarkGray)));
                }
                lines.push(Line::raw(""));
            }

            let args = &tc.function.arguments;
            if tc.function.name == "file_edit" {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
                    let file_path = v.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                    let old_str = v.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
                    let new_str = v.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
                    lines.extend(crate::services::file_diff::preview_file_diff(
                        file_path, old_str, new_str, body[1].width as usize,
                    ));
                }
            } else if tc.function.name == "file_write" {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
                    let file_path = v.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                    let content = v.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    lines.extend(crate::services::file_diff::preview_file_diff(
                        file_path, "", content, body[1].width as usize,
                    ));
                }
            } else if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
                let formatted = serde_json::to_string_pretty(&v).unwrap_or_else(|_| args.to_string());
                lines.push(Line::styled("Arguments", Style::default().add_modifier(Modifier::BOLD)));
                for line in formatted.lines() {
                    lines.push(Line::raw(line.to_string()));
                }
            } else {
                lines.push(Line::styled("Arguments", Style::default().add_modifier(Modifier::BOLD)));
                for line in args.lines() {
                    lines.push(Line::raw(line.to_string()));
                }
            }
        } else {
            lines.push(Line::styled(
                "No pending approvals. Tool requests will appear here when confirmation is needed.",
                Style::default().fg(Color::DarkGray),
            ));
        }

        let detail = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Detail"))
            .wrap(Wrap { trim: false })
            .scroll((state.approval_detail_scroll as u16, 0));
        f.render_widget(detail, body[1]);
    }
}
