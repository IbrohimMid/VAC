use crate::tui::app::AppState;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

pub fn render_profile_switcher(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(50, 40, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let input = Paragraph::new(Line::from(vec![
        Span::styled("Filter ", Style::default().fg(Color::DarkGray)),
        Span::raw(&state.profile_search_input),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Profile Switcher"),
    );
    f.render_widget(input, chunks[0]);

    let profiles = state.profile_switcher_filtered();
    let items: Vec<ListItem> = profiles
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let style = if i == state.profile_switcher_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let prefix = if p == &state.active_profile {
                "* "
            } else {
                "  "
            };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Green)),
                Span::styled(p.clone(), style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Profiles"))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(list, chunks[1]);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
