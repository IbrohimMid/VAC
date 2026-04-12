//! View — layout and rendering.
//! Geometry tracking pattern adapted from stakpak/tui/src/view.rs (Apache-2.0).

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

use super::app::{FocusPane, TuiApp};
use super::services::detail::{DetailMode, render_detail_content};
use super::services::history::render_history_items;
use super::services::transcript::render_transcript_lines;

pub fn render(frame: &mut Frame, app: &mut TuiApp) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(10), Constraint::Length(4)])
        .split(area);

    render_header(frame, chunks[0], app);
    render_body(frame, chunks[1], app);
    render_composer(frame, chunks[2], app);

    if app.pending_approval.is_some() {
        render_approval_modal(frame, area, app);
    }
}

// ── Header ────────────────────────────────────────────────────────────────────

fn render_header(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let session_info = if app.sessions.len() > 1 {
        format!("{} ({}/{})", app.sessions[app.active_session].label, app.active_session + 1, app.sessions.len())
    } else {
        app.sessions[app.active_session].label.clone()
    };

    let busy_indicator = if app.session().active_task.is_some() {
        format!("{} {} | {}", spinner(app.spinner_tick), trunc(app.session().active_task.as_deref().unwrap_or(""), 44), app.last_activity)
    } else {
        format!("🟢 Idle — {}", app.last_activity)
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("VAC", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(busy_indicator, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(format!("{} | {} | Phase: {}", app.active_provider, app.active_model, app.current_phase)),
        Line::from(format!("{session_info} | Tasks {} done {} failed {} | Tab:focus={:?}",
            app.status.total_tasks, app.status.completed_tasks, app.status.failed_tasks, app.focus)),
    ];

    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

// ── Body ──────────────────────────────────────────────────────────────────────

fn render_body(frame: &mut Frame, area: Rect, app: &mut TuiApp) {
    let show_bottom = app.detail.is_some() || !app.live_diff_files.is_empty();

    // Left: transcript + optional bottom panel
    let left_right = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(area);

    let left = if show_bottom {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(9)])
            .split(left_right[0])
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(0)])
            .split(left_right[0])
    };

    render_transcript(frame, left[0], app);
    if show_bottom {
        render_detail_panel(frame, left[1], app);
    }

    // Right: inspector + history + lanes
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Length(9), Constraint::Min(6)])
        .split(left_right[1]);

    render_inspector(frame, right[0], app);
    render_history(frame, right[1], app);
    render_lanes(frame, right[2], app);
}

// ── Transcript ────────────────────────────────────────────────────────────────

fn render_transcript(frame: &mut Frame, area: Rect, app: &mut TuiApp) {
    // Store geometry for click detection
    app.scroll.transcript.area = area;

    let max_width = area.width.saturating_sub(4) as usize;
    let visible_h = area.height.saturating_sub(2) as usize;

    let all_lines = render_transcript_lines(app.session().transcript.as_slice(), max_width);
    let total = all_lines.len();

    // Clamp scroll (writeback pattern from Stakpak view.rs)
    let scroll = app.scroll.transcript.clamp(total, visible_h);

    let visible: Vec<Line> = all_lines.into_iter().skip(scroll).take(visible_h).collect();

    let focused = app.focus == FocusPane::Transcript;
    let border_style = if focused { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::Gray) };
    let title = if focused { "Transcript [PgUp/Dn scroll]" } else { "Transcript [Tab to focus]" };

    frame.render_widget(
        Paragraph::new(visible)
            .block(Block::default().title(title).borders(Borders::ALL).border_style(border_style)),
        area,
    );
}

// ── Detail panel (bottom-left, contextual) ────────────────────────────────────

fn render_detail_panel(frame: &mut Frame, area: Rect, app: &mut TuiApp) {
    // Store geometry for click detection
    app.scroll.detail.area = area;

    let max_w = area.width.saturating_sub(4) as usize;
    let (lines, total_rows) = render_detail_content(
        &app.detail,
        &app.session().history,
        &app.live_diff_files,
        max_w.max(20),
    );

    // Update scroll with correct total
    let visible_h = area.height.saturating_sub(2) as usize;
    let scroll_offset = app.scroll.detail.clamp(total_rows, visible_h);

    // Apply scroll offset to visible lines
    let visible_lines: Vec<Line> = lines.into_iter()
        .skip(scroll_offset)
        .take(visible_h)
        .collect();

    let title = match &app.detail {
        DetailMode::LiveChanges => "Live Changes",
        DetailMode::ErrorDetail(_) => "Error",
        DetailMode::TaskDetail(_) => "Task Detail",
        DetailMode::RevertConfirm(_) => "Confirm Revert",
        DetailMode::None => "Detail",
    };
    let border_color = match &app.detail {
        DetailMode::LiveChanges => Color::Yellow,
        DetailMode::ErrorDetail(_) => Color::Red,
        DetailMode::RevertConfirm(_) => Color::Magenta,
        _ => Color::DarkGray,
    };
    let focused = app.focus == FocusPane::Detail;
    let border_style = if focused { Style::default().fg(Color::Blue) } else { Style::default().fg(border_color) };

    frame.render_widget(
        Paragraph::new(visible_lines)
            .block(Block::default().title(title).borders(Borders::ALL).border_style(border_style))
            .wrap(Wrap { trim: true }),
        area,
    );
}

// ── Inspector ─────────────────────────────────────────────────────────────────

fn render_inspector(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let mut lines = vec![
        Line::from(format!("Auth: {}", if app.auth_ready { "✓" } else { "✗ run vac auth login" })),
        Line::from(format!("Subsystems: {}", if app.status.subsystems_initialized { "ready" } else { "booting" })),
        Line::from(format!("Busy: {}", app.session().active_task.as_deref().map(|t| trunc(t, 32)).unwrap_or_else(|| "no".to_string()))),
        Line::from(format!("Phase: {}", trunc(&app.current_phase, 36))),
    ];
    if app.show_help {
        lines.push(Line::from(""));
        lines.push(Line::from("Enter  run task"));
        lines.push(Line::from("Tab    cycle focus"));
        lines.push(Line::from("PgUp/Dn scroll pane"));
        lines.push(Line::from("↑↓ in History: navigate"));
        lines.push(Line::from("Enter/R/D in History"));
        lines.push(Line::from("Ctrl-N/]/[ sessions"));
        lines.push(Line::from("q quit"));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Inspector [Tab=help]").borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        area,
    );
}

// ── History ───────────────────────────────────────────────────────────────────

fn render_history(frame: &mut Frame, area: Rect, app: &mut TuiApp) {
    // Store geometry for click detection
    app.scroll.history.area = area;

    let visible_h = area.height.saturating_sub(2) as usize;
    let total_items = app.session().history.len();
    let total_rows = total_items * 2; // Each item is 2 visual rows
    
    // Clamp scroll
    let scroll_offset = app.scroll.history.clamp(total_rows, visible_h);
    let skip_items = scroll_offset / 2;
    
    // Render items with scroll offset
    let items = render_history_items(
        &app.session().history.iter().skip(skip_items).cloned().collect::<Vec<_>>(),
        visible_h,
    );

    let focused = app.focus == FocusPane::History;
    let border_style = if focused { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::Blue) };
    let title = if focused { "History [↑↓ Enter R D]" } else { "History [Tab to focus]" };

    let mut state = app.history.list.clone();
    frame.render_stateful_widget(
        ratatui::widgets::List::new(items)
            .block(Block::default().title(title).borders(Borders::ALL).border_style(border_style))
            .highlight_style(Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD))
            .highlight_symbol("▶ "),
        area,
        &mut state,
    );
}

// ── Lanes ─────────────────────────────────────────────────────────────────────

fn render_lanes(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(33), Constraint::Percentage(34), Constraint::Percentage(33)])
        .split(area);

    render_lane(frame, rows[0], "Thinking / Plan", &app.session().thinking_log, Color::LightYellow);
    render_lane(frame, rows[1], "Reading / Search", &app.session().reading_log, Color::Cyan);
    render_lane(frame, rows[2], "Commands / Writes", &app.session().commands_log, Color::LightGreen);
}

fn render_lane(frame: &mut Frame, area: Rect, title: &str, items: &[String], color: Color) {
    let visible = area.height.saturating_sub(2) as usize;
    let list_items: Vec<ListItem> = items.iter().rev().take(visible.max(1))
        .collect::<Vec<_>>().into_iter().rev()
        .map(|msg| ListItem::new(Line::from(Span::styled(msg.clone(), Style::default().fg(color)))))
        .collect();
    frame.render_widget(
        List::new(list_items).block(Block::default()
            .title(Span::styled(title, Style::default().fg(color).add_modifier(Modifier::BOLD)))
            .borders(Borders::ALL).border_style(Style::default().fg(color))),
        area,
    );
}

// ── Composer ──────────────────────────────────────────────────────────────────

fn render_composer(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let busy = app.session().active_task.is_some();
    let border_style = Style::default().fg(if busy { Color::Yellow } else { Color::Cyan });
    let title = if busy { "Composer (busy)" } else { "Composer" };

    let text = if app.input.is_empty() {
        vec![Line::from(Span::styled("Describe the task...", Style::default().fg(Color::DarkGray)))]
    } else {
        vec![Line::from(app.input.clone())]
    };

    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(text).block(Block::default().title(title).borders(Borders::ALL).border_style(border_style)),
        area,
    );

    if !busy && app.pending_approval.is_none() {
        let cx = area.x + app.input.chars().count() as u16 + 1;
        let cy = area.y + 1;
        frame.set_cursor_position((cx.min(area.x + area.width - 2), cy));
    }
}

// ── Approval modal ────────────────────────────────────────────────────────────

fn render_approval_modal(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let Some(pending) = &app.pending_approval else { return };
    let modal = centered_rect(60, 40, area);
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(format!("Approve `{}`?", pending.tool_name), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from(pending.summary.clone()),
            Line::from(""),
            Line::from(trunc(&pending.args_preview, 200)),
            Line::from(""),
            Line::from("[y] Allow  [n/Esc] Deny"),
        ])
        .block(Block::default().title("Approval").borders(Borders::ALL).border_style(Style::default().fg(Color::Yellow)))
        .wrap(Wrap { trim: true }),
        modal,
    );
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn trunc(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n { out.push('…'); }
    out
}

fn spinner(tick: usize) -> &'static str {
    ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"][tick % 10]
}

fn centered_rect(w_pct: u16, h_pct: u16, area: Rect) -> Rect {
    let v = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Percentage((100-h_pct)/2), Constraint::Percentage(h_pct), Constraint::Percentage((100-h_pct)/2)])
        .split(area);
    Layout::default().direction(Direction::Horizontal)
        .constraints([Constraint::Percentage((100-w_pct)/2), Constraint::Percentage(w_pct), Constraint::Percentage((100-w_pct)/2)])
        .flex(Flex::Center).split(v[1])[1]
}
