# Checklist

- [x] `ContextIndex::search` performs term matching and does not ignore `query` parameter (F-01).
- [x] `ContextIndex::add_entry` seeds the initial score with `attention_weight` (F-06).
- [x] `ContextEngine` has `clear` and `evict` methods that call `shm.free` on allocations (F-03).
- [x] `wait_for_record` in `crates/vac_approvals/src/lib.rs` is `pub async fn` (F-02).
- [x] `wait_for_intent` is implemented in `crates/vac_approvals/src/lib.rs` and consumed by `wait_for_approval_intent` (F-02).
- [x] `wait_for_approval_intent` in `crates/vac_runtime/src/autopilot.rs` uses `tokio::select!` with event-driven wait instead of 200ms loop (F-02).
- [x] `ApprovalStore::remove_by_session` cleans up record notifiers (F-02).
- [x] `ShmArena::as_ptr` no longer uses `blocking_read()` and is safe for async context (F-05).
- [x] `ShmArena::{write, read}` use `checked_add` to prevent overflow panic (F-07).
- [x] `LlmRouter::complete` loop uses `RetryDecision::{Retry, GiveUp}` instead of manual checking (F-04).
