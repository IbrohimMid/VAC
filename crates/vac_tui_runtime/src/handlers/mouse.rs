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
    // R7 / PR-T15 — if a hover popup is on screen, a click outside its
    // rect dismisses the popup and is considered handled (so the click is
    // not also routed to row / tab / body targets underneath). A click
    // inside the popup falls through, so users can still interact with the
    // underlying surface on the next click after reading the detail.
    if state.active_hover.is_some() {
        let outside = state
            .hover_popup_region
            .map_or(true, |r| !hit(&r, col, row));
        if outside {
            state.active_hover = None;
            state.hover_popup_region = None;
            state.active_hover_row_idx = None;
            return true;
        }
    }

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
        // R7 / PR-T15 — try to anchor a hover popup at the clicked issue's
        // (file, line) position against the LSP snapshot. When any piece is
        // missing (no snapshot, issue has no file/line, no intersecting
        // diag), we clear any stale popup instead of surfacing a blank one.
        state.active_hover = (|| {
            let issue = crate::services::vil_workbench::selected_issue(state)?;
            let file = issue.file.as_ref()?;
            let line_1based = issue.line?;
            let snap = state.lsp_diagnostics.as_ref()?;
            let line0 = line_1based.saturating_sub(1);
            let line0_u32 = u32::try_from(line0).ok()?;
            crate::services::diagnostics_overlay::hover_detail_at(
                snap,
                std::path::Path::new(file),
                line0_u32,
            )
        })();
        // Renderer will refresh hover_popup_region on the next frame.
        state.hover_popup_region = None;
        // R7b hot-path cache: click seeds the cache with the selected row
        // so a subsequent MouseMove on the same row short-circuits.
        state.active_hover_row_idx = if state.active_hover.is_some() {
            Some(idx)
        } else {
            None
        };
        return true;
    }

    // PR-T16 R5 — Sessions workbench tab rows. Clicking a session row
    // selects that session index and focuses the workbench, matching
    // the review / approvals / vil ergonomics. Actual resume of the
    // selected session still lives behind the keyboard shortcut ('r')
    // so a stray click cannot trigger a session switch.
    let sessions_hit = state
        .sessions_row_regions
        .iter()
        .find(|(_, rect)| hit(rect, col, row))
        .map(|(idx, _)| *idx);
    if let Some(idx) = sessions_hit
        && idx < state.sessions.len()
    {
        state.sessions_selected_idx = idx;
        state.focus = WorkspaceFocus::Workbench;
        state.workbench_tab = crate::app::WorkbenchTab::Sessions;
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

/// Dispatch a mouse-move (hover) event. Returns `true` when state changed
/// (popup shown / hidden / retargeted) and the frame should repaint.
///
/// R7 / PR-T15 real-hover extension. Semantics:
///   * Over a VIL issue row → populate `active_hover` for that row and
///     reset `hover_popup_region` so the renderer re-anchors next frame.
///     Moving between rows swaps the hover without flicker (a pending-None
///     gap would cause the popup to tear down + rebuild every cell).
///   * Over the current popup rect → keep the hover alive (user is reading).
///   * Anywhere else → clear `active_hover` + `hover_popup_region`.
///
/// Click-based populate in [`dispatch_click`] is intentionally kept as a
/// fallback for terminals that do not emit `MouseEventKind::Moved`.
pub fn dispatch_hover(state: &mut AppState, col: u16, row: u16) -> bool {
    // 1. Over a VIL row?
    let row_idx = state
        .vil_issue_row_regions
        .iter()
        .find(|(_, rect)| hit(rect, col, row))
        .map(|(idx, _)| *idx);

    if let Some(idx) = row_idx {
        // R7b hot-path cache: when the pointer is still over the same
        // filtered row as the current popup, skip `issue_at_filtered_index`
        // and `hover_detail_at` entirely. Both allocate owned `VilIssue` /
        // cloned `String` fields on every call; terminals can emit dozens
        // of MouseMove events per second during a drag so this matters.
        if state.active_hover.is_some() && state.active_hover_row_idx == Some(idx) {
            return false;
        }

        let new_hover = (|| {
            let issue = crate::services::vil_workbench::issue_at_filtered_index(state, idx)?;
            let file = issue.file.as_ref()?;
            let line_1based = issue.line?;
            let snap = state.lsp_diagnostics.as_ref()?;
            let line0 = line_1based.saturating_sub(1);
            let line0_u32 = u32::try_from(line0).ok()?;
            crate::services::diagnostics_overlay::hover_detail_at(
                snap,
                std::path::Path::new(file),
                line0_u32,
            )
        })();

        if new_hover != state.active_hover {
            state.active_hover = new_hover;
            // Renderer will refresh hover_popup_region on the next frame.
            state.hover_popup_region = None;
            state.active_hover_row_idx = if state.active_hover.is_some() {
                Some(idx)
            } else {
                None
            };
            return true;
        }
        // Same row / same detail — refresh the cache anyway (protects
        // against stale keys after click seeding) and skip repaint.
        state.active_hover_row_idx = Some(idx);
        return false;
    }

    // 2. Over the current popup? Keep it alive.
    if let Some(rect) = state.hover_popup_region
        && hit(&rect, col, row)
    {
        return false;
    }

    // 3. Outside every tracked region — dismiss if a hover was up.
    if state.active_hover.is_some() || state.hover_popup_region.is_some() {
        state.active_hover = None;
        state.hover_popup_region = None;
        state.active_hover_row_idx = None;
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
