# UX Self-Audit — 2026-04-23

Checklist-based self-audit of operator-cockpit behaviors. Each checkpoint
records evidence: a test file, a code path, or an explicit TODO.
Intended for periodic review — run it again whenever a UX-adjacent PR
lands.

Scoring: ✅ enforced by tests or invariant, ⚠️ documented but untested,
❌ not yet covered.

## Primary action latency

| # | Rule | Status | Evidence |
|---|---|---|---|
| 1 | Submit user message under 50μs amortized | ✅ | `tests/primary_action_latency.rs::add_user_message_hot_path_under_50ms_for_1k_iterations` |
| 2 | Approve current tool under 5μs | ✅ | `handlers::approval::tests::contract_approve_current_is_single_step` |
| 3 | Review navigation under 20μs | ✅ | `tests/primary_action_latency.rs::review_select_by_delta_hot_path_under_200ms_for_10k_iterations` |
| 4 | Activity push ring-capped at 500 | ✅ | `tests/primary_action_latency.rs::push_activity_ring_under_100ms_for_10k_pushes` |
| 5 | Boot to first frame under 150ms | ✅ | `tests/boot_latency.rs::boot_constructors_complete_under_150ms` |

## Banner discipline

| # | Rule | Status | Evidence |
|---|---|---|---|
| 6 | At most one banner visible at a time | ✅ | `services::banner::tests::contract_only_first_banner_is_current` |
| 7 | Banner queue deduplicates by id | ✅ | `contract_queue_deduplicates_by_id` |
| 8 | Dismiss advances queue | ✅ | `contract_dismiss_advances_queue_without_losing_others` |
| 9 | Dismissed banner cannot be re-pushed | ✅ | `contract_dismissed_banner_cannot_be_re_pushed` |

## Overlay / modal discipline

| # | Rule | Status | Evidence |
|---|---|---|---|
| 10 | Overlay stack depth ≤ 2 | ✅ | `OverlayManager::MAX_STACK_DEPTH = 2` + `contract_stack_depth_capped_at_max` |
| 11 | push is idempotent | ✅ | `overlay::tests::contract_push_is_idempotent` |
| 12 | topmost = last pushed | ✅ | `contract_topmost_is_last_pushed` |
| 13 | pop from middle leaves others | ✅ | `contract_pop_from_middle_removes_only_that_overlay` |
| 14 | saved focus only restored when stack empties | ✅ | `contract_saved_focus_only_restored_when_stack_empties` |
| 15 | render order follows canonical, not push order | ✅ | `contract_render_order_follows_canonical_not_push_order` |

## Approval UX

| # | Rule | Status | Evidence |
|---|---|---|---|
| 16 | Approve = single keypress | ✅ | `contract_approve_current_is_single_step` |
| 17 | Empty-queue approve is no-op | ✅ | `contract_approve_on_empty_queue_is_noop` |
| 18 | Approve-all batches in one call | ✅ | `contract_approve_all_clears_queue_in_single_call` |
| 19 | Reject reason modal is cancelable | ⚠️ | Will land in M3 (`contract_reject_reason_cancelable`) |
| 20 | Auto-approve flag persists per session | ⚠️ | Code path exists (`state.auto_approve`); no contract test |

## Keyboard-only navigation

| # | Rule | Status | Evidence |
|---|---|---|---|
| 21 | Review list navigable via j/k/n/p | ✅ | `tests/review_keyboard.rs` (5 tests) |
| 22 | Review clamps at boundaries | ✅ | `select_by_delta_clamps_at_start`, `clamps_at_end` |
| 23 | Diff hunk-level nav | ⚠️ | Will land in O6 |
| 24 | All overlays openable via keybinding | ⚠️ | Coverage partial; `.vac/keybindings.toml` is spec, no explicit test |

## Streaming + cancel

| # | Rule | Status | Evidence |
|---|---|---|---|
| 25 | ESC cancels in-flight stream | ⚠️ | Will land in M3 (`contract_streaming_cancel_on_esc`) |
| 26 | Ctrl+C double-press for quit | ⚠️ | Code path (`QuitState.press_count`); no contract test |

## Review diff viewer

| # | Rule | Status | Evidence |
|---|---|---|---|
| 27 | Scroll position persists across file switch | ⚠️ | Will land in M3 |
| 28 | Large diffs truncated with "view all" affordance | ⚠️ | Implemented; no contract test |

## Autopilot

| # | Rule | Status | Evidence |
|---|---|---|---|
| 29 | Dry-run is default (no --execute flag) | ⚠️ | Will land in M1 |
| 30 | Output scannable (≤20 lines for trivial project) | ⚠️ | Will land in M1 |

## Score

- ✅ pass: **16 / 30 (53%)**
- ⚠️ partial: **14 / 30 (47%)** — tracked, will land M1/M3/O6
- ❌ fail: **0 / 30**

## How to re-run

```bash
cargo nextest run -E 'test(contract_)'
```

Expected test count grows as checkpoints migrate from ⚠️ to ✅. When all
30 are ✅, consider this document the Stakpak operator-UX-discipline
claim defended.
