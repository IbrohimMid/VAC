//! Mouse click dispatcher (PR-T16).
//!
//! Consumes click regions populated during the view render pass and maps
//! left-button clicks to the appropriate state transition. Banner dismissal
//! and side-panel row clicks continue to live in
//! [`input_core::handle_mouse_drag_start`] for backward compatibility; this
//! module only owns the *new* regions (workbench tabs + task tray rows) plus
//! the integration point invoked from `input_core` once banner/side-panel
//! checks have already run.

use ratatui::layout::Rect;
use tokio::sync::mpsc::Sender;

use crate::app::{AppState, OutputEvent, WorkspaceFocus};

/// Dispatch a left-button click. Returns `true` when the click matched a
/// tracked region and state was updated; callers should then stop further
/// mouse routing (e.g. text-selection drag start).
pub fn dispatch_click(
    state: &mut AppState,
    _output_tx: &Sender<OutputEvent>,
    col: u16,
    row: u16,
) -> bool {
    // Workbench tab strip.
    let tab_hit = state
        .workbench_tab_regions
        .iter()
        .find(|(_, rect)| hit(rect, col, row))
        .map(|(tab, _)| *tab);
    if let Some(tab) = tab_hit {
        state.workbench_tab = tab;
        state.focus = WorkspaceFocus::Workbench;
        return true;
    }

    // Task tray overlay rows.
    if state
        .overlay_manager
        .is_active(crate::overlay::OverlayId::TaskTray)
    {
        let tray_hit = state
            .task_tray_row_regions
            .iter()
            .position(|rect| hit(rect, col, row));
        if let Some(idx) = tray_hit {
            state.task_tray_selected = idx;
            return true;
        }
    }

    // PR-T16 P1 — Review pane file rows. Clicking a row selects that path
    // and focuses the workbench so the diff body becomes visible.
    let review_hit = state
        .review_file_row_regions
        .iter()
        .find(|(_, rect)| hit(rect, col, row))
        .map(|(path, _)| path.clone());
    if let Some(path) = review_hit {
        let idx = state
            .review_filtered_paths()
            .iter()
            .position(|p| *p == path)
            .unwrap_or(0);
        state.review.selected_idx = idx;
        state.review.selected_path = Some(path);
        state.focus = WorkspaceFocus::Workbench;
        state.workbench_tab = crate::app::WorkbenchTab::Review;
        return true;
    }

    // PR-T16 P1 — Approvals pane rows.
    let approval_hit = state
        .approvals_row_regions
        .iter()
        .find(|(_, rect)| hit(rect, col, row))
        .map(|(idx, _)| *idx);
    if let Some(idx) = approval_hit {
        state.approval_selected_idx = idx;
        state.focus = WorkspaceFocus::Workbench;
        state.workbench_tab = crate::app::WorkbenchTab::Approvals;
        return true;
    }

    // PR-T16 P1 — vil_workbench issue rows.
    let vil_hit = state
        .vil_issue_row_regions
        .iter()
        .find(|(_, rect)| hit(rect, col, row))
        .map(|(idx, _)| *idx);
    if let Some(idx) = vil_hit {
        state.vil.workbench_selected = idx;
        state.focus = WorkspaceFocus::Workbench;
        state.workbench_tab = crate::app::WorkbenchTab::Vil;
        return true;
    }

    // PR-T16 P1 — Workbench panel body (focus-grab fallback). Only fires if
    // no more-specific region matched above, so row clicks still win.
    if let Some(rect) = state.workbench_body_region
        && hit(&rect, col, row)
    {
        state.focus = WorkspaceFocus::Workbench;
        return true;
    }

    false
}

#[inline]
fn hit(rect: &Rect, col: u16, row: u16) -> bool {
    col >= rect.x
        && col < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppState, InputEvent, WorkbenchTab, WorkspaceFocus};
    use crate::overlay::OverlayId;
    use ratatui::layout::Rect;
    use tokio::sync::mpsc;

    fn make_state_with_channel() -> (AppState, mpsc::Sender<OutputEvent>, mpsc::Receiver<OutputEvent>)
    {
        let state = AppState::default();
        let (tx, rx) = mpsc::channel::<OutputEvent>(64);
        (state, tx, rx)
    }

    #[test]
    fn mouse_click_on_tab_switches() {
        let (mut state, tx, _rx) = make_state_with_channel();
        // Start on Approvals and focus outside the workbench.
        state.workbench_tab = WorkbenchTab::Approvals;
        state.focus = WorkspaceFocus::Input;
        // Two tabs with deterministic rects: Review (x=2..=7) and Sessions (x=12..=20).
        state
            .workbench_tab_regions
            .push((WorkbenchTab::Review, Rect::new(2, 0, 6, 1)));
        state
            .workbench_tab_regions
            .push((WorkbenchTab::Sessions, Rect::new(12, 0, 9, 1)));

        let handled = dispatch_click(&mut state, &tx, 14, 0);
        assert!(handled, "click on Sessions tab region should be handled");
        assert_eq!(state.workbench_tab, WorkbenchTab::Sessions);
        assert_eq!(state.focus, WorkspaceFocus::Workbench);
    }

    #[test]
    fn mouse_click_on_tray_focuses() {
        let (mut state, tx, _rx) = make_state_with_channel();
        // Simulate an open task tray overlay with two rows tracked.
        state
            .overlay_manager
            .push(OverlayId::TaskTray, WorkspaceFocus::Input);
        state.task_tray_selected = 0;
        state
            .task_tray_row_regions
            .push(Rect::new(10, 20, 40, 1));
        state
            .task_tray_row_regions
            .push(Rect::new(10, 21, 40, 1));

        let handled = dispatch_click(&mut state, &tx, 15, 21);
        assert!(handled, "click on tray row 2 should be handled");
        assert_eq!(state.task_tray_selected, 1);
    }

    #[test]
    fn mouse_click_on_banner_dismisses() {
        use crate::handlers::input_core::handle_input_event;
        use crate::services::banner::{BannerMessage, BannerStyle};

        let (mut state, tx, _rx) = make_state_with_channel();
        // Seed a banner with a dismiss region. The banner dispatcher lives in
        // input_core; this test guards that MouseDragStart still routes through
        // the banner path and clears banner state on hit.
        state.banner_message = Some(BannerMessage::persistent("hello", BannerStyle::Info));
        state.banner_dismiss_region = Some(Rect::new(10, 0, 3, 1));

        handle_input_event(&mut state, &tx, InputEvent::MouseDragStart(11, 0));

        assert!(state.banner_message.is_none(), "banner should be dismissed");
        assert!(state.banner_dismiss_region.is_none());
        assert!(state.banner_click_regions.is_empty());
    }

    #[test]
    fn mouse_click_outside_regions_is_ignored() {
        let (mut state, tx, _rx) = make_state_with_channel();
        let before_tab = state.workbench_tab;
        state
            .workbench_tab_regions
            .push((WorkbenchTab::Review, Rect::new(2, 0, 6, 1)));
        let handled = dispatch_click(&mut state, &tx, 100, 100);
        assert!(!handled);
        assert_eq!(state.workbench_tab, before_tab);
    }

    #[test]
    fn mouse_click_on_approvals_row_selects_and_focuses() {
        let (mut state, tx, _rx) = make_state_with_channel();
        state.focus = WorkspaceFocus::Input;
        state.workbench_tab = WorkbenchTab::Review;
        state
            .approvals_row_regions
            .push((0, Rect::new(4, 10, 30, 1)));
        state
            .approvals_row_regions
            .push((1, Rect::new(4, 11, 30, 1)));

        let handled = dispatch_click(&mut state, &tx, 10, 11);
        assert!(handled, "approvals row 1 click must dispatch");
        assert_eq!(state.approval_selected_idx, 1);
        assert_eq!(state.workbench_tab, WorkbenchTab::Approvals);
        assert_eq!(state.focus, WorkspaceFocus::Workbench);
    }

    #[test]
    fn mouse_click_on_vil_issue_row_selects_and_focuses() {
        let (mut state, tx, _rx) = make_state_with_channel();
        state.focus = WorkspaceFocus::Input;
        state.workbench_tab = WorkbenchTab::Review;
        state
            .vil_issue_row_regions
            .push((2, Rect::new(2, 20, 50, 1)));

        let handled = dispatch_click(&mut state, &tx, 5, 20);
        assert!(handled, "vil issue row click must dispatch");
        assert_eq!(state.vil.workbench_selected, 2);
        assert_eq!(state.workbench_tab, WorkbenchTab::Vil);
        assert_eq!(state.focus, WorkspaceFocus::Workbench);
    }

    #[test]
    fn mouse_click_on_workbench_body_grabs_focus() {
        let (mut state, tx, _rx) = make_state_with_channel();
        state.focus = WorkspaceFocus::Input;
        state.workbench_body_region = Some(Rect::new(0, 5, 80, 20));

        let handled = dispatch_click(&mut state, &tx, 40, 15);
        assert!(handled, "workbench body click must grab focus");
        assert_eq!(state.focus, WorkspaceFocus::Workbench);
    }

    #[test]
    fn mouse_click_row_region_wins_over_body_fallback() {
        let (mut state, tx, _rx) = make_state_with_channel();
        state.focus = WorkspaceFocus::Input;
        state.workbench_tab = WorkbenchTab::Review;
        state.workbench_body_region = Some(Rect::new(0, 5, 80, 20));
        state
            .vil_issue_row_regions
            .push((0, Rect::new(2, 10, 20, 1)));

        let handled = dispatch_click(&mut state, &tx, 5, 10);
        assert!(handled);
        // The specific VIL tab switch must fire, not the generic focus-only fallback.
        assert_eq!(state.workbench_tab, WorkbenchTab::Vil);
        assert_eq!(state.vil.workbench_selected, 0);
    }
}
