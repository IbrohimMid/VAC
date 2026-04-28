//! E2 — Memory workbench tab.
//!
//! Lists the consolidator runs the `auto_dream` job has archived
//! under `.vac/memory/archived/`. The list is sourced from a cached
//! index on `AppState` (see `AppState.workspace.memory_archive`);
//! `read_dir` per render would tie the TUI render thread to the
//! filesystem, so the refresh lives on an idle tick (follow-up
//! commit wires the actual producer — today the cache is seeded
//! once at boot).

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use super::WorkbenchTabView;
use crate::app::AppState;

pub struct MemoryTab;

impl WorkbenchTabView for MemoryTab {
    fn tab_label(state: &AppState) -> String {
        let n = state.workspace.memory_archive.entries.len();
        if n == 0 {
            "Memory".to_string()
        } else {
            format!("Memory ({n})")
        }
    }

    fn render(f: &mut Frame, state: &mut AppState, area: Rect) {
        let archive = &state.workspace.memory_archive;
        if archive.entries.is_empty() {
            let msg = format!(
                "no consolidator runs cached yet\n\nsource: {}\n\nthe cache is refreshed by the auto-dream idle tick — run `/dream` or wait for the next idle window.",
                archive.source.display(),
            );
            let para = Paragraph::new(msg).wrap(Wrap { trim: false }).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Memory archive"),
            );
            f.render_widget(para, area);
            return;
        }

        let items: Vec<ListItem> = archive
            .entries
            .iter()
            .map(|e| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{:>10}  ", e.timestamp),
                        Style::default().add_modifier(Modifier::DIM),
                    ),
                    Span::raw(e.name.clone()),
                    Span::styled(
                        format!("  ({} bytes)", e.size_bytes),
                        Style::default().add_modifier(Modifier::DIM),
                    ),
                ]))
            })
            .collect();
        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Memory archive — {}", archive.source.display())),
        );
        f.render_widget(list, area);
    }
}
