//! Lane-aware rendering for Tri-Lane communication visualization.
//! Shows Trigger, Data, and Control lanes side-by-side.

use super::TuiApp;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
};

#[allow(dead_code)]
pub fn render_lanes(f: &mut Frame, area: Rect, app: &TuiApp) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ])
        .split(area);

    // Trigger Lane
    let trigger_items: Vec<ListItem> = app
        .trigger_lane_log
        .iter()
        .map(|msg| {
            ListItem::new(Line::from(Span::styled(
                msg.clone(),
                Style::default().fg(Color::Yellow),
            )))
        })
        .collect();
    let trigger_list = List::new(trigger_items).block(
        Block::default()
            .title(" ⚡ Trigger Lane ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow)),
    );
    f.render_widget(trigger_list, chunks[0]);

    // Data Lane
    let data_items: Vec<ListItem> = app
        .data_lane_log
        .iter()
        .map(|msg| {
            ListItem::new(Line::from(Span::styled(
                msg.clone(),
                Style::default().fg(Color::Cyan),
            )))
        })
        .collect();
    let data_list = List::new(data_items).block(
        Block::default()
            .title(" 📦 Data Lane ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(data_list, chunks[1]);

    // Control Lane
    let control_items: Vec<ListItem> = app
        .control_lane_log
        .iter()
        .map(|msg| {
            ListItem::new(Line::from(Span::styled(
                msg.clone(),
                Style::default().fg(Color::Green),
            )))
        })
        .collect();
    let control_list = List::new(control_items).block(
        Block::default()
            .title(" 🛡️ Control Lane ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green)),
    );
    f.render_widget(control_list, chunks[2]);
}
