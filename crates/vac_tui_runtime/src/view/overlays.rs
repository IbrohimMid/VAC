//! Modal overlays: model switcher, file search, changeset, toast, command palette, shortcuts

use crate::app::AppState;
use crate::services::ToastStyle;
use crate::services::theme::StyleKey;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

pub(super) fn render_model_switcher(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(70, 60, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let input = Paragraph::new(Line::from(vec![
        Span::styled("Filter ", state.theme.style(StyleKey::Muted)),
        Span::raw(&state.switchers.model_filter),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Model Switcher"),
    );
    f.render_widget(input, chunks[0]);

    let models = state.model_switcher_filtered();
    let items: Vec<ListItem> = models
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let style = if i == state.switchers.model_selected {
                state.theme.style(StyleKey::ListSelected)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{}  ", m.provider),
                    state.theme.style(StyleKey::Muted),
                ),
                Span::styled(m.name.clone(), style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Models"))
        .highlight_style(state.theme.style(StyleKey::ListSelected));
    f.render_widget(list, chunks[1]);
}

pub(super) fn render_file_search(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(80, 70, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let input = Paragraph::new(Line::from(vec![
        Span::styled("Query ", state.theme.style(StyleKey::Muted)),
        Span::raw(&state.file_search_query),
    ]))
    .block(Block::default().borders(Borders::ALL).title("File Search"));
    f.render_widget(input, chunks[0]);

    let items: Vec<ListItem> = state
        .file_search_results
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let style = if i == state.file_search_selected_idx {
                state.theme.style(StyleKey::ListSelected)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(path.clone(), style)))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Files"))
        .highlight_style(state.theme.style(StyleKey::ListSelected));
    f.render_widget(list, chunks[1]);
}

pub(super) fn render_changeset(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(90, 80, f.area());
    f.render_widget(Clear, area);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    let entries = state.changeset_store.entries();
    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let style = if i == state.changeset_ui.selected_idx {
                state.theme.style(StyleKey::ListSelected)
            } else {
                Style::default()
            };
            let indicator = match entry.state {
                vac_changeset::FileState::Created => "[+]",
                vac_changeset::FileState::Modified => "[~]",
                vac_changeset::FileState::Removed => "[-]",
                vac_changeset::FileState::Reverted => "[✓]",
                vac_changeset::FileState::FailedRestore => "[✗]",
            };
            let indicator_style = match entry.state {
                vac_changeset::FileState::Created => state.theme.style(StyleKey::Success),
                vac_changeset::FileState::Modified => state.theme.style(StyleKey::Warning),
                vac_changeset::FileState::Removed => state.theme.style(StyleKey::Error),
                vac_changeset::FileState::Reverted => state.theme.style(StyleKey::Accent),
                vac_changeset::FileState::FailedRestore => state.theme.style(StyleKey::Error),
            };
            ListItem::new(Line::from(vec![
                Span::styled(indicator, indicator_style),
                Span::raw(" "),
                Span::styled(entry.path.clone(), style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Changeset"))
        .highlight_style(state.theme.style(StyleKey::ListSelected));
    f.render_widget(list, body[0]);

    let width = body[1].width.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();
    if let Some(diff) = &state.changeset_ui.diff {
        if let Some(err) = &diff.last_error {
            lines.push(Line::styled(
                err.clone(),
                state
                    .theme
                    .style(StyleKey::Error)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ));
        } else if let (Some(old), Some(new)) = (&diff.old_content, &diff.new_content) {
            lines.extend(crate::services::preview_file_diff(
                &state.theme,
                &diff.path,
                old,
                new,
                width,
            ));
        } else {
            lines.push(Line::styled(
                "No diff available",
                state.theme.style(StyleKey::Muted),
            ));
        }
    } else {
        lines.push(Line::styled(
            "Select a file to preview diff",
            state.theme.style(StyleKey::Muted),
        ));
    }

    let detail = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Preview"))
        .wrap(Wrap { trim: false })
        .scroll((state.changeset_ui.diff_scroll as u16, 0));
    f.render_widget(detail, body[1]);
}

pub(super) fn render_toast(f: &mut Frame, state: &mut AppState) {
    let Some(toast) = state.toasts.last() else {
        return;
    };

    let area = f.area();
    let max_width = area.width.saturating_sub(2).min(60);
    let text_width = toast.message.chars().count() as u16;
    let width = (text_width + 4).min(max_width).max(10);
    let height = 3u16.min(area.height.saturating_sub(1)).max(1);
    let x = area.x + area.width.saturating_sub(width + 1);
    let y = area.y + 1;

    let toast_style = match toast.style {
        ToastStyle::Success => state.theme.style(StyleKey::ToastSuccess),
        ToastStyle::Error => state.theme.style(StyleKey::ToastError),
        ToastStyle::Warning => state.theme.style(StyleKey::ToastWarning),
        ToastStyle::Info => state.theme.style(StyleKey::ToastInfo),
    };

    let rect = Rect {
        x,
        y,
        width,
        height,
    };
    f.render_widget(Clear, rect);
    let widget = Paragraph::new(Line::from(Span::raw(toast.message.clone())))
        .style(toast_style)
        .block(Block::default().borders(Borders::ALL).style(toast_style))
        .wrap(Wrap { trim: true });
    f.render_widget(widget, rect);
}

pub(super) fn render_command_palette(f: &mut Frame, state: &mut AppState) {
    let area = centered_rect(60, 40, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    // Input
    let input = Paragraph::new(Line::from(vec![
        Span::styled("/", state.theme.style(StyleKey::Warning)),
        Span::raw(&state.command_palette_input),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Command"));
    f.render_widget(input, chunks[0]);

    // Commands list
    let filtered = state.filtered_commands();
    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .map(|(i, cmd)| {
            let style = if i == state.command_palette_selected {
                state.theme.style(StyleKey::ListSelected)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(&cmd.command, style),
                Span::raw(" - "),
                Span::styled(&cmd.description, state.theme.style(StyleKey::Muted)),
            ]))
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Commands"));
    f.render_widget(list, chunks[1]);
}

pub(super) fn render_shortcuts(f: &mut Frame, _state: &mut AppState) {
    let area = centered_rect(70, 60, f.area());
    f.render_widget(Clear, area);

    let shortcuts = vec![
        "Ctrl+P - Command palette",
        "Ctrl+C - Quit",
        "Esc - Cancel/Close",
        "Up/Down - Scroll/Navigate",
        "Enter - Submit/Select",
        "Ctrl+L - Toggle mouse capture",
        "Ctrl+X - Revert selected (Review)",
        "Ctrl+Y - Revert filtered (Review)",
        "Ctrl+Z - Revert all (Review)",
        "Ctrl+N - Open in editor (Review)",
        "PageUp/PageDown - Scroll diff (Review)",
        "Ctrl+G - Open review workstation",
        "Ctrl+F - Toggle auto-approve",
        "Tab - Cycle focus panes",
        "Ctrl+Tab - Cycle workbench tabs",
        "a/r - Approve/Reject selected (Approvals tab)",
    ];
    let items: Vec<ListItem> = shortcuts
        .iter()
        .map(|s| ListItem::new(Line::raw(*s)))
        .collect();
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Shortcuts (Esc to close)"),
    );
    f.render_widget(list, area);
}

pub(crate) fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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
