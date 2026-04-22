//! J2 — Primary-action latency guards.
//!
//! Claude-Code-friction benchmark: the primary actions an operator
//! performs (approve/reject a tool, navigate a file list, add a user
//! message) must complete in sub-millisecond time. This is a regression
//! guard, not a hard latency guarantee — if a future change adds
//! filesystem I/O or blocking work to one of these paths, the timing
//! jumps and the test fails loudly.

use std::time::Instant;

use vac_tui_runtime::app::AppState;

/// 10k iterations of review navigation must stay under 200ms (~20µs/op).
#[test]
fn review_select_by_delta_hot_path_under_200ms_for_10k_iterations() {
    let mut state = AppState::default();
    let start = Instant::now();
    for i in 0..10_000 {
        state.review_select_by_delta(if i % 2 == 0 { 1 } else { -1 });
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 200,
        "10k review_select_by_delta took {}ms (budget: 200ms)",
        elapsed.as_millis()
    );
}

/// 1k iterations of add_user_message must stay under 50ms.
#[test]
fn add_user_message_hot_path_under_50ms_for_1k_iterations() {
    let mut state = AppState::default();
    let start = Instant::now();
    for i in 0..1_000 {
        state.add_user_message(format!("message-{i}"));
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 50,
        "1k add_user_message took {}ms (budget: 50ms)",
        elapsed.as_millis()
    );
}

/// push_activity ring-capped at 500 entries; 10k pushes must stay under 100ms.
#[test]
fn push_activity_ring_under_100ms_for_10k_pushes() {
    let mut state = AppState::default();
    let start = Instant::now();
    for i in 0..10_000 {
        state.push_activity(
            vac_tui_runtime::app::ActivityKind::Status,
            format!("event-{i}"),
        );
    }
    let elapsed = start.elapsed();
    // Ring-cap means .drain() runs 9500 times; still must be fast.
    assert!(
        elapsed.as_millis() < 100,
        "10k push_activity took {}ms (budget: 100ms)",
        elapsed.as_millis()
    );
    assert!(state.activity.len() <= 500);
}
