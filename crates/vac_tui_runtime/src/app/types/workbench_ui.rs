use ratatui::layout::Rect;

use super::WorkbenchTab;

/// Render-chrome regions owned by the workbench. Rebuilt each frame;
/// grouped here so callers don't pollute AppState with per-row coordinate
/// vectors.
#[derive(Debug, Clone, Default)]
pub struct WorkbenchChromeState {
    pub tab_regions: Vec<(WorkbenchTab, Rect)>,
    pub task_tray_row_regions: Vec<Rect>,
    pub review_file_row_regions: Vec<(String, Rect)>,
    pub approvals_row_regions: Vec<(usize, Rect)>,
    pub vil_issue_row_regions: Vec<(usize, Rect)>,
    pub sessions_row_regions: Vec<(usize, Rect)>,
    pub body_region: Option<Rect>,
}
