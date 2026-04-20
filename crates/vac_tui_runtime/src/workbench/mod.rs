//! Workbench tab views — one implementor per WorkbenchTab variant.
//!
//! Each tab provides:
//!   - `tab_label(state)` → display string for the tab bar
//!   - `render(f, state, area)` → content rendering (list on left, detail on right)

pub mod agents;
pub mod approvals;
pub mod plan;
pub mod review;
pub mod runtime;
pub mod sessions;
pub mod vil;

pub use agents::AgentsTab;
pub use approvals::ApprovalsTab;
pub use plan::PlanTab;
pub use review::ReviewTab;
pub use runtime::RuntimeTab;
pub use sessions::SessionsTab;
pub use vil::VilTab;

use crate::app::{AppState, WorkbenchTab};
use ratatui::{Frame, layout::Rect};

// ── Trait ────────────────────────────────────────────────────────────────────

pub trait WorkbenchTabView {
    fn tab_label(state: &AppState) -> String;
    fn render(f: &mut Frame, state: &mut AppState, area: Rect);
}

// ── Dispatch helpers (called from view.rs) ───────────────────────────────────

/// Returns ordered tab label strings for the tab bar.
pub fn tab_labels(state: &AppState) -> Vec<String> {
    vec![
        ApprovalsTab::tab_label(state),
        ReviewTab::tab_label(state),
        SessionsTab::tab_label(state),
        AgentsTab::tab_label(state),
        RuntimeTab::tab_label(state),
        PlanTab::tab_label(state),
        VilTab::tab_label(state),
    ]
}

/// Returns the 0-based index of the given tab (matches tab bar order).
pub fn active_tab_index(tab: &WorkbenchTab) -> usize {
    match tab {
        WorkbenchTab::Approvals => 0,
        WorkbenchTab::Review => 1,
        WorkbenchTab::Sessions => 2,
        WorkbenchTab::Agents => 3,
        WorkbenchTab::Runtime => 4,
        WorkbenchTab::Plan => 5,
        WorkbenchTab::Vil => 6,
    }
}

/// Inverse of `active_tab_index` — restores a tab from a saved index.
pub fn tab_from_index(index: usize) -> WorkbenchTab {
    match index {
        1 => WorkbenchTab::Review,
        2 => WorkbenchTab::Sessions,
        3 => WorkbenchTab::Agents,
        4 => WorkbenchTab::Runtime,
        5 => WorkbenchTab::Plan,
        6 => WorkbenchTab::Vil,
        _ => WorkbenchTab::Approvals,
    }
}

/// Dispatches rendering to the currently active tab.
pub fn render_active_tab(f: &mut Frame, state: &mut AppState, area: Rect) {
    match state.workbench_tab {
        WorkbenchTab::Approvals => ApprovalsTab::render(f, state, area),
        WorkbenchTab::Review => ReviewTab::render(f, state, area),
        WorkbenchTab::Sessions => SessionsTab::render(f, state, area),
        WorkbenchTab::Agents => AgentsTab::render(f, state, area),
        WorkbenchTab::Runtime => RuntimeTab::render(f, state, area),
        WorkbenchTab::Plan => PlanTab::render(f, state, area),
        WorkbenchTab::Vil => VilTab::render(f, state, area),
    }
}
