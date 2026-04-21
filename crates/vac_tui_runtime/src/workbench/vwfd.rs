//! VWFD tab — inspector for VWFD documents (workflows, triggers, handlers).
//!
//! Delegates rendering to `services::vwfd_inspector`. The tab reads from
//! `state.vwfd_inspector` (a `VwfdInspectorState`) which is populated by
//! commands or the change-set preview pipeline. When no document is loaded
//! the inspector renders an empty placeholder.

use super::WorkbenchTabView;
use crate::app::AppState;
use ratatui::{Frame, layout::Rect};

pub struct VwfdTab;

impl WorkbenchTabView for VwfdTab {
    fn tab_label(state: &AppState) -> String {
        match state.vwfd_inspector.doc.as_ref() {
            Some(doc) => format!("VWFD [{}]", doc.metadata.name),
            None => "VWFD".to_string(),
        }
    }

    fn render(f: &mut Frame, state: &mut AppState, area: Rect) {
        crate::services::vwfd_inspector::render(f, &state.vwfd_inspector, &state.theme, area);
    }
}
