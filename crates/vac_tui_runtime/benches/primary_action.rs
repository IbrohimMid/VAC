//! O2 — Criterion microbench for primary-path actions.
//!
//! Runs: review navigation, user-message append, approval-queue push.
//! Target (release build):
//! - review_select_by_delta ≤ 200 ns
//! - add_user_message       ≤ 1 µs
//! - push_activity          ≤ 200 ns
//!
//! Regression signal: any of these hitting 10× target in CI means a
//! hot-path regressed — usually new filesystem or allocation work.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use vac_tui_runtime::app::{ActivityKind, AppState};

fn bench_review_select_by_delta(c: &mut Criterion) {
    let mut state = AppState::default();
    c.bench_function("primary_action::review_select_by_delta", |b| {
        b.iter(|| {
            state.review_select_by_delta(black_box(1));
            state.review_select_by_delta(black_box(-1));
        });
    });
}

fn bench_add_user_message(c: &mut Criterion) {
    let mut state = AppState::default();
    c.bench_function("primary_action::add_user_message", |b| {
        b.iter(|| {
            state.add_user_message(black_box("hello".to_string()));
        });
    });
}

fn bench_push_activity(c: &mut Criterion) {
    let mut state = AppState::default();
    c.bench_function("primary_action::push_activity", |b| {
        b.iter(|| {
            state.push_activity(black_box(ActivityKind::Status), black_box("tick"));
        });
    });
}

criterion_group!(
    benches,
    bench_review_select_by_delta,
    bench_add_user_message,
    bench_push_activity
);
criterion_main!(benches);
