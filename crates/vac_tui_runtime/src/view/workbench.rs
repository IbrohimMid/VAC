//! Workbench panel rendering

use crate::app::AppState;
use crate::ui::style::focus_style;
use crate::app::WorkspaceFocus;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::Span,
    widgets::{Block, Borders, Tabs},
};

use super::messages::render_messages;
use super::input::render_input;
use super::operator::render_operator_panel;
use super::operator::render_activity_panel;

pub(super) fn render_workspace(f: &mut Frame, state: &mut AppState, area: Rect) {
    let main_area = if state.side_panel_visible {
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(state.side_panel_width),
                Constraint::Min(0),
            ])
            .split(area);

        crate::services::side_panel::render_side_panel(f, state, h_chunks[0]);
        h_chunks[1]
    } else {
        area
    };

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .margin(1)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(main_area);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(5)])
        .split(body[0]);

    render_messages(f, state, left[0]);
    render_input(f, state, left[1]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Percentage(40),
            Constraint::Percentage(60),
        ])
        .split(body[1]);

    render_operator_panel(f, state, right[0]);
    render_activity_panel(f, state, right[1]);
    render_workbench_panel(f, state, right[2]);
}

pub(super) fn render_workbench_panel(f: &mut Frame, state: &mut AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(area);

    let idx = crate::workbench::active_tab_index(&state.workbench_tab);
    let tabs = Tabs::new(crate::workbench::tab_labels(state))
        .select(idx)
        .block(Block::default().borders(Borders::ALL).title(Span::styled(
            "Workbench",
            focus_style(state.focus == WorkspaceFocus::Workbench),
        )))
        .highlight_style(state.theme.style(crate::services::theme::StyleKey::ListSelected));
    f.render_widget(tabs, chunks[0]);

    crate::workbench::render_active_tab(f, state, chunks[1]);
}
