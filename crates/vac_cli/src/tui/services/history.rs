//! History panel service — selection, navigation, revert.
//! VAC-native, not copied from Stakpak.

use ratatui::widgets::ListState;
use vac_core::engine::TaskHistoryEntry;
use vac_core::TaskStatus;

/// History panel state.
#[derive(Default)]
pub struct HistoryState {
    pub list: ListState,
    // TODO Task 8.2: Add filter: Option<String> for search
}

impl HistoryState {
    pub fn new() -> Self {
        Self { list: ListState::default() }
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

    /// Total visual rows for scroll calculation (2 lines per item: description + tokens).
    pub fn total_visual_rows(&self, entries: &[TaskHistoryEntry]) -> usize {
        entries.len() * 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vac_core::engine::TaskHistoryEntry;
    use vac_core::task::TaskStatus;
    use uuid::Uuid;

    fn make_entry(desc: &str) -> TaskHistoryEntry {
        TaskHistoryEntry {
            task_id: Uuid::new_v4(),
            description: desc.to_string(),
            status: TaskStatus::Completed,
            updated_at: chrono::Utc::now(),
            total_tokens_used: 100,
            summary: None,
        }
    }

    #[test]
    fn select_prev_from_zero_stays_zero() {
        let mut state = HistoryState::new();
        let entries = vec![make_entry("task1"), make_entry("task2")];
        state.list.select(Some(0));
        state.select_prev(&entries);
        assert_eq!(state.selected(), Some(0));
    }

    #[test]
    fn select_next_from_last_stays_last() {
        let mut state = HistoryState::new();
        let entries = vec![make_entry("task1"), make_entry("task2")];
        state.list.select(Some(1));
        state.select_next(&entries);
        assert_eq!(state.selected(), Some(1));
    }

    #[test]
    fn total_visual_rows_accurate() {
        let state = HistoryState::new();
        let entries = vec![make_entry("task1"), make_entry("task2"), make_entry("task3")];
        assert_eq!(state.total_visual_rows(&entries), 6); // 3 items * 2 rows
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
