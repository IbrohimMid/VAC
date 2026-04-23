//! Approvals tab — review and approve/reject pending tool calls.

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

pub struct ApprovalsTab;

impl WorkbenchTabView for ApprovalsTab {
    fn tab_label(state: &AppState) -> String {
        format!("Approvals ({})", state.execution.approvals.pending_approvals.len())
    }

    fn render(f: &mut Frame, state: &mut AppState, area: Rect) {
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(area);

        // PR-T16 P1 — record per-row click regions for the pending-approvals
        // list. Inner area is `body[0]` minus its 1-char border.
        state.layout.workbench_chrome.approvals_row_regions.clear();
        if body[0].width > 2 && body[0].height > 2 {
            let inner_x = body[0].x + 1;
            let inner_y = body[0].y + 1;
            let inner_w = body[0].width - 2;
            let inner_h = body[0].height - 2;
            for idx in 0..state.execution.approvals.pending_approvals.len() {
                if idx as u16 >= inner_h {
                    break;
                }
                state.layout.workbench_chrome.approvals_row_regions.push((
                    idx,
                    ratatui::layout::Rect::new(inner_x, inner_y + idx as u16, inner_w, 1),
                ));
            }
        }

        let items: Vec<ListItem> = state
            .execution.approvals.pending_approvals
            .iter()
            .enumerate()
            .map(|(idx, tc)| {
                let selected = idx == state.execution.approvals.approval_selected_idx;
                let style = if selected {
                    state
                        .core.theme
                        .style(StyleKey::Warning)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let id_short = tc.id.chars().take(8).collect::<String>();
                ListItem::new(Line::from(vec![
                    Span::styled(id_short, state.core.theme.style(StyleKey::Muted)),
                    Span::raw(" "),
                    Span::styled(tc.function.name.clone(), style),
                ]))
            })
            .collect();

        let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Pending"));
        f.render_widget(list, body[0]);

        let mut lines: Vec<Line> = Vec::new();
        if let Some(tc) = state.execution.approvals.pending_approvals.get(state.execution.approvals.approval_selected_idx) {
            lines.push(Line::from(vec![
                Span::styled("Tool: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    tc.function.name.clone(),
                    state.core.theme.style(StyleKey::Warning),
                ),
            ]));
            lines.push(Line::raw(""));

            if let Some(expl) = state
            .execution.approvals.approval_explanations
                .get(&tc.id)
                .and_then(|v| v.clone())
            {
                lines.push(Line::styled(
                    "Explanation",
                    Style::default().add_modifier(Modifier::BOLD),
                ));
                for l in expl.lines() {
                    lines.push(Line::styled(
                        l.to_string(),
                        state.core.theme.style(StyleKey::Muted),
                    ));
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
                        &state.core.theme,
                        file_path,
                        old_str,
                        new_str,
                        body[1].width as usize,
                    ));
                }
            } else if tc.function.name == "file_write" {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
                    let file_path = v.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                    let content = v.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    lines.extend(crate::services::file_diff::preview_file_diff(
                        &state.core.theme,
                        file_path,
                        "",
                        content,
                        body[1].width as usize,
                    ));
                }
            } else if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
                let formatted =
                    serde_json::to_string_pretty(&v).unwrap_or_else(|_| args.to_string());
                lines.push(Line::styled(
                    "Arguments",
                    Style::default().add_modifier(Modifier::BOLD),
                ));
                for line in formatted.lines() {
                    lines.push(Line::raw(line.to_string()));
                }
            } else {
                lines.push(Line::styled(
                    "Arguments",
                    Style::default().add_modifier(Modifier::BOLD),
                ));
                for line in args.lines() {
                    lines.push(Line::raw(line.to_string()));
                }
            }
        } else {
            lines.push(Line::styled(
                "No pending approvals. Tool requests will appear here when confirmation is needed.",
                state.core.theme.style(StyleKey::Muted),
            ));
        }

        let detail = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Detail"))
            .wrap(Wrap { trim: false })
            .scroll((state.execution.approvals.approval_detail_scroll as u16, 0));
        f.render_widget(detail, body[1]);
    }
}
