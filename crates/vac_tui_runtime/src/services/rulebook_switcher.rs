use crate::app::AppState;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

pub fn render_rulebook_switcher(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(70, 60, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let input = Paragraph::new(Line::from(vec![
        Span::styled("Filter ", Style::default().fg(Color::DarkGray)),
        Span::raw(&state.rulebook_search_input),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Rulebook Switcher"),
    );
    f.render_widget(input, chunks[0]);

    let rulebooks = state.rulebook_switcher_filtered();
    let items: Vec<ListItem> = rulebooks
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let style = if i == state.rulebook_switcher_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let prefix = if state.selected_rulebooks.contains(&r.id) {
                "[x] "
            } else {
                "[ ] "
            };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Green)),
                Span::styled(r.id.clone(), style),
                Span::raw(" - "),
                Span::styled(r.name.clone(), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Rulebooks (Space to toggle, Enter to confirm)"),
        )
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
