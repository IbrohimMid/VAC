//! Review tab — inspect and revert file changes.

use crate::app::AppState;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};
use super::WorkbenchTabView;

pub struct ReviewTab;

impl WorkbenchTabView for ReviewTab {
    fn tab_label(state: &AppState) -> String {
        format!("Review ({})", state.changeset_store.active_entries().len())
    }

    fn render(f: &mut Frame, state: &mut AppState, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(area);

        let title = format!("Review ({})", state.review_filtered_paths().len());
        let filter_line = if state.review.filter.is_empty() {
            Line::from(vec![
                Span::styled("Filter: ", Style::default().fg(Color::Cyan)),
                Span::styled("type to filter…", Style::default().fg(Color::DarkGray)),
            ])
        } else {
            Line::from(vec![
                Span::styled("Filter: ", Style::default().fg(Color::Cyan)),
                Span::styled(&state.review.filter, Style::default().add_modifier(Modifier::BOLD)),
            ])
        };
        let header = Paragraph::new(filter_line)
            .block(Block::default().borders(Borders::ALL).title(title));
        f.render_widget(header, chunks[0]);

        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(chunks[1]);

        let files = state.review_filtered_paths();
        let items: Vec<ListItem> = files
            .iter()
            .enumerate()
            .map(|(idx, path)| {
                let is_selected = idx == state.review.selected_idx;
                let style = if is_selected {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let (status_span, snap_span) = match state.review.items.get(path) {
                    Some(it) => {
                        let status = match it.status {
                            crate::app::ReviewItemStatus::Pending => Span::styled("• ", Style::default().fg(Color::DarkGray)),
                            crate::app::ReviewItemStatus::Restored => Span::styled("✓ ", Style::default().fg(Color::Green)),
                            crate::app::ReviewItemStatus::Failed => Span::styled("! ", Style::default().fg(Color::Red)),
                        };
                        let snap = if it.has_snapshot {
                            Span::styled("S ", Style::default().fg(Color::Cyan))
                        } else {
                            Span::styled("- ", Style::default().fg(Color::DarkGray))
                        };
                        (status, snap)
                    }
                    None => (
                        Span::styled("• ", Style::default().fg(Color::DarkGray)),
                        Span::styled("? ", Style::default().fg(Color::DarkGray)),
                    ),
                };

                ListItem::new(Line::from(vec![status_span, snap_span, Span::styled(path.clone(), style)]))
            })
            .collect();

        let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Files"));
        f.render_widget(list, body[0]);

        let diff_height = body[1].height.saturating_sub(2) as usize;
        let diff_title = if let Some(path) = &state.review.selected_path {
            format!("Diff: {path}")
        } else {
            "Diff".to_string()
        };

        let diff_lines: Vec<Line> = if let Some(diff) = &state.review.diff {
            if let (Some(old), Some(new)) = (diff.old_content.as_deref(), diff.new_content.as_deref()) {
                crate::services::review::render_diff_viewport(old, new, body[1].width as usize, diff.scroll, diff_height)
            } else if let Some(err) = &diff.last_error {
                vec![Line::from(Span::styled(err.clone(), Style::default().fg(Color::Red)))]
            } else {
                vec![Line::raw("No diff loaded.")]
            }
        } else if let Some(path) = state.review.selected_path.clone()
            && let Some(it) = state.review.items.get(&path)
            && let Some(err) = &it.last_error
        {
            vec![Line::from(Span::styled(err.clone(), Style::default().fg(Color::Red)))]
        } else {
            vec![Line::raw("Enter: toggle diff • PgUp/PgDn: scroll")]
        };

        let diff = Paragraph::new(diff_lines)
            .block(Block::default().borders(Borders::ALL).title(diff_title))
            .wrap(Wrap { trim: false });
        f.render_widget(diff, body[1]);
    }
}
