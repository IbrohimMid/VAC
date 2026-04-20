//! VIL tab — validation issues, audit results, and repair proposals.

use crate::app::AppState;
use ratatui::{Frame, layout::Rect};
use super::WorkbenchTabView;

pub struct VilTab;

impl WorkbenchTabView for VilTab {
    fn tab_label(state: &AppState) -> String {
        format!("VIL ({})", state.vil.status.validation_issues.len())
    }

    fn render(f: &mut Frame, state: &mut AppState, area: Rect) {
        crate::services::vil_workbench::render(f, state, area);
    }
}
