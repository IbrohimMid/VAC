//! Lane-aware rendering for Tri-Lane communication visualization.

use super::TuiApp;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
};

pub fn render_lanes(f: &mut Frame, area: Rect, app: &TuiApp) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ])
        .split(area);

    render_lane(f, rows[0], "Thinking / Plan", app.trigger_lane_log(), Color::LightYellow);
    render_lane(f, rows[1], "Reading / Search", app.data_lane_log(), Color::Cyan);
    render_lane(f, rows[2], "Commands / Writes", app.control_lane_log(), Color::LightGreen);
}

fn render_lane(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    items: &[String],
    color: Color,
) {
    let visible = area.height.saturating_sub(2) as usize;
    let list_items: Vec<ListItem> = items
        .iter()
        .rev()
        .take(visible.max(1))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|msg| {
            ListItem::new(Line::from(Span::styled(
                msg.clone(),
                Style::default().fg(color),
            )))
        })
        .collect();

    let list = List::new(list_items).block(
        Block::default()
            .title(Span::styled(
                title,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color)),
    );
    frame.render_widget(list, area);
}
