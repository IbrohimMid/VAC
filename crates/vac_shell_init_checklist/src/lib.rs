//! D18 — init checklist widget.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
pub use vac_shell_contracts::{InitChecklistAction, InitChecklistStatus, InitChecklistViewModel};

#[derive(Debug, Clone, Default)]
pub struct InitChecklistView {
    pub visible: bool,
    pub model: InitChecklistViewModel,
    pub selected: usize,
    pub scroll: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitChecklistKey {
    Up,
    Down,
    Enter,
    Escape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitChecklistEvent {
    Action(InitChecklistAction),
    Consumed,
}

pub fn on_init_key(view: &mut InitChecklistView, key: InitChecklistKey) -> InitChecklistEvent {
    match key {
        InitChecklistKey::Up => {
            if view.selected > 0 {
                view.selected -= 1;
            }
            InitChecklistEvent::Consumed
        }
        InitChecklistKey::Down => {
            if view.selected < view.model.rows.len().saturating_sub(1) {
                view.selected += 1;
            }
            InitChecklistEvent::Consumed
        }
        InitChecklistKey::Enter => {
            if let Some(row) = view.model.rows.get(view.selected) {
                InitChecklistEvent::Action(row.action)
            } else {
                InitChecklistEvent::Consumed
            }
        }
        InitChecklistKey::Escape => InitChecklistEvent::Consumed,
    }
}

fn status_label(s: InitChecklistStatus) -> &'static str {
    match s {
        InitChecklistStatus::Unknown => "?",
        InitChecklistStatus::Ready => "ok",
        InitChecklistStatus::Warning => "!",
        InitChecklistStatus::Blocked => "X",
    }
}

fn status_color(s: InitChecklistStatus) -> Color {
    match s {
        InitChecklistStatus::Unknown => Color::DarkGray,
        InitChecklistStatus::Ready => Color::Green,
        InitChecklistStatus::Warning => Color::Yellow,
        InitChecklistStatus::Blocked => Color::Red,
    }
}

pub fn render_init_checklist(f: &mut Frame, view: &InitChecklistView, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Init Checklist ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(inner);

    if view.model.rows.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  no checklist rows",
                Style::default().fg(Color::DarkGray),
            ))),
            chunks[0],
        );
        return;
    }

    let height = chunks[0].height as usize;
    let total = view.model.rows.len();
    let max_scroll = total.saturating_sub(height);
    let scroll = view.scroll.min(max_scroll);

    let mut left: Vec<Line<'static>> = Vec::new();
    for (i, row) in view.model.rows.iter().enumerate().skip(scroll).take(height) {
        let is_sel = i == view.selected;
        let status = status_label(row.status);
        let status_color = status_color(row.status);
        let label_style = if is_sel {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let status_style = if is_sel {
            Style::default()
                .fg(Color::Black)
                .bg(status_color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD)
        };
        left.push(Line::from(vec![
            Span::styled(format!(" {:>2} ", status), status_style),
            Span::styled(row.label.clone(), label_style),
        ]));
    }

    let paragraph = Paragraph::new(left).wrap(Wrap { trim: false });
    f.render_widget(paragraph, chunks[0]);

    let right_label = if let Some(row) = view.model.rows.get(view.selected) {
        let status = status_label(row.status);
        let status_color = status_color(row.status);
        vec![
            Line::from(Span::styled(
                row.label.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!(" status: {status}"),
                Style::default().fg(status_color),
            )),
            Line::from(Span::styled(
                row.summary.clone(),
                Style::default().fg(Color::Gray),
            )),
        ]
    } else {
        vec![Line::from(Span::styled(
            " no selection",
            Style::default().fg(Color::DarkGray),
        ))]
    };

    if let Some(detail) = view
        .model
        .rows
        .get(view.selected)
        .and_then(|r| r.detail.as_ref())
    {
        let mut lines = right_label;
        lines.push(Line::from(Span::styled(
            detail.clone(),
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), chunks[1]);
    } else {
        f.render_widget(
            Paragraph::new(right_label).wrap(Wrap { trim: false }),
            chunks[1],
        );
    }

    let footer = if let Some(action) = view.model.rows.get(view.selected).map(|r| r.action) {
        let action_str = match action {
            InitChecklistAction::None => "",
            InitChecklistAction::OpenDoctor => "Enter: run /doctor",
            InitChecklistAction::OpenStatus => "Enter: run /status",
            InitChecklistAction::OpenLogs => "Enter: run /logs",
            InitChecklistAction::OpenSessions => "Enter: run /sessions",
            InitChecklistAction::OpenModelSwitcher => "Enter: run /model",
        };
        format!("  ↑↓ select | {action_str} | Esc close")
    } else {
        "  ↑↓ select | Esc close".to_string()
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            footer,
            Style::default().fg(Color::DarkGray),
        ))),
        inner,
    );
}
