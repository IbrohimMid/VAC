use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::tui::app::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageAction {
    CopyMessage,
    RevertToMessage,
}

impl MessageAction {
    pub fn all() -> Vec<Self> {
        vec![Self::CopyMessage, Self::RevertToMessage]
    }
}

pub fn render_message_action_popup(f: &mut Frame, state: &AppState) {
    if !state.show_message_action_popup {
        return;
    }

    let popup_width: u16 = 50;
    let popup_height: u16 = 7;

    let terminal_area = f.area();
    let x = (terminal_area.width.saturating_sub(popup_width)) / 2;
    let y = (terminal_area.height.saturating_sub(popup_height)) / 2;

    let area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    f.render_widget(block, area);

    let inner_area = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(2),
        ])
        .split(inner_area);

    let title = Paragraph::new(Line::from(vec![Span::styled(
        " Message Action",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]));
    f.render_widget(title, chunks[0]);

    let actions = MessageAction::all();
    let mut item_lines: Vec<Line> = Vec::new();

    for (idx, action) in actions.iter().enumerate() {
        let is_selected = idx == state.message_action_popup_selected;

        let (highlight_word, rest_text) = match action {
            MessageAction::CopyMessage => ("Copy", " message text to clipboard"),
            MessageAction::RevertToMessage => ("Revert", " undo messages and file changes"),
        };

        let available_width = (inner_area.width as usize).saturating_sub(2);
        let text_len = 2 + highlight_word.len() + rest_text.len();
        let padding = available_width.saturating_sub(text_len);

        let line = if is_selected {
            Line::from(vec![
                Span::styled(
                    "  ",
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White),
                ),
                Span::styled(
                    highlight_word,
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    rest_text,
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White),
                ),
                Span::styled(
                    " ".repeat(padding),
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White),
                ),
            ])
        } else {
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    highlight_word,
                    Style::default().fg(Color::Reset),
                ),
                Span::styled(rest_text, Style::default().fg(Color::DarkGray)),
            ])
        };

        item_lines.push(line);
    }

    let items = Paragraph::new(item_lines);
    f.render_widget(items, chunks[2]);
}

pub fn get_selected_action(state: &AppState) -> Option<MessageAction> {
    let actions = MessageAction::all();
    actions.get(state.message_action_popup_selected).copied()
}