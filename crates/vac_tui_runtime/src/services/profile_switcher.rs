use crate::app::AppState;
use crate::services::theme::StyleKey;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
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
        Span::styled("Filter ", state.theme.style(StyleKey::Muted)),
        Span::raw(&state.switchers.profile_search),
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
            let style = if i == state.switchers.profile_selected {
                state.theme.style(StyleKey::ListSelected)
            } else {
                Style::default()
            };
            let prefix = if p == &state.switchers.active_profile {
                "* "
            } else {
                "  "
            };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, state.theme.style(StyleKey::Success)),
                Span::styled(p.clone(), style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Profiles"))
        .highlight_style(state.theme.style(StyleKey::ListSelected));
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
