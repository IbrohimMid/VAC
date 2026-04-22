use super::*;
use crate::app::{AppState, InputEvent, WorkbenchTab, WorkspaceFocus};
use crate::app::types::support::SidePanelSection;
use crate::overlay::OverlayId;
use ratatui::layout::Rect;
use tokio::sync::mpsc;

fn make_state_with_channel() -> (
    AppState,
    mpsc::Sender<OutputEvent>,
    mpsc::Receiver<OutputEvent>,
) {
    let state = AppState::default();
    let (tx, rx) = mpsc::channel::<OutputEvent>(64);
    (state, tx, rx)
}

// ── existing coverage ──────────────────────────────────────────────────────

#[test]
fn mouse_click_on_tab_switches() {
    let (mut state, tx, _rx) = make_state_with_channel();
    state.workbench_tab = WorkbenchTab::Approvals;
    state.focus = WorkspaceFocus::Input;
    state
        .workbench_chrome.tab_regions
        .push((WorkbenchTab::Review, Rect::new(2, 0, 6, 1)));
    state
        .workbench_chrome.tab_regions
        .push((WorkbenchTab::Sessions, Rect::new(12, 0, 9, 1)));

    let handled = dispatch_click(&mut state, &tx, 14, 0);
    assert!(handled, "click on Sessions tab region should be handled");
    assert_eq!(state.workbench_tab, WorkbenchTab::Sessions);
    assert_eq!(state.focus, WorkspaceFocus::Workbench);
}

#[test]
fn mouse_click_on_tray_focuses() {
    let (mut state, tx, _rx) = make_state_with_channel();
    state
        .overlay_manager
        .push(OverlayId::TaskTray, WorkspaceFocus::Input);
    state.task_tray_selected = 0;
    state.workbench_chrome.task_tray_row_regions.push(Rect::new(10, 20, 40, 1));
    state.workbench_chrome.task_tray_row_regions.push(Rect::new(10, 21, 40, 1));

    let handled = dispatch_click(&mut state, &tx, 15, 21);
    assert!(handled, "click on tray row 2 should be handled");
    assert_eq!(state.task_tray_selected, 1);
}

#[test]
fn mouse_click_on_banner_dismisses() {
    use crate::handlers::input_core::handle_input_event;
    use crate::services::banner::{BannerMessage, BannerStyle};

    let (mut state, tx, _rx) = make_state_with_channel();
    state.banner.message = Some(BannerMessage::persistent("hello", BannerStyle::Info));
    state.banner.dismiss_region = Some(Rect::new(10, 0, 3, 1));

    handle_input_event(&mut state, &tx, InputEvent::MouseDragStart(11, 0));

    assert!(state.banner.message.is_none(), "banner should be dismissed");
    assert!(state.banner.dismiss_region.is_none());
    assert!(state.banner.click_regions.is_empty());
}

#[test]
fn mouse_click_outside_regions_is_ignored() {
    let (mut state, tx, _rx) = make_state_with_channel();
    let before_tab = state.workbench_tab;
    state
        .workbench_chrome.tab_regions
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
        .workbench_chrome.approvals_row_regions
        .push((0, Rect::new(4, 10, 30, 1)));
    state
        .workbench_chrome.approvals_row_regions
        .push((1, Rect::new(4, 11, 30, 1)));

    let handled = dispatch_click(&mut state, &tx, 10, 11);
    assert!(handled, "approvals row 1 click must dispatch");
    assert_eq!(state.approvals.approval_selected_idx, 1);
    assert_eq!(state.workbench_tab, WorkbenchTab::Approvals);
    assert_eq!(state.focus, WorkspaceFocus::Workbench);
}

#[test]
fn mouse_click_on_review_row_selects_path_and_focuses() {
    let (mut state, tx, _rx) = make_state_with_channel();
    state.focus = WorkspaceFocus::Input;
    state.workbench_tab = WorkbenchTab::Approvals;
    state.review.filter.clear();
    state.review.items.insert(
        "src/lib.rs".to_string(),
        crate::app::ReviewItem {
            path: "src/lib.rs".to_string(),
            status: crate::app::ReviewItemStatus::Pending,
            has_snapshot: false,
            last_error: None,
            dirty_generation: 0,
        },
    );
    state.review.items.insert(
        "src/main.rs".to_string(),
        crate::app::ReviewItem {
            path: "src/main.rs".to_string(),
            status: crate::app::ReviewItemStatus::Pending,
            has_snapshot: false,
            last_error: None,
            dirty_generation: 0,
        },
    );
    state
        .workbench_chrome.review_file_row_regions
        .push(("src/main.rs".to_string(), Rect::new(4, 11, 30, 1)));

    let handled = dispatch_click(&mut state, &tx, 10, 11);
    assert!(handled, "review row click must dispatch");
    assert_eq!(state.review.selected_path.as_deref(), Some("src/main.rs"));
    assert_eq!(state.workbench_tab, WorkbenchTab::Review);
    assert_eq!(state.focus, WorkspaceFocus::Workbench);
}

#[test]
fn mouse_click_on_vil_issue_row_selects_and_focuses() {
    let (mut state, tx, _rx) = make_state_with_channel();
    state.focus = WorkspaceFocus::Input;
    state.workbench_tab = WorkbenchTab::Review;
    state
        .workbench_chrome.vil_issue_row_regions
        .push((2, Rect::new(2, 20, 50, 1)));

    let handled = dispatch_click(&mut state, &tx, 5, 20);
    assert!(handled, "vil issue row click must dispatch");
    assert_eq!(state.vil.workbench_selected, 2);
    assert_eq!(state.workbench_tab, WorkbenchTab::Vil);
    assert_eq!(state.focus, WorkspaceFocus::Workbench);
}

#[test]
fn mouse_click_on_sessions_row_selects_and_focuses() {
    let (mut state, tx, _rx) = make_state_with_channel();
    state.focus = WorkspaceFocus::Input;
    state.workbench_tab = WorkbenchTab::Review;
    state.sessions = vec![
        crate::app::SessionInfo {
            title: "A".to_string(),
            id: "session-a".to_string(),
            updated_at: "now".to_string(),
            checkpoints: Vec::new(),
            task_count: 0,
            last_activity: "first".to_string(),
            has_checkpoint: false,
            snapshot_present: false,
            snapshot_stale: false,
        },
        crate::app::SessionInfo {
            title: "B".to_string(),
            id: "session-b".to_string(),
            updated_at: "now".to_string(),
            checkpoints: Vec::new(),
            task_count: 1,
            last_activity: "second".to_string(),
            has_checkpoint: false,
            snapshot_present: false,
            snapshot_stale: false,
        },
    ];
    state
        .workbench_chrome.sessions_row_regions
        .push((1, Rect::new(2, 18, 40, 1)));

    let handled = dispatch_click(&mut state, &tx, 5, 18);
    assert!(handled, "sessions row click must dispatch");
    assert_eq!(state.sessions_selected_idx, 1);
    assert_eq!(state.workbench_tab, WorkbenchTab::Sessions);
    assert_eq!(state.focus, WorkspaceFocus::Workbench);
}

#[test]
fn mouse_click_on_workbench_body_grabs_focus() {
    let (mut state, tx, _rx) = make_state_with_channel();
    state.focus = WorkspaceFocus::Input;
    state.workbench_chrome.body_region = Some(Rect::new(0, 5, 80, 20));

    let handled = dispatch_click(&mut state, &tx, 40, 15);
    assert!(handled, "workbench body click must grab focus");
    assert_eq!(state.focus, WorkspaceFocus::Workbench);
}

#[test]
fn dispatch_hover_short_circuits_on_sticky_row_regardless_of_hover_presence() {
    let (mut state, _tx, _rx) = make_state_with_channel();
    state
        .workbench_chrome.vil_issue_row_regions
        .push((7, Rect::new(2, 20, 50, 1)));

    let first = dispatch_hover(&mut state, 5, 20);
    assert!(
        !first,
        "first probe over an empty row must not request a repaint"
    );
    assert_eq!(state.active_hover_row_idx, Some(7));
    assert!(state.active_hover.is_none());

    let second = dispatch_hover(&mut state, 40, 20);
    assert!(
        !second,
        "sticky-row probe must short-circuit and skip repaint"
    );
    assert_eq!(
        state.active_hover_row_idx,
        Some(7),
        "sticky row must remain recorded"
    );

    let off = dispatch_hover(&mut state, 200, 200);
    assert!(
        !off,
        "off-region move with no prior popup must not request repaint"
    );
    assert_eq!(
        state.active_hover_row_idx, None,
        "leaving all tracked regions must clear the sticky-row cache"
    );
}

#[test]
fn mouse_click_row_region_wins_over_body_fallback() {
    let (mut state, tx, _rx) = make_state_with_channel();
    state.focus = WorkspaceFocus::Input;
    state.workbench_tab = WorkbenchTab::Review;
    state.workbench_chrome.body_region = Some(Rect::new(0, 5, 80, 20));
    state
        .workbench_chrome.vil_issue_row_regions
        .push((0, Rect::new(2, 10, 20, 1)));

    let handled = dispatch_click(&mut state, &tx, 5, 10);
    assert!(handled);
    assert_eq!(state.workbench_tab, WorkbenchTab::Vil);
    assert_eq!(state.vil.workbench_selected, 0);
}

// ── Task-5 new coverage (5 exact names required by plan) ──────────────────

#[test]
fn dispatch_click_selects_review_row_at_position() {
    let (mut state, tx, _rx) = make_state_with_channel();
    state.focus = WorkspaceFocus::Input;
    state.review.filter.clear();
    state.review.items.insert(
        "a.rs".to_string(),
        crate::app::ReviewItem {
            path: "a.rs".to_string(),
            status: crate::app::ReviewItemStatus::Pending,
            has_snapshot: false,
            last_error: None,
            dirty_generation: 0,
        },
    );
    state
        .workbench_chrome.review_file_row_regions
        .push(("a.rs".to_string(), Rect::new(0, 5, 40, 1)));

    let handled = dispatch_click(&mut state, &tx, 10, 5);
    assert!(handled);
    assert_eq!(state.review.selected_path.as_deref(), Some("a.rs"));
    assert_eq!(state.workbench_tab, WorkbenchTab::Review);
}

#[test]
fn dispatch_click_switches_side_panel_tab() {
    let (mut state, tx, _rx) = make_state_with_channel();
    assert!(
        !state
            .side_panel.section_collapsed
            .contains(&SidePanelSection::Context),
        "Context section should start expanded"
    );
    state
        .side_panel.header_areas
        .insert(SidePanelSection::Context, Rect::new(0, 2, 30, 1));

    let handled = dispatch_click(&mut state, &tx, 5, 2);
    assert!(handled, "click on side panel header must be handled");
    assert!(
        state
            .side_panel.section_collapsed
            .contains(&SidePanelSection::Context),
        "Context section should be collapsed after click"
    );

    // Second click toggles back to expanded.
    let handled2 = dispatch_click(&mut state, &tx, 5, 2);
    assert!(handled2);
    assert!(
        !state
            .side_panel.section_collapsed
            .contains(&SidePanelSection::Context),
        "Context section should be expanded after second click"
    );
}

#[test]
fn dispatch_click_triggers_approval_action() {
    let (mut state, tx, _rx) = make_state_with_channel();
    state.focus = WorkspaceFocus::Input;
    state
        .workbench_chrome.approvals_row_regions
        .push((0, Rect::new(0, 10, 20, 1)));
    state
        .workbench_chrome.approvals_row_regions
        .push((1, Rect::new(0, 11, 20, 1)));

    let handled = dispatch_click(&mut state, &tx, 5, 10);
    assert!(handled, "click on approval row 0 must dispatch");
    assert_eq!(state.approvals.approval_selected_idx, 0);
    assert_eq!(state.workbench_tab, WorkbenchTab::Approvals);

    let handled2 = dispatch_click(&mut state, &tx, 5, 11);
    assert!(handled2);
    assert_eq!(state.approvals.approval_selected_idx, 1);
}

#[test]
fn dispatch_click_scrolls_task_tray_overlay_on_hit() {
    let (mut state, tx, _rx) = make_state_with_channel();
    state
        .overlay_manager
        .push(OverlayId::TaskTray, WorkspaceFocus::Input);
    state.task_tray_selected = 0;
    for row_y in 5u16..10 {
        state
            .workbench_chrome.task_tray_row_regions
            .push(Rect::new(0, row_y, 40, 1));
    }

    let handled = dispatch_click(&mut state, &tx, 5, 8);
    assert!(handled, "click on tray row must be handled");
    // row index 3 (y=5→0, y=6→1, y=7→2, y=8→3)
    assert_eq!(state.task_tray_selected, 3);
}

#[test]
fn dispatch_click_on_workbench_tab_changes_view() {
    let (mut state, tx, _rx) = make_state_with_channel();
    state.workbench_tab = WorkbenchTab::Sessions;
    state.focus = WorkspaceFocus::Input;
    state
        .workbench_chrome.tab_regions
        .push((WorkbenchTab::Review, Rect::new(0, 0, 8, 1)));
    state
        .workbench_chrome.tab_regions
        .push((WorkbenchTab::Approvals, Rect::new(9, 0, 10, 1)));

    let handled = dispatch_click(&mut state, &tx, 12, 0);
    assert!(handled, "click on Approvals tab must dispatch");
    assert_eq!(state.workbench_tab, WorkbenchTab::Approvals);
    assert_eq!(state.focus, WorkspaceFocus::Workbench);
}
