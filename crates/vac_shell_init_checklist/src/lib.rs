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

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let body = vertical[0];
    let footer_area = vertical[1];

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(body);

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

    let mut right_lines = if let Some(row) = view.model.rows.get(view.selected) {
        let status = status_label(row.status);
        let status_color = status_color(row.status);
        let mut lines = vec![
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
        ];
        if let Some(detail) = row.detail.as_ref() {
            lines.push(Line::from(Span::styled(
                detail.clone(),
                Style::default().fg(Color::DarkGray),
            )));
        }
        lines
    } else {
        vec![Line::from(Span::styled(
            " no selection",
            Style::default().fg(Color::DarkGray),
        ))]
    };

    if let Some(next) = &view.model.next_action {
        right_lines.push(Line::from(Span::styled(
            format!(" next: {next}"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
    }

    f.render_widget(
        Paragraph::new(right_lines).wrap(Wrap { trim: false }),
        chunks[1],
    );

    let action_hint = if let Some(action) = view.model.rows.get(view.selected).map(|r| r.action) {
        match action {
            InitChecklistAction::None => "",
            InitChecklistAction::OpenDoctor => "Enter: /doctor",
            InitChecklistAction::OpenStatus => "Enter: /status",
            InitChecklistAction::OpenLogs => "Enter: /logs",
            InitChecklistAction::OpenSessions => "Enter: /sessions",
            InitChecklistAction::OpenModelSwitcher => "Enter: /model",
        }
    } else {
        ""
    };
    let footer_text = if action_hint.is_empty() {
        " ↑↓ navigate | Esc close".to_string()
    } else {
        format!(" ↑↓ navigate | {action_hint} | Esc close")
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            footer_text,
            Style::default().fg(Color::DarkGray),
        ))),
        footer_area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn sample_rows() -> Vec<vac_shell_contracts::InitChecklistRow> {
        vec![
            vac_shell_contracts::InitChecklistRow {
                id: "model".into(),
                label: "Model".into(),
                status: InitChecklistStatus::Ready,
                summary: "model: claude-sonnet-4.5".into(),
                detail: Some("Use /model to switch".into()),
                action: InitChecklistAction::OpenModelSwitcher,
            },
            vac_shell_contracts::InitChecklistRow {
                id: "sessions".into(),
                label: "Sessions".into(),
                status: InitChecklistStatus::Unknown,
                summary: "sessions: 0 total".into(),
                detail: Some("Use /sessions to browse".into()),
                action: InitChecklistAction::OpenSessions,
            },
            vac_shell_contracts::InitChecklistRow {
                id: "doctor".into(),
                label: "Doctor".into(),
                status: InitChecklistStatus::Warning,
                summary: "doctor: Warning".into(),
                detail: Some("Some checks have warnings".into()),
                action: InitChecklistAction::OpenDoctor,
            },
        ]
    }

    fn view_with_rows(rows: Vec<vac_shell_contracts::InitChecklistRow>) -> InitChecklistView {
        InitChecklistView {
            visible: true,
            model: InitChecklistViewModel {
                title: "Init Checklist".into(),
                rows,
                next_action: Some("ready to start".into()),
            },
            selected: 0,
            scroll: 0,
        }
    }

    #[test]
    fn down_moves_selection() {
        let mut view = view_with_rows(sample_rows());
        on_init_key(&mut view, InitChecklistKey::Down);
        assert_eq!(view.selected, 1);
    }

    #[test]
    fn down_stops_at_last_row() {
        let mut view = view_with_rows(sample_rows());
        view.selected = 2;
        on_init_key(&mut view, InitChecklistKey::Down);
        assert_eq!(view.selected, 2);
    }

    #[test]
    fn up_stops_at_zero() {
        let mut view = view_with_rows(sample_rows());
        on_init_key(&mut view, InitChecklistKey::Up);
        assert_eq!(view.selected, 0);
    }

    #[test]
    fn enter_returns_selected_row_action() {
        let mut view = view_with_rows(sample_rows());
        view.selected = 0;
        let event = on_init_key(&mut view, InitChecklistKey::Enter);
        assert_eq!(
            event,
            InitChecklistEvent::Action(InitChecklistAction::OpenModelSwitcher)
        );
    }

    #[test]
    fn enter_on_doctor_row_returns_open_doctor() {
        let mut view = view_with_rows(sample_rows());
        view.selected = 2;
        let event = on_init_key(&mut view, InitChecklistKey::Enter);
        assert_eq!(
            event,
            InitChecklistEvent::Action(InitChecklistAction::OpenDoctor)
        );
    }

    #[test]
    fn enter_on_empty_rows_consumed() {
        let mut view = InitChecklistView::default();
        let event = on_init_key(&mut view, InitChecklistKey::Enter);
        assert_eq!(event, InitChecklistEvent::Consumed);
    }

    #[test]
    fn escape_is_consumed() {
        let mut view = view_with_rows(sample_rows());
        let event = on_init_key(&mut view, InitChecklistKey::Escape);
        assert_eq!(event, InitChecklistEvent::Consumed);
    }

    #[test]
    fn render_does_not_panic_with_rows() {
        let view = view_with_rows(sample_rows());
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render_init_checklist(f, &view, f.area()))
            .unwrap();
    }

    #[test]
    fn render_does_not_panic_with_empty_rows() {
        let view = InitChecklistView::default();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render_init_checklist(f, &view, f.area()))
            .unwrap();
    }

    #[test]
    fn footer_does_not_overwrite_body() {
        let view = view_with_rows(sample_rows());
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render_init_checklist(f, &view, f.area()))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let footer_y = (24 - 2) as usize;
        let footer_start = (footer_y * 80) as usize;
        let footer_content: String = buf.content[footer_start..footer_start + 80]
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(
            footer_content.contains("Esc close"),
            "footer should contain 'Esc close', got: {footer_content:?}"
        );
    }
}
