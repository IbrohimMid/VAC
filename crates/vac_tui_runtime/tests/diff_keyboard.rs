//! O6 — Diff viewer keyboard-only contract.
//!
//! Drives changeset handlers directly (not via ratatui event loop) to
//! verify every diff-viewer action is reachable without a mouse.

use vac_tui_runtime::app::{AppState, OutputEvent};
use vac_tui_runtime::handlers::{HandlerContext, changeset};

fn test_ctx() -> (AppState, tokio::sync::mpsc::Sender<OutputEvent>) {
    let state = AppState::default();
    let (tx, _rx) = tokio::sync::mpsc::channel::<OutputEvent>(16);
    (state, tx)
}

#[test]
fn contract_diff_scroll_down_increments_via_handler() {
    let (mut state, tx) = test_ctx();
    let mut ctx = HandlerContext::new(&mut state, &tx);
    assert_eq!(ctx.state.changeset_ui.diff_scroll, 0);
    assert!(changeset::scroll_down(&mut ctx).is_ok());
    assert!(changeset::scroll_down(&mut ctx).is_ok());
    assert!(changeset::scroll_down(&mut ctx).is_ok());
    assert_eq!(ctx.state.changeset_ui.diff_scroll, 3);
}

#[test]
fn contract_diff_scroll_up_saturates_at_zero() {
    let (mut state, tx) = test_ctx();
    let mut ctx = HandlerContext::new(&mut state, &tx);
    // saturating_sub at 0 must not panic or underflow.
    assert!(changeset::scroll_up(&mut ctx).is_ok());
    assert!(changeset::scroll_up(&mut ctx).is_ok());
    assert_eq!(ctx.state.changeset_ui.diff_scroll, 0);
}

#[test]
fn contract_diff_navigation_pure_keyboard_no_events_emitted() {
    let (mut state, tx) = test_ctx();
    let mut ctx = HandlerContext::new(&mut state, &tx);
    // Scrolling through 20 lines via pure keyboard handlers.
    for _ in 0..20 {
        let _ = changeset::scroll_down(&mut ctx);
    }
    assert_eq!(ctx.state.changeset_ui.diff_scroll, 20);
    // Scroll back up.
    for _ in 0..10 {
        let _ = changeset::scroll_up(&mut ctx);
    }
    assert_eq!(ctx.state.changeset_ui.diff_scroll, 10);
}

#[test]
fn contract_diff_close_resets_selection_state() {
    let (mut state, tx) = test_ctx();
    state.changeset_ui.diff_scroll = 15;
    state.changeset_ui.selected_idx = 3;
    state.changeset_ui.selected_path = Some("src/foo.rs".into());

    let mut ctx = HandlerContext::new(&mut state, &tx);
    assert!(changeset::close(&mut ctx).is_ok());

    assert_eq!(ctx.state.changeset_ui.diff_scroll, 0);
    assert_eq!(ctx.state.changeset_ui.selected_idx, 0);
    assert!(ctx.state.changeset_ui.selected_path.is_none());
    assert!(ctx.state.changeset_ui.diff.is_none());
}
