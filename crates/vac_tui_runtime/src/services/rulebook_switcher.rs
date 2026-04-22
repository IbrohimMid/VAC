use crate::app::AppState;
use crate::services::theme::StyleKey;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
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
        Span::styled("Filter ", state.theme.style(StyleKey::Muted)),
        Span::raw(&state.switchers.rulebook_search),
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
            let style = if i == state.switchers.rulebook_selected {
                state.theme.style(StyleKey::ListSelected)
            } else {
                Style::default()
            };
            let prefix = if state.switchers.selected_rulebooks.contains(&r.id) {
                "[x] "
            } else {
                "[ ] "
            };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, state.theme.style(StyleKey::Success)),
                Span::styled(r.id.clone(), style),
                Span::raw(" - "),
                Span::styled(r.name.clone(), state.theme.style(StyleKey::Muted)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Rulebooks (Space to toggle, Enter to confirm)"),
        )
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
