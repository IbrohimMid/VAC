//! Workbench panel rendering

use crate::app::AppState;
use crate::app::WorkspaceFocus;
use crate::ui::style::focus_style;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::Span,
    widgets::{Block, Borders, Tabs},
};

use super::input::render_input;
use super::messages::render_messages;
use super::operator::render_activity_panel;
use super::operator::render_operator_panel;

pub(super) fn render_workspace(f: &mut Frame, state: &mut AppState, area: Rect) {
    let main_area = if state.layout.side_panel.visible {
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(state.layout.side_panel.width),
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
    // Dogfood F2 follow-up: cache the input Rect so overlay
    // renderers can anchor dropdowns directly above the input
    // border instead of guessing from frame bottom.
    state.layout.input_area = Some(left[1]);
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

    let idx = crate::workbench::active_tab_index(&state.layout.workbench_tab);
    let labels = crate::workbench::tab_labels(state);
    let tabs = Tabs::new(labels.clone())
        .select(idx)
        .block(Block::default().borders(Borders::ALL).title(Span::styled(
            "Workbench",
            focus_style(
                state.layout.focus == WorkspaceFocus::Workbench,
                &state.core.theme,
            ),
        )))
        .highlight_style(
            state
                .core
                .theme
                .style(crate::services::theme::StyleKey::ListSelected),
        );
    f.render_widget(tabs, chunks[0]);

    // PR-T16 — record per-tab click regions for the mouse dispatcher.
    // Layout mirrors ratatui::Tabs rendering: each label is drawn inside the
    // bordered block on the first inner row, separated by `" │ "` (3 cols).
    state.layout.workbench_chrome.tab_regions.clear();
    if chunks[0].height >= 3 && chunks[0].width >= 3 {
        let inner_y = chunks[0].y + 1;
        let inner_x_start = chunks[0].x + 1;
        let inner_x_end = chunks[0].x + chunks[0].width - 1;
        let mut x = inner_x_start;
        for (i, label) in labels.iter().enumerate() {
            if x >= inner_x_end {
                break;
            }
            let w = label.chars().count() as u16;
            let avail = inner_x_end - x;
            let rect_w = w.min(avail);
            let rect = ratatui::layout::Rect::new(x, inner_y, rect_w, 1);
            state
                .layout
                .workbench_chrome
                .tab_regions
                .push((crate::workbench::tab_from_index(i), rect));
            x = x.saturating_add(w + 3); // " │ " separator between tabs
        }
    }

    crate::workbench::render_active_tab(f, state, chunks[1]);
}
