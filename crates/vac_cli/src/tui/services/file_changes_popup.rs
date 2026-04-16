//! File Changes Popup
//!
//! Compact, searchable popup listing modified files with per-file revert.
//! Complements the full-screen changeset workstation (`show_changeset`).

use crate::tui::app::AppState;
use crate::tui::services::changeset::FileState;
use crate::tui::services::detect_term::ThemeColors;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
};

pub fn filtered_paths(state: &AppState) -> Vec<String> {
    let query = state.file_changes_search.to_lowercase();
    state
        .changeset_store
        .entries()
        .iter()
        .filter(|e| query.is_empty() || e.path.to_lowercase().contains(&query))
        .map(|e| e.path.clone())
        .collect()
}

pub fn render_file_changes_popup(f: &mut Frame, state: &AppState) {
    const MIN_POPUP_WIDTH: u16 = 80;
    let terminal_area = f.area();
    let width = std::cmp::max(terminal_area.width / 2, MIN_POPUP_WIDTH).min(terminal_area.width);
    let height = (terminal_area.height * 60 / 100).max(10);
    let x = (terminal_area.width.saturating_sub(width)) / 2;
    let y = (terminal_area.height.saturating_sub(height)) / 2;
    let area = Rect::new(x, y, width, height);

    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ThemeColors::cyan()));
    f.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width - 2,
        height: area.height - 2,
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(inner);

    let all_entries: Vec<_> = state.changeset_store.entries().iter().collect();
    let query = state.file_changes_search.to_lowercase();
    let filtered: Vec<_> = all_entries
        .iter()
        .filter(|e| query.is_empty() || e.path.to_lowercase().contains(&query))
        .copied()
        .collect();

    // Title
    let count = filtered.len();
    let count_text = if count == 1 {
        format!("{} file changed", count)
    } else {
        format!("{} files changed", count)
    };
    let available = inner.width as usize;
    let left = " Modified Files";
    let spacing = available.saturating_sub(left.len() + count_text.len() + 1);
    let title_spans = vec![
        Span::styled(
            left,
            Style::default()
                .fg(ThemeColors::yellow())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(spacing)),
        Span::styled(count_text, Style::default().fg(ThemeColors::cyan())),
        Span::raw(" "),
    ];
    f.render_widget(Paragraph::new(Line::from(title_spans)), chunks[0]);

    // Search
    let search_spans = if state.file_changes_search.is_empty() {
        vec![
            Span::raw(" "),
            Span::styled(">", Style::default().fg(ThemeColors::magenta())),
            Span::raw(" "),
            Span::styled("|", Style::default().fg(ThemeColors::cyan())),
            Span::styled("Type to filter", Style::default().fg(ThemeColors::dark_gray())),
        ]
    } else {
        vec![
            Span::raw(" "),
            Span::styled(">", Style::default().fg(ThemeColors::magenta())),
            Span::raw(" "),
            Span::styled(
                state.file_changes_search.clone(),
                Style::default()
                    .fg(ThemeColors::text())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("|", Style::default().fg(ThemeColors::cyan())),
        ]
    };
    f.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(""),
            Line::from(search_spans),
            Line::from(""),
        ])),
        chunks[1],
    );

    // List
    let height = chunks[2].height as usize;
    let total = filtered.len();
    let scroll = state.file_changes_scroll;
    let mut lines: Vec<Line> = Vec::new();

    for i in 0..height {
        let idx = scroll + i;
        if idx >= total {
            break;
        }
        let entry = filtered[idx];
        let is_selected = idx == state.file_changes_selected;
        let bg_color = if is_selected {
            ThemeColors::highlight_bg()
        } else {
            Color::Reset
        };

        let (label, label_color) = match entry.state {
            FileState::Created => ("[+]", ThemeColors::green()),
            FileState::Modified => ("[~]", ThemeColors::yellow()),
            FileState::Removed => ("[-]", ThemeColors::red()),
            FileState::Reverted => ("[✓]", ThemeColors::cyan()),
            FileState::FailedRestore => ("[✗]", ThemeColors::red()),
        };

        let name_style = match entry.state {
            FileState::Reverted | FileState::Removed | FileState::FailedRestore => {
                Style::default()
                    .fg(ThemeColors::dark_gray())
                    .add_modifier(Modifier::CROSSED_OUT)
                    .bg(bg_color)
            }
            _ => {
                let s = if is_selected {
                    Style::default().fg(ThemeColors::highlight_fg())
                } else {
                    Style::default().fg(Color::Reset)
                };
                s.bg(bg_color)
            }
        };

        lines.push(Line::from(vec![
            Span::styled(" ", Style::default().bg(bg_color)),
            Span::styled(label, Style::default().fg(label_color).bg(bg_color)),
            Span::styled(" ", Style::default().bg(bg_color)),
            Span::styled(entry.path.clone(), name_style),
        ]));
    }
    f.render_widget(Paragraph::new(lines), chunks[2]);

    // Footer
    let footer = vec![
        Span::raw(" "),
        Span::styled("↑/↓", Style::default().fg(ThemeColors::cyan())),
        Span::styled(": Navigate  ", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled("Ctrl+X", Style::default().fg(ThemeColors::cyan())),
        Span::styled(": Revert  ", Style::default().fg(ThemeColors::dark_gray())),
        Span::styled("Esc", Style::default().fg(ThemeColors::cyan())),
        Span::styled(": Close", Style::default().fg(ThemeColors::dark_gray())),
    ];
    f.render_widget(Paragraph::new(Line::from(footer)), chunks[3]);
}
