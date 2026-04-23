//! J3 — Review workbench keyboard-only navigation contract.
//!
//! Enforces the Claude-Code-friction rule: every workbench action must
//! be reachable via keyboard. These tests drive `AppState::review_select_by_delta`
//! directly (the integer the keyboard handler feeds it) and assert
//! selection moves as expected without any mouse event.

use vac_tui_runtime::app::{AppState, ReviewItem, ReviewItemStatus};

fn seeded_state() -> AppState {
    let mut state = AppState::default();
    for i in 0..5 {
        let path = format!("src/file_{i}.rs");
        state.workspace.review.items.insert(
            path.clone(),
            ReviewItem {
                path,
                status: ReviewItemStatus::Pending,
                has_snapshot: false,
                last_error: None,
                dirty_generation: 0,
            },
        );
    }
    state.workspace.review.selected_idx = 0;
    state.workspace.review.selected_path = Some("src/file_0.rs".to_string());
    state
}

#[test]
fn select_by_delta_moves_down_with_j_like_input() {
    let mut state = seeded_state();
    state.review_select_by_delta(1);
    assert_eq!(state.workspace.review.selected_idx, 1);
    assert_eq!(state.workspace.review.selected_path.as_deref(), Some("src/file_1.rs"));
}

#[test]
fn select_by_delta_moves_up_with_k_like_input() {
    let mut state = seeded_state();
    state.workspace.review.selected_idx = 3;
    state.workspace.review.selected_path = Some("src/file_3.rs".to_string());
    state.review_select_by_delta(-1);
    assert_eq!(state.workspace.review.selected_idx, 2);
}

#[test]
fn select_by_delta_clamps_at_start() {
    let mut state = seeded_state();
    // Already at start: -1 stays at 0.
    state.review_select_by_delta(-1);
    assert_eq!(state.workspace.review.selected_idx, 0);
}

#[test]
fn select_by_delta_clamps_at_end() {
    let mut state = seeded_state();
    // 5 items, idx 4 is last; +1 stays at 4.
    state.workspace.review.selected_idx = 4;
    state.workspace.review.selected_path = Some("src/file_4.rs".to_string());
    state.review_select_by_delta(1);
    assert_eq!(state.workspace.review.selected_idx, 4);
}

#[test]
fn select_by_delta_jumps_five_files_keyboard_only() {
    // Simulates 5 j presses: no mouse, pure delta.
    let mut state = seeded_state();
    for _ in 0..5 {
        state.review_select_by_delta(1);
    }
    // Clamps at last (idx 4, 5 items).
    assert_eq!(state.workspace.review.selected_idx, 4);
    assert_eq!(state.workspace.review.selected_path.as_deref(), Some("src/file_4.rs"));
}
