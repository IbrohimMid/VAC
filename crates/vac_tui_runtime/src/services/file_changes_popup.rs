//! File Changes Popup
//!
//! Compact, searchable popup listing modified files with per-file revert.
//! Complements the full-screen changeset workstation (`show_changeset`).

use crate::app::AppState;
use crate::services::theme::StyleKey;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color as C, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
};
use vac_changeset::FileState;

pub fn filtered_paths(state: &AppState) -> Vec<String> {
    let query = state.workspace.file_index.changes_search.to_lowercase();
    state
        .workspace
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
        .border_style(state.core.theme.style(StyleKey::Accent));
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

    let all_entries: Vec<_> = state.workspace.changeset_store.entries().iter().collect();
    let query = state.workspace.file_index.changes_search.to_lowercase();
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
            state
                .core
                .theme
                .style(StyleKey::Warning)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(spacing)),
        Span::styled(count_text, state.core.theme.style(StyleKey::Accent)),
        Span::raw(" "),
    ];
    f.render_widget(Paragraph::new(Line::from(title_spans)), chunks[0]);

    // Search
    let search_spans = if state.workspace.file_index.changes_search.is_empty() {
        vec![
            Span::raw(" "),
            Span::styled(">", state.core.theme.style(StyleKey::AppTitle)),
            Span::raw(" "),
            Span::styled("|", state.core.theme.style(StyleKey::Accent)),
            Span::styled("Type to filter", state.core.theme.style(StyleKey::Muted)),
        ]
    } else {
        vec![
            Span::raw(" "),
            Span::styled(">", state.core.theme.style(StyleKey::AppTitle)),
            Span::raw(" "),
            Span::styled(
                state.workspace.file_index.changes_search.clone(),
                state
                    .core
                    .theme
                    .style(StyleKey::Text)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("|", state.core.theme.style(StyleKey::Accent)),
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
    let scroll = state.workspace.file_index.changes_scroll;
    let mut lines: Vec<Line> = Vec::new();

    for i in 0..height {
        let idx = scroll + i;
        if idx >= total {
            break;
        }
        let entry = filtered[idx];
        let is_selected = idx == state.workspace.file_index.changes_selected;
        let bg_color = if is_selected {
            state
                .core
                .theme
                .style(StyleKey::HighlightBg)
                .bg
                .unwrap_or(C::Reset)
        } else {
            C::Reset
        };

        let (label, label_style) = match entry.state {
            FileState::Created => ("[+]", state.core.theme.style(StyleKey::DiffAdded)),
            FileState::Modified => ("[~]", state.core.theme.style(StyleKey::Warning)),
            FileState::Removed => ("[-]", state.core.theme.style(StyleKey::DiffRemoved)),
            FileState::Reverted => ("[✓]", state.core.theme.style(StyleKey::Accent)),
            FileState::FailedRestore => ("[✗]", state.core.theme.style(StyleKey::Error)),
        };

        let name_style = match entry.state {
            FileState::Reverted | FileState::Removed | FileState::FailedRestore => state
                .core
                .theme
                .style(StyleKey::Muted)
                .add_modifier(Modifier::CROSSED_OUT)
                .bg(bg_color),
            _ => {
                let s = if is_selected {
                    state.core.theme.style(StyleKey::HighlightFg)
                } else {
                    Style::default()
                };
                s.bg(bg_color)
            }
        };

        lines.push(Line::from(vec![
            Span::styled(" ", Style::default().bg(bg_color)),
            Span::styled(label, label_style.bg(bg_color)),
            Span::styled(" ", Style::default().bg(bg_color)),
            Span::styled(entry.path.clone(), name_style),
        ]));
    }
    f.render_widget(Paragraph::new(lines), chunks[2]);

    // Footer
    let footer = vec![
        Span::raw(" "),
        Span::styled("↑/↓", state.core.theme.style(StyleKey::Accent)),
        Span::styled(": Navigate  ", state.core.theme.style(StyleKey::Muted)),
        Span::styled("Ctrl+X", state.core.theme.style(StyleKey::Accent)),
        Span::styled(": Revert  ", state.core.theme.style(StyleKey::Muted)),
        Span::styled("Esc", state.core.theme.style(StyleKey::Accent)),
        Span::styled(": Close", state.core.theme.style(StyleKey::Muted)),
    ];
    f.render_widget(Paragraph::new(Line::from(footer)), chunks[3]);
}
