//! VWFD tab — inspector for VWFD documents (workflows, triggers, handlers).
//!
//! Minimal stub for PR-T11 plumbing. Rendering logic and tree navigation
//! will be added in a follow-up PR once the tab variant is wired through
//! the workbench dispatch and action-context tables.

use super::WorkbenchTabView;
use crate::app::AppState;
use crate::services::theme::StyleKey;
use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub struct VwfdTab;

impl WorkbenchTabView for VwfdTab {
    fn tab_label(_state: &AppState) -> String {
        "VWFD".to_string()
    }

    fn render(f: &mut Frame, state: &mut AppState, area: Rect) {
        let lines = vec![
            Line::from(Span::styled(
                "VWFD Inspector",
                state
                    .theme
                    .style(StyleKey::Warning)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Workflow / trigger / handler inspector — coming soon.",
                state.theme.style(StyleKey::Muted),
            )),
            Line::from(Span::styled(
                "Load a .vwfd.yaml via the change-set preview or the VWFD semantic-diff pipeline.",
                state.theme.style(StyleKey::Muted),
            )),
        ];

        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(
                "VWFD",
                state
                    .theme
                    .style(StyleKey::Warning)
                    .add_modifier(Modifier::BOLD),
            ))
            .border_style(state.theme.style(StyleKey::Muted));
        let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
        f.render_widget(paragraph, area);
    }
}
