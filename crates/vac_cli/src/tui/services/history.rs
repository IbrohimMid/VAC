//! History panel service — selection, navigation, revert.
//! VAC-native, not copied from Stakpak.

use ratatui::widgets::ListState;
use vac_core::engine::TaskHistoryEntry;
use vac_core::TaskStatus;

/// History panel state.
#[derive(Default)]
pub struct HistoryState {
    pub list: ListState,
    /// Visual rows per item (2 lines: description + tokens).
    pub rows_per_item: usize,
}

impl HistoryState {
    pub fn new() -> Self {
        Self { list: ListState::default(), rows_per_item: 2 }
    }

    /// Select previous item (up).
    pub fn select_prev(&mut self, entries: &[TaskHistoryEntry]) {
        if entries.is_empty() { return; }
        let i = match self.list.selected() {
            None => entries.len() - 1,
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.list.select(Some(i));
    }

    /// Select next item (down).
    pub fn select_next(&mut self, entries: &[TaskHistoryEntry]) {
        if entries.is_empty() { return; }
        let i = match self.list.selected() {
            None => 0,
            Some(i) => (i + 1).min(entries.len() - 1),
        };
        self.list.select(Some(i));
    }

    /// Get selected entry index.
    pub fn selected(&self) -> Option<usize> {
        self.list.selected()
    }

    /// Clear selection.
    pub fn clear(&mut self) {
        self.list.select(None);
    }

    /// Total visual rows for scroll calculation.
    pub fn total_visual_rows(&self, entries: &[TaskHistoryEntry]) -> usize {
        entries.len() * self.rows_per_item
    }
}

/// Render history items into list items.
pub fn render_history_items(entries: &[TaskHistoryEntry], max_rows: usize) -> Vec<ratatui::widgets::ListItem<'static>> {
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::ListItem;

    entries.iter()
        .take(max_rows)
        .map(|e| {
            let (sym, col) = match &e.status {
                TaskStatus::Completed => ("✓", Color::Green),
                TaskStatus::Failed(_) => ("✗", Color::Red),
                _ => ("·", Color::Gray),
            };
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(format!("{sym} "), Style::default().fg(col)),
                    Span::raw(trunc(&e.description, 38)),
                ]),
                Line::from(Span::styled(
                    format!("  {}tok", e.total_tokens_used),
                    Style::default().fg(Color::DarkGray),
                )),
            ])
        })
        .collect()
}

fn trunc(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().take(max).collect();
    if s.chars().count() > max {
        format!("{}…", chars.into_iter().collect::<String>())
    } else {
        s.to_string()
    }
}
