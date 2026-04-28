use crate::app::AppState;
use crate::services::theme::StyleKey;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem},
};

pub fn render_isolation_switcher(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(40, 30, f.area());
    f.render_widget(Clear, area);

    let items: Vec<ListItem> = state
        .layout
        .switchers
        .isolation_modes
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let style = if i == state.layout.switchers.isolation_selected {
                state.core.theme.style(StyleKey::ListSelected)
            } else {
                Style::default()
            };
            let prefix = if p == &state.layout.switchers.active_isolation_mode {
                "* "
            } else {
                "  "
            };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, state.core.theme.style(StyleKey::Success)),
                Span::styled(p.clone(), style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Isolation Environment"),
        )
        .highlight_style(state.core.theme.style(StyleKey::ListSelected));
    f.render_widget(list, area);
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
