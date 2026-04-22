//! Plan tab — view and manage the active plan.

use super::WorkbenchTabView;
use crate::app::{AppState, WorkspaceFocus};
use crate::services::theme::StyleKey;
use crate::ui::style::focus_style;
use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub struct PlanTab;

impl WorkbenchTabView for PlanTab {
    fn tab_label(state: &AppState) -> String {
        match &state.plan.metadata {
            Some(m) => format!("Plan [{}]", m.status),
            None => "Plan".to_string(),
        }
    }

    fn render(f: &mut Frame, state: &mut AppState, area: Rect) {
        let body_text = if state.plan.draft.is_empty() {
            "No plan loaded yet. Run /plan to create one or /plan-review to inspect an existing plan.".to_string()
        } else {
            crate::services::plan::extract_plan_body(&state.plan.draft).to_string()
        };

        let mut lines: Vec<Line> = Vec::new();
        if let Some(meta) = &state.plan.metadata {
            lines.push(Line::from(vec![
                Span::styled("Title: ", state.theme.style(StyleKey::Muted)),
                Span::styled(
                    meta.title.clone(),
                    state
                        .theme
                        .style(StyleKey::Warning)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            let (status_label, status_key) = match meta.status {
                crate::services::plan::PlanStatus::Drafting => ("drafting", StyleKey::Warning),
                crate::services::plan::PlanStatus::PendingReview => {
                    ("pending_review", StyleKey::Accent)
                }
                crate::services::plan::PlanStatus::Approved => ("approved", StyleKey::Success),
            };
            lines.push(Line::from(vec![
                Span::styled("Status: ", state.theme.style(StyleKey::Muted)),
                Span::styled(status_label.to_string(), state.theme.style(status_key)),
                Span::styled(
                    format!("  v{}", meta.version),
                    state.theme.style(StyleKey::Muted),
                ),
            ]));
            lines.push(Line::raw(""));
        }
        for line in body_text.lines() {
            lines.push(Line::raw(line.to_string()));
        }
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "  e: edit in $EDITOR | a: approve | r: request changes | /plan-review: overlay",
            state.theme.style(StyleKey::Muted),
        )));

        let focus_style = focus_style(state.focus == WorkspaceFocus::Workbench, &state.theme);

        let para = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(Span::styled("Plan", focus_style)),
            )
            .wrap(Wrap { trim: false });
        f.render_widget(para, area);
    }
}
