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
    if state.layout.lsp_ui.active_hover.is_some() {
        let outside = state
            .layout
            .lsp_ui
            .hover_popup_region
            .is_none_or(|r| !hit(&r, col, row));
        if outside {
            state.layout.lsp_ui.active_hover = None;
            state.layout.lsp_ui.hover_popup_region = None;
            state.layout.lsp_ui.active_hover_row_idx = None;
            return true;
        }
    }

    // Workbench tab strip.
    let tab_hit = state
        .layout
        .workbench_chrome
        .tab_regions
        .iter()
        .find(|(_, rect)| hit(rect, col, row))
        .map(|(tab, _)| *tab);
    if let Some(tab) = tab_hit {
        state.layout.workbench_tab = tab;
        state.layout.focus = WorkspaceFocus::Workbench;
        return true;
    }

    // Task tray overlay rows.
    if state
        .layout
        .overlay_manager
        .is_active(crate::overlay::OverlayId::TaskTray)
    {
        let tray_hit = state
            .layout
            .workbench_chrome
            .task_tray_row_regions
            .iter()
            .position(|rect| hit(rect, col, row));
        if let Some(idx) = tray_hit {
            state.execution.task_tray.selected = idx;
            return true;
        }
    }

    // PR-T16 P1 — Review pane file rows. Clicking a row selects that path
    // and focuses the workbench so the diff body becomes visible.
    let review_hit = state
        .layout
        .workbench_chrome
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
        state.workspace.review.selected_idx = idx;
        state.workspace.review.selected_path = Some(path);
        state.layout.focus = WorkspaceFocus::Workbench;
        state.layout.workbench_tab = crate::app::WorkbenchTab::Review;
        return true;
    }

    // PR-T16 P1 — Approvals pane rows.
    let approval_hit = state
        .layout
        .workbench_chrome
        .approvals_row_regions
        .iter()
        .find(|(_, rect)| hit(rect, col, row))
        .map(|(idx, _)| *idx);
    if let Some(idx) = approval_hit {
        state.execution.approvals.approval_selected_idx = idx;
        state.layout.focus = WorkspaceFocus::Workbench;
        state.layout.workbench_tab = crate::app::WorkbenchTab::Approvals;
        return true;
    }

    // PR-T16 P1 — vil_workbench issue rows.
    let vil_hit = state
        .layout
        .workbench_chrome
        .vil_issue_row_regions
        .iter()
        .find(|(_, rect)| hit(rect, col, row))
        .map(|(idx, _)| *idx);
    if let Some(idx) = vil_hit {
        state.vil_domain.vil.workbench_selected = idx;
        state.layout.focus = WorkspaceFocus::Workbench;
        state.layout.workbench_tab = crate::app::WorkbenchTab::Vil;
        // R7 / PR-T15 — try to anchor a hover popup at the clicked issue's
        // (file, line) position against the LSP snapshot. When any piece is
        // missing (no snapshot, issue has no file/line, no intersecting
        // diag), we clear any stale popup instead of surfacing a blank one.
        state.layout.lsp_ui.active_hover = (|| {
            let issue = crate::services::vil_workbench::selected_issue(state)?;
            let file = issue.file.as_ref()?;
            let line_1based = issue.line?;
            let snap = state.layout.lsp_ui.lsp_diagnostics.as_ref()?;
            let line0 = line_1based.saturating_sub(1);
            let line0_u32 = u32::try_from(line0).ok()?;
            crate::services::diagnostics_overlay::hover_detail_at(
                snap,
                std::path::Path::new(file),
                line0_u32,
            )
        })();
        // Renderer will refresh hover_popup_region on the next frame.
        state.layout.lsp_ui.hover_popup_region = None;
        // R7b hot-path cache: click seeds the cache with the selected row
        // so a subsequent MouseMove on the same row short-circuits.
        state.layout.lsp_ui.active_hover_row_idx = if state.layout.lsp_ui.active_hover.is_some() {
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
        .layout
        .workbench_chrome
        .sessions_row_regions
        .iter()
        .find(|(_, rect)| hit(rect, col, row))
        .map(|(idx, _)| *idx);
    if let Some(idx) = sessions_hit
        && idx < state.session.sessions.len()
    {
        state.operator_config.operator.sessions_selected_idx = idx;
        state.layout.focus = WorkspaceFocus::Workbench;
        state.layout.workbench_tab = crate::app::WorkbenchTab::Sessions;
        return true;
    }

    // PR-T16 T5 — Side panel section header areas. Clicking a section
    // header toggles its collapse state, matching keyboard 'c' behaviour.
    let section_hit = state
        .layout
        .side_panel
        .header_areas
        .iter()
        .find(|(_, rect)| hit(rect, col, row))
        .map(|(section, _)| *section);
    if let Some(section) = section_hit {
        if state.layout.side_panel.section_collapsed.contains(&section) {
            state.layout.side_panel.section_collapsed.remove(&section);
        } else {
            state.layout.side_panel.section_collapsed.insert(section);
        }
        return true;
    }

    // PR-T16 P1 — Workbench panel body (focus-grab fallback). Only fires if
    // no more-specific region matched above, so row clicks still win.
    if let Some(rect) = state.layout.workbench_chrome.body_region
        && hit(&rect, col, row)
    {
        state.layout.focus = WorkspaceFocus::Workbench;
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
        .layout
        .workbench_chrome
        .vil_issue_row_regions
        .iter()
        .find(|(_, rect)| hit(rect, col, row))
        .map(|(idx, _)| *idx);

    if let Some(idx) = row_idx {
        // R7b hot-path cache, tightened by the PR-T17 reviewer audit.
        // Short-circuit on *row* identity alone, not on
        // `(row, active_hover.is_some())`. Terminals emit many MouseMove
        // events per second while the pointer sits over one row, and the
        // earlier form only skipped the clone-heavy lookup when the row
        // already had a Some hover — rows with no diagnostic therefore
        // re-ran `issue_at_filtered_index` + `hover_detail_at` (both of
        // which clone strings) on every event. Now we record the last
        // probed row index on every probe, Some or None, so a sticky
        // pointer stays cheap regardless of diagnostic presence.
        if state.layout.lsp_ui.active_hover_row_idx == Some(idx) {
            return false;
        }

        let new_hover = (|| {
            let issue = crate::services::vil_workbench::issue_at_filtered_index(state, idx)?;
            let file = issue.file.as_ref()?;
            let line_1based = issue.line?;
            let snap = state.layout.lsp_ui.lsp_diagnostics.as_ref()?;
            let line0 = line_1based.saturating_sub(1);
            let line0_u32 = u32::try_from(line0).ok()?;
            crate::services::diagnostics_overlay::hover_detail_at(
                snap,
                std::path::Path::new(file),
                line0_u32,
            )
        })();

        let changed = new_hover != state.layout.lsp_ui.active_hover;
        state.layout.lsp_ui.active_hover = new_hover;
        // Always record the probed row, even when the probe yielded None.
        // The short-circuit above depends on this invariant.
        state.layout.lsp_ui.active_hover_row_idx = Some(idx);
        if changed {
            // Renderer will refresh hover_popup_region on the next frame.
            state.layout.lsp_ui.hover_popup_region = None;
        }
        return changed;
    }

    // 2. Over the current popup? Keep it alive.
    if let Some(rect) = state.layout.lsp_ui.hover_popup_region
        && hit(&rect, col, row)
    {
        return false;
    }

    // 3. Outside every tracked region. Dismiss any live popup *and*
    // clear the sticky-row cache so a future re-entry over any row re-runs
    // the lookup exactly once. The cache must be cleared even when no
    // popup was visible (the PR-T17 reviewer audit tightening records
    // `active_hover_row_idx` on every probe, Some or None); otherwise
    // a stale row index could make the short-circuit fire on a
    // genuinely different region's first probe.
    let had_popup = state.layout.lsp_ui.active_hover.is_some()
        || state.layout.lsp_ui.hover_popup_region.is_some();
    let had_sticky_row = state.layout.lsp_ui.active_hover_row_idx.is_some();
    if had_popup || had_sticky_row {
        state.layout.lsp_ui.active_hover = None;
        state.layout.lsp_ui.hover_popup_region = None;
        state.layout.lsp_ui.active_hover_row_idx = None;
        // Only request a repaint when something *visible* changed.
        // Clearing a sticky-row cache that wasn't driving any popup is
        // purely internal bookkeeping and should not churn the frame.
        return had_popup;
    }

    false
}

#[inline]
pub(crate) fn hit(rect: &Rect, col: u16, row: u16) -> bool {
    col >= rect.x
        && col < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
}

#[cfg(test)]
mod tests;
