# Tasks

- [x] Task 1: Fix F-01 and F-06 - Implement keyword retrieval in `ContextIndex::search` and seed `attention_weight`
  - [x] SubTask 1.1: Refactor `ContextIndex::search` in `crates/vil_context/src/engine.rs` to use keyword extraction and overlap scoring.
  - [x] SubTask 1.2: Update `ContextIndex::add_entry` in `crates/vil_context/src/engine.rs` to use `entry.attention_weight` instead of flat `1.0`.
  - [x] SubTask 1.3: Add unit tests for retrieval behavior and scoring in `crates/vil_context/src/engine.rs`.
- [x] Task 2: Fix F-03 - Add SHM eviction path to `ContextEngine`
  - [x] SubTask 2.1: Add `drain_entries` and `remove_entry` to `ContextIndex` in `crates/vil_context/src/engine.rs`.
  - [x] SubTask 2.2: Implement `clear` and `evict` methods on `ContextEngine` that call `shm.free` in `crates/vil_context/src/engine.rs`.
  - [x] SubTask 2.3: Update `ContextEngine::ingest` or add `ingest_source` to handle re-ingestion without leaking.
  - [x] SubTask 2.4: Add tests for SHM memory release in `crates/vil_context/src/engine.rs`.
- [x] Task 3: Fix F-02 - Migrate `wait_for_approval_intent` to event-driven API
  - [x] SubTask 3.1: Expose `wait_for_record` as `pub async fn` in `crates/vac_approvals/src/lib.rs`.
  - [x] SubTask 3.2: Implement `wait_for_intent` helper in `crates/vac_approvals/src/lib.rs`.
  - [x] SubTask 3.3: Clean up notifiers in `ApprovalStore::remove_by_session` in `crates/vac_approvals/src/lib.rs`.
  - [x] SubTask 3.4: Refactor `wait_for_approval_intent` in `crates/vac_runtime/src/autopilot.rs` to use `wait_for_intent` without polling.
  - [x] SubTask 3.5: Add unit tests for `wait_for_record` and `wait_for_intent`.
- [x] Task 4: Fix F-05 and F-07 - SHM safety improvements
  - [x] SubTask 4.1: Refactor `ShmArena::as_ptr` to `as_ptr_async` or `with_read` in `crates/vil_context/src/shm.rs` to remove `blocking_read`. Update consumers in `ContextEngine`.
  - [x] SubTask 4.2: Replace `offset + len` with `checked_add` in `ShmArena::{write, read}` guards in `crates/vil_context/src/shm.rs` to prevent integer overflow.
- [x] Task 5: Fix F-04 - Migrate `resolve_retry_delay_ms` caller
  - [x] SubTask 5.1: Refactor `LlmRouter::complete` in `crates/vil_llm/src/router.rs` to use `next_retry_decision`.
  - [x] SubTask 5.2: Add unit tests in `crates/vil_llm/src/router.rs`.

# Task Dependencies
- [Task 1] depends on nothing
- [Task 2] depends on nothing
- [Task 3] depends on nothing
- [Task 4] depends on nothing
- [Task 5] depends on nothing
